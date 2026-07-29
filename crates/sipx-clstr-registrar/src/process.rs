//! The REGISTER decision function.
//!
//! Pure: the command in, the current binding set in, the policy in; an [`Outcome`] out. No clock,
//! no socket, no store handle — which is what lets every vector in
//! [location-service §9](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)
//! run as an ordinary unit test, and lets the CAS driver loop re-run it after a conflict without
//! re-doing anything else.
//!
//! Steps run in the order §5.1 fixes, and the first failure terminates with nothing committed.
//! That ordering is normative, not stylistic: RFC 3261 §10.3 requires a REGISTER to be processed
//! "completely or not at all".

use std::time::Duration;

use crate::binding::{BindingSet, Timestamp};
use crate::command::{
    Accepted, ContactOp, ContactOps, Outcome, RegisterCommand, Rejection, TenantPolicy,
    binding_for, describe,
};

/// The option tags this registrar implements, for S2.
const SUPPORTED: &[&str] = &["path"];

/// Decide what a REGISTER does, without doing it.
///
/// `current` is the set as read from the store. On [`Outcome::Commit`] the driver commits the
/// returned set under the revision it read, and on conflict re-reads and calls this again.
#[must_use]
pub fn process(cmd: &RegisterCommand, current: &BindingSet, policy: &TenantPolicy) -> Outcome {
    // S2 — Require. Answered before anything is looked at: a registrar that acted on a request it
    // only partly understood would be guessing at the part it did not.
    let unsupported: Vec<String> = cmd
        .require
        .iter()
        .filter(|tag| !SUPPORTED.contains(&tag.to_ascii_lowercase().as_str()))
        .cloned()
        .collect();
    if !unsupported.is_empty() {
        return Outcome::Reject(Rejection::BadExtension(unsupported));
    }

    // §5.6 — Path from a UAC that did not advertise it. Committing the binding would make the UA
    // reachable only through a route set it does not know exists, so this platform refuses where
    // RFC 3327 leaves it to local policy.
    if !cmd.path.is_empty() && !cmd.supports_path {
        return Outcome::Reject(Rejection::ExtensionRequired("path"));
    }

    match &cmd.contacts {
        ContactOps::Wildcard => wildcard(cmd, current),
        ContactOps::Explicit(ops) => explicit(cmd, current, policy, ops),
    }
}

/// §5.4 — `Contact: *`.
fn wildcard(cmd: &RegisterCommand, current: &BindingSet) -> Outcome {
    // W2. The wildcard's whole meaning is "remove everything"; without an explicit `Expires: 0`
    // the request is asking for something the grammar cannot express, and §10.3 step 6 says to
    // reject rather than interpret.
    if cmd.expires_header != Some(0) {
        return Outcome::Reject(Rejection::BadRequest(
            "a wildcard Contact requires an explicit Expires: 0",
        ));
    }
    // W1 is enforced where the request is parsed — a `*` alongside a URI is a malformed header
    // set, not a decision. See `parse::register_command`.

    let mut set = current.clone();
    set.drop_expired(cmd.now);

    // W3 — the same ordering check as B2/B3/B5, applied to every binding at once.
    for binding in set.all() {
        if binding.call_id == cmd.call_id && cmd.cseq <= binding.cseq {
            return Outcome::Reject(Rejection::StaleSequence);
        }
    }

    if set.all().is_empty() {
        // Nothing to remove. Not a failure, and not a write either: the requested state already
        // holds, which is exactly B4's idempotency rule applied to a wildcard.
        return Outcome::Noop {
            response: Accepted::default(),
        };
    }

    Outcome::Commit {
        set: BindingSet::new(),
        response: Accepted::default(),
    }
}

/// §5.2, §5.3, §5.5 — explicit `Contact` values.
fn explicit(
    cmd: &RegisterCommand,
    current: &BindingSet,
    policy: &TenantPolicy,
    ops: &[ContactOp],
) -> Outcome {
    // RFC 3261 §10.2.3: a REGISTER with no Contact is a query. It answers with the complete set
    // and changes nothing.
    if ops.is_empty() {
        return Outcome::Noop {
            response: describe(current, cmd.now),
        };
    }

    // S7 — expiry selection for every contact, before any mutation. E6 fails the *whole* request,
    // so it has to be decided for all contacts before the first one is applied; otherwise a
    // too-brief second contact would leave the first one committed.
    let mut granted = Vec::with_capacity(ops.len());
    for op in ops {
        let requested = op
            .expires // E1
            .or(cmd.expires_header) // E2
            .unwrap_or(policy.default_expires); // E3

        if requested == 0 {
            granted.push(0); // E4 — removal, never subject to E5 or E6.
            continue;
        }
        if requested < policy.min_expires {
            // E6 — and the response must state the minimum, or the UA cannot correct itself.
            return Outcome::Reject(Rejection::IntervalTooBrief {
                min: policy.min_expires,
            });
        }
        // E5 — silently lowered. §10.3 step 7 allows shortening, and the response states what was
        // actually granted, so the UA is not misled about when to refresh.
        granted.push(requested.min(policy.max_expires));
    }

    let mut set = current.clone();
    set.drop_expired(cmd.now);
    let mut changed = false;

    // Parse each stored contact **once** for the whole reconciliation (`RG-14`). The hot loop used
    // to re-parse every stored binding against every incoming op — `O(contacts · bindings · quota)`
    // parses, and against an open registrar that is a CPU amplifier an attacker can tune. The parse
    // is the expensive part; equivalence on two already-parsed URIs is not. A binding whose stored
    // contact will not parse is left unparsed and will match nothing, which is the same answer the
    // old code gave it.
    let parsed: Vec<Option<sipx_sip::Uri>> = set
        .all()
        .iter()
        .map(|binding| {
            #[cfg(debug_assertions)]
            CONTACT_PARSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            sipx_sip::Uri::parse(binding.contact.clone()).ok()
        })
        .collect();

    for (op, granted) in ops.iter().zip(granted.iter().copied()) {
        // §5.3 — binding identity by §19.1.4 comparison. Equivalence is non-transitive, so an
        // incoming contact can match more than one stored binding; the first in creation order is
        // the one updated, which is a deterministic choice a vector can assert.
        // Equivalence is non-transitive, so an incoming contact can match more than one stored
        // binding; the first in creation order is the one updated, which is a deterministic choice a
        // vector can assert. Matching reads the parsed view computed above, so the cost per op is a
        // scan of `equivalent` calls, not `bindings` parses.
        let matched = parsed.iter().position(|stored| {
            stored
                .as_ref()
                .is_some_and(|stored| stored.equivalent(&op.uri))
        });

        match matched {
            None => {
                if granted == 0 {
                    continue; // B1 — removing a contact that is not there is not an error.
                }
                set.insert(binding_for(op, cmd, granted, cmd.now));
                changed = true;
            }
            Some(index) => {
                let Some(stored) = set.all().get(index) else {
                    continue;
                };

                if stored.call_id == cmd.call_id {
                    if cmd.cseq < stored.cseq {
                        // B5 — the ordering token went backwards. Abort everything.
                        return Outcome::Reject(Rejection::StaleSequence);
                    }
                    if cmd.cseq == stored.cseq {
                        // B4 — an idempotent retry, but only if the stored state already *is* what
                        // this command asks for. Same token with a different request is not a
                        // retry; it is a second write under a spent token, which B5 refuses.
                        if already_holds(stored, cmd, granted) {
                            continue;
                        }
                        return Outcome::Reject(Rejection::StaleSequence);
                    }
                    // B3 — newer CSeq, same Call-ID: apply.
                } // B2 — a different Call-ID applies regardless of CSeq: the UA restarted, and its
                // sequence numbering restarted with it.

                if granted == 0 {
                    set.remove(index);
                } else {
                    let registered_at = stored.registered_at;
                    set.replace(index, binding_for(op, cmd, granted, registered_at));
                }
                changed = true;
            }
        }
    }

    // S8 — the quota, checked against the *committed outcome* rather than the request, so a
    // refresh or a removal can never trip it.
    if set.active_count(cmd.now) > policy.max_bindings_per_aor {
        return Outcome::Reject(Rejection::Forbidden(
            "the address-of-record already holds its maximum bindings",
        ));
    }

    let response = describe(&set, cmd.now);
    if changed {
        Outcome::Commit { set, response }
    } else {
        Outcome::Noop { response }
    }
}

/// How many times a stored contact has been parsed during reconciliation (`RG-14`).
///
/// An instrument, not part of the contract: the test that proves the contact path is linear counts
/// parses rather than wall-clock time, which would be flaky. `release` builds do not carry it.
#[cfg(debug_assertions)]
pub static CONTACT_PARSES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Whether the stored binding already is what this command asks for — B4's precise condition.
fn already_holds(stored: &crate::binding::Binding, cmd: &RegisterCommand, granted: u32) -> bool {
    if granted == 0 {
        // A removal whose target is still present has not been applied.
        return false;
    }
    // §5.3.1 B4.1 — the base is the granted **duration**, not the absolute deadline. `now` is
    // stamped when a request is admitted, so two deliveries of one REGISTER never share one: a UDP
    // retransmission after a lost `200`, a CAS re-read and a re-presentation at a second node all
    // arrive later than the delivery that wrote the binding. Comparing deadlines would call every
    // retry that actually happens a second write and refuse it (B5), which is how this rule came to
    // answer an ordinary retransmission `500`.
    //
    // What the caller does with a `true` is `continue` — no mutation at all (B4.2). The deadline is
    // deliberately *not* pushed out: a retry that refreshed it would let one ordering token buy a
    // second lifetime.
    stored.refreshed_at.until(stored.expires_at) == Duration::from_secs(u64::from(granted))
        && stored.path == cmd.path
}

/// Whether a set is empty of active bindings at `now` — for the driver's housekeeping decisions.
#[must_use]
pub fn is_empty_at(set: &BindingSet, now: Timestamp) -> bool {
    set.active_count(now) == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rg14_tests {
    use super::*;
    use crate::binding::{Binding, Timestamp};
    use crate::command::{ContactOp, ContactOps, RegisterCommand};
    use bytes::Bytes;
    use sipx_sip::Uri;

    fn aor(uri: &str) -> crate::CanonicalAor {
        crate::CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    fn op(contact: &str) -> ContactOp {
        ContactOp {
            uri: Uri::parse(Bytes::copy_from_slice(contact.as_bytes())).expect("a valid contact"),
            verbatim: Bytes::copy_from_slice(contact.as_bytes()),
            expires: Some(3600),
            q: None,
            instance_id: None,
            reg_id: None,
            push: None,
        }
    }

    fn command(contacts: Vec<ContactOp>) -> RegisterCommand {
        RegisterCommand {
            tenant: "t".to_owned(),
            aor: aor("sip:alice@example.test"),
            call_id: Bytes::from_static(b"rg14-call"),
            cseq: 1,
            contacts: ContactOps::Explicit(contacts),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(1000),
        }
    }

    /// **`RG-14`'s failing-first test.** The number of times a *stored* contact is parsed during one
    /// reconciliation must be the number of stored bindings, not (ops × bindings). Before this
    /// story, matching re-parsed every stored contact for every incoming op, so a REGISTER with many
    /// contacts against an address-of-record holding many bindings cost `O(ops · bindings)` parses —
    /// and the quota that would cap the result was applied after all of that work was done.
    ///
    /// Counted, not timed: wall-clock would be flaky, a parse is the expensive unit.
    #[test]
    fn rg14_reconciliation_parses_each_stored_contact_once() {
        use std::sync::atomic::Ordering::Relaxed;

        // Ten bindings already stored for this address-of-record.
        let mut current = BindingSet::new();
        for i in 0..10 {
            current.insert(binding(&format!("sip:alice@10.0.0.{i}:5060")));
        }

        // And a REGISTER carrying ten contacts.
        let contacts: Vec<ContactOp> = (0..10)
            .map(|i| op(&format!("sip:alice@198.51.100.{i}:5060")))
            .collect();
        let cmd = command(contacts);

        let before = CONTACT_PARSES.load(Relaxed);
        let _ = process(&cmd, &current, &TenantPolicy::default());
        let parses = CONTACT_PARSES.load(Relaxed) - before;

        assert_eq!(
            parses, 10,
            "one parse per stored binding, not one per (op × binding); got {parses}"
        );
    }

    /// An op that matches a stored binding still resolves, through the precomputed view rather than
    /// a fresh parse of every candidate.
    #[test]
    fn rg14_a_matching_contact_still_updates_the_same_binding() {
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.5:5060"));

        let cmd = command(vec![op("sip:alice@10.0.0.5:5060")]);
        let outcome = process(&cmd, &current, &TenantPolicy::default());

        // A refresh of the same contact must not insert a second one. The active count is read
        // back out of the set that comes back, however it came back.
        match outcome {
            Outcome::Commit { set, .. } => {
                assert_eq!(set.active(cmd.now).count(), 1, "a refresh updates in place");
            }
            Outcome::Noop { response } => {
                let _ = response;
                // No change is also a correct answer for a refresh that is already the committed
                // state — but the stored set must still hold exactly one active binding.
                assert_eq!(current.active(cmd.now).count(), 1);
            }
            Outcome::Reject(reason) => panic!("a refresh must not be refused: {reason:?}"),
        }
    }

    fn binding(contact: &str) -> Binding {
        Binding {
            contact: Bytes::copy_from_slice(contact.as_bytes()),
            q: 1000,
            call_id: Bytes::from_static(b"other"),
            cseq: 1,
            expires_at: Timestamp::from_secs(3600),
            registered_at: Timestamp::from_secs(1),
            refreshed_at: Timestamp::from_secs(1),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }
}
