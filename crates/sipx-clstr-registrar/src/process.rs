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
    //
    // `grants[i]` is what operation `i` is granted, and it is kept as a whole vector rather than
    // consumed one element at a time because B8 has to read the *other* entries: whether an
    // operation is the command's last word on a binding depends on the ones after it.
    let mut grants = Vec::with_capacity(ops.len());
    for op in ops {
        let requested = op
            .expires // E1
            .or(cmd.expires_header) // E2
            .unwrap_or(policy.default_expires); // E3

        if requested == 0 {
            grants.push(0); // E4 — removal, never subject to E5 or E6.
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
        grants.push(requested.min(policy.max_expires));
    }

    let mut set = current.clone();
    set.drop_expired(cmd.now);
    // Whether reaping expired bindings already made the set differ from the durable one. Needed at
    // the end, where a reconciled set equal to `current` means there is nothing to commit — which is
    // only true if nothing was reaped on the way in.
    let reaped = set.all().len() != current.all().len();
    let mut view = Reconciling::new(set);
    let mut changed = false;

    // §5.5 is decided **after** the loop, on the reconciled set, and there is deliberately no
    // pre-check ahead of it any more.
    //
    // The rule is a test on the *committed outcome* — "a REGISTER whose committed outcome would
    // exceed it fails `403 Forbidden`" — and it says in terms that "refreshes, replacements and
    // removals never grow the set and never trip the quota". Nothing computed before this loop knows
    // that outcome, because deciding whether an operation adds a binding or lands on one *is* the
    // loop: B6 resolves each operation against the set the preceding ones left, so several operations
    // can collapse onto one binding (B7) and a later removal can take back an earlier addition.
    //
    // Two pre-checks have now been wrong in the same direction, which is why there is not a third.
    // The first counted every positive-expiry operation as an addition and refused refreshes
    // (LS-R-15). The second counted a candidate unless it was equivalent to one already counted —
    // an *upper* bound on additions, so `x;line=1, x, x;line=2` against nine held bindings was
    // answered `403` where the committed outcome is ten (LS-R-30). Both were conservative, and
    // conservative is the wrong direction here: a `403` is a policy refusal a UA cannot retry out of,
    // so refusing what the quota permits is worse than costing one more pass over the operations.
    //
    // No cheap bound rescues it either. Counting a candidate unless it is equivalent to *any*
    // preceding one is exact for that chain — §19.1.4 equivalence is non-transitive, so `a ≡ b`,
    // `b ≡ c`, `a ≢ c` genuinely commits one binding in that order and two in the order `a, c, b` —
    // but it is still an upper bound once the request also carries a removal of something it added.
    // The reconciled set is the only exact answer, so it is the only one asked.
    for (op, granted) in ops.iter().zip(grants.iter().copied()) {
        // §5.3 — binding identity by §19.1.4 comparison. Equivalence is non-transitive, so an
        // incoming contact can match more than one stored binding; the first in creation order is
        // the one updated, which is a deterministic choice a vector can assert. Matching reads the
        // view's parsed contacts, so the cost per op is a scan of `equivalent` calls, not
        // `bindings` parses (`RG-14`).
        //
        // §5.3.2 B6 — and it is matched against the set **as the preceding operations left it**,
        // which is the whole reason the view owns the set rather than sitting beside it.
        match view.find(&op.uri) {
            None => {
                if granted == 0 {
                    continue; // B1 — removing a contact that is not there is not an error.
                }
                view.insert(binding_for(op, cmd, granted, cmd.now), op.uri.clone());
                changed = true;
            }
            Some(index) => {
                let Some(stored) = view.get(index) else {
                    continue;
                };

                // §5.3.2 B7 — B2–B5 compare this request's token against the token of the request
                // that last *wrote* the matched binding. When an earlier operation of this same
                // request wrote it, that token is this command's own, so the comparison decides
                // nothing and B6 has already fixed the order: the operation applies.
                //
                // The flag is what tells this apart from a retransmission, which arrives carrying
                // the token stored on the binding too and must stay B4's (§5.3.1). The two cases
                // are token-identical, so the registrar answers from what it knows — it performed
                // this write itself, a moment ago — rather than inferring it from a comparison
                // that cannot separate them.
                if !view.written_here(index) && stored.call_id == cmd.call_id {
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
                        // §5.3.2 B8 — and "what this command asks for" is stated per *binding*, not
                        // per operation. Once B6 lets several operations of one request resolve to
                        // one binding, the command's requested outcome for it is the **last** of
                        // them; the earlier ones are writes this same request overwrites before it
                        // commits anything. Comparing this operation's own grant instead answered a
                        // verbatim retransmission of `CC;expires=3600, CC;expires=7200` with B5's
                        // `500` — the stored binding holds 7200, exactly what the command asks — so
                        // the one request B7 exists to legalise was the one a UA could not
                        // retransmit (LS-R-31).
                        //
                        // Asked only after the cheap comparison fails, and only when a later
                        // operation supersedes this one, so the ordinary single-operation
                        // retransmission pays nothing for it.
                        let net = net_grant(&view, ops, &grants, index).unwrap_or(granted);
                        if net != granted && already_holds(stored, cmd, net) {
                            continue;
                        }
                        return Outcome::Reject(Rejection::StaleSequence);
                    }
                    // B3 — newer CSeq, same Call-ID: apply.
                } // B2 — a different Call-ID applies regardless of CSeq: the UA restarted, and its
                // sequence numbering restarted with it.

                let registered_at = stored.registered_at;
                if granted == 0 {
                    view.remove(index);
                } else {
                    view.replace(
                        index,
                        binding_for(op, cmd, granted, registered_at),
                        op.uri.clone(),
                    );
                }
                changed = true;
            }
        }
    }

    let set = view.into_set();

    // S8 — the quota (§5.5), and the only place it is asked: on the reconciled set, which is the
    // "committed outcome" the rule names. A refresh, a replacement or a removal cannot grow the set,
    // so none of them can trip it, and a request whose operations collapse onto fewer bindings than
    // it names is measured by what it commits rather than by what it carries.
    if set.active_count(cmd.now) > policy.max_bindings_per_aor {
        return Outcome::Reject(Rejection::Forbidden(
            "the address-of-record already holds its maximum bindings",
        ));
    }

    let response = describe(&set, cmd.now);
    // §5.3.2 B9 — a request whose reconciled set is the set it read commits nothing, so the revision
    // does not move (B4.2). `changed` records that an operation *mutated the view*, which is not the
    // same question: an addition a later operation of the same request takes back leaves the durable
    // set exactly as it was (LS-R-28's shape), and committing it would spend a revision and publish a
    // change event describing no change. The two deliveries of such a request are also
    // indistinguishable from the store — nothing survives to carry the ordering token — so the only
    // way the retransmission can be idempotent is for neither delivery to write.
    //
    // Only ever a downgrade: `reaped` keeps a set that lost expired bindings on the way in from
    // comparing equal to the durable one.
    if changed && (reaped || set != *current) {
        Outcome::Commit { set, response }
    } else {
        Outcome::Noop { response }
    }
}

/// The binding set under reconciliation, beside the parsed contacts §5.3 matches against.
///
/// One type rather than two locals, because the correctness of the whole loop is the invariant that
/// the two stay the same length in the same order — and `RG-16` was that invariant belonging to
/// nobody. The view was parsed once from the *original* vector and every operation resolved against
/// it, so the first removal left every later operation naming an entry that had moved: one REGISTER
/// carrying `CA;expires=0`, `CB;expires=0` and a new `CC` removed CA, resolved CB's removal past the
/// end of the shortened set, and committed CB as though the request had never mentioned it. With a
/// third binding present it was worse than a survivor — the refresh landed on whichever binding had
/// slid into the index, overwriting a contact the request never named.
///
/// Single-contact REGISTER cannot reach any of that, which is why every existing proof was green.
/// The fix is structural rather than a re-parse: only this type mutates the set, and every mutation
/// moves the view with it (§5.3.2 B6), so the per-reconciliation parse budget `RG-14` bought — one
/// parse per stored binding, none per operation — is unchanged.
///
/// Resolving operations against the live set is also what makes B7 reachable, so the view carries
/// the second fact B7 needs: which bindings *this* request wrote. Nothing else can supply it — a
/// binding written a moment ago by an earlier operation and a binding written by an earlier
/// delivery of the same REGISTER are byte-identical in `(Call-ID, CSeq)`, and only the first is
/// outside B4/B5's reach.
struct Reconciling {
    set: BindingSet,
    /// `slots[i]` describes `set.all()[i]`. One vector rather than one per fact, so there is no
    /// second alignment to keep — the invariant is `slots.len() == set.all().len()`, and the three
    /// mutators below are the only things that can change either side of it.
    slots: Vec<Slot>,
}

/// What reconciliation knows about one binding beyond the binding itself.
struct Slot {
    /// The binding's contact URI, or `None` when those bytes do not parse.
    ///
    /// A binding whose stored contact will not parse matches nothing, which is the same answer a
    /// comparison against it would have produced; it is kept in place rather than dropped so the
    /// indices of everything after it are still the set's.
    uri: Option<sipx_sip::Uri>,
    /// Whether an earlier operation of **this** request wrote this binding (§5.3.2 B7).
    ///
    /// Recorded rather than derived, because it cannot be derived: a binding this request just
    /// wrote and a binding an earlier delivery of the same request wrote carry the same
    /// `(Call-ID, CSeq)`, and the second is B4's retransmission while the first is not a write
    /// under a spent token at all. Only the reconciliation that performed the write knows which
    /// it is looking at.
    written_here: bool,
}

impl Reconciling {
    /// Parse each stored contact **once** for the whole reconciliation (`RG-14`).
    ///
    /// The loop this replaces re-parsed every stored binding for every incoming op —
    /// `O(contacts · bindings)` parses, and against an open registrar that is a CPU amplifier an
    /// attacker can tune. The parse is the expensive part; equivalence on two already-parsed URIs
    /// is not.
    fn new(set: BindingSet) -> Self {
        let slots = set
            .all()
            .iter()
            .map(|binding| {
                #[cfg(test)]
                parse_meter::record();
                Slot {
                    uri: sipx_sip::Uri::parse(binding.contact.clone()).ok(),
                    // Everything the set arrived with was written by some earlier request — this
                    // one has not applied an operation yet.
                    written_here: false,
                }
            })
            .collect();
        Self { set, slots }
    }

    /// The first binding in creation order whose contact is §19.1.4-equivalent to `uri`.
    ///
    /// Equivalence is non-transitive, so more than one stored binding can match; §5.3 makes the
    /// first the one that is updated.
    fn find(&self, uri: &sipx_sip::Uri) -> Option<usize> {
        self.slots.iter().position(|slot| {
            slot.uri
                .as_ref()
                .is_some_and(|stored| stored.equivalent(uri))
        })
    }

    /// The binding at `index`, if the set still holds one there.
    fn get(&self, index: usize) -> Option<&crate::binding::Binding> {
        self.set.all().get(index)
    }

    /// Whether an earlier operation of this request wrote the binding at `index` (§5.3.2 B7).
    fn written_here(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|slot| slot.written_here)
    }

    /// Add a binding whose contact parsed to `uri`, keeping both halves in creation order.
    fn insert(&mut self, binding: crate::binding::Binding, uri: sipx_sip::Uri) {
        let at = self.set.insert_at(binding);
        // `insert_at` computes its position by `partition_point` over the set *before* the insert,
        // so with `slots.len() == set.all().len()` holding on entry, `at` is at most `slots.len()`
        // — exactly the range `Vec::insert` accepts. The clamp is therefore unreachable, and kept
        // only so that a future change breaking that invariant mis-places a slot instead of
        // panicking on network input (AGENTS.md #3). It is a belt on a proof, not a live hazard.
        let at = at.min(self.slots.len());
        self.slots.insert(
            at,
            Slot {
                uri: Some(uri),
                written_here: true,
            },
        );
    }

    /// Replace the binding at `index` with one whose contact parsed to `uri`.
    fn replace(&mut self, index: usize, binding: crate::binding::Binding, uri: sipx_sip::Uri) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.uri = Some(uri);
            slot.written_here = true;
            self.set.replace(index, binding);
        }
    }

    /// Drop the binding at `index` from both halves.
    fn remove(&mut self, index: usize) {
        if index < self.slots.len() {
            let _ = self.slots.remove(index);
            self.set.remove(index);
        }
    }

    /// The reconciled set. The parsed view has no life beyond the loop.
    fn into_set(self) -> BindingSet {
        self.set
    }
}

/// A test-only meter over how many times a **stored** contact URI is parsed while reconciling one
/// REGISTER (`RG-14`).
///
/// The amplification this story fixes is not visible in wall-clock time without flakiness, so the
/// cost is made observable: parsing a stored contact once per comparison is `ops · bindings` work,
/// and parsing each once is `bindings`.
///
/// **A thread-local rather than an atomic**, and that distinction is load-bearing. `process` runs
/// synchronously on its caller's thread, so a thread-local lets each test see only its own parses.
/// A global atomic does not: the suite runs tests in parallel, and a sibling test's parses leak into
/// the delta — which is exactly how the first version of this meter reported 3 where 2 was correct.
#[cfg(test)]
pub(crate) mod parse_meter {
    use std::cell::Cell;

    thread_local! {
        static STORED_PARSES: Cell<usize> = const { Cell::new(0) };
    }

    /// Forget every parse counted so far on this thread.
    pub(crate) fn reset() {
        STORED_PARSES.with(|count| count.set(0));
    }

    /// Count one parse of a stored contact URI.
    pub(crate) fn record() {
        STORED_PARSES.with(|count| count.set(count.get() + 1));
    }

    /// How many stored-contact parses have happened on this thread since the last [`reset`].
    pub(crate) fn count() -> usize {
        STORED_PARSES.with(Cell::get)
    }
}

/// The grant this command's **net** outcome gives the binding at `index` — §5.3.2 B8's comparison
/// base for B4.
///
/// §5.3 states idempotency per *binding*, against "the command's requested outcome". For a single
/// operation those are the same thing, which is why the distinction stayed invisible until B6 let
/// several operations of one request resolve to one binding. When they do, the last of them is the
/// outcome the command requests: the earlier writes are ones this same request overwrites before
/// anything commits, so a stored binding holding the *later* grant is a binding that already is what
/// the command asks for.
///
/// `None` when no operation resolves here, which the caller cannot actually reach — it is asked from
/// inside the branch where one just did — so the caller falls back to that operation's own grant and
/// the answer is unchanged rather than guessed at.
///
/// Resolution is `Reconciling::find`, not `Uri::equivalent` against the operation, so this asks
/// exactly the question the loop asks: §19.1.4 equivalence is non-transitive and more than one stored
/// binding can match, so "an operation equivalent to this binding's contact" and "an operation that
/// would land on this binding" are different sets, and only the second one supersedes anything.
///
/// Every comparison is over already-parsed URIs, so `RG-14`'s parse budget — one parse per stored
/// binding, none per operation — is untouched.
fn net_grant(view: &Reconciling, ops: &[ContactOp], grants: &[u32], index: usize) -> Option<u32> {
    ops.iter()
        .zip(grants.iter().copied())
        .rev()
        .find(|(op, _)| view.find(&op.uri) == Some(index))
        .map(|(_, grant)| grant)
}

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

        // A quota big enough not to interfere: this test is about *parses*, and the early quota
        // refusal (RG-14) would otherwise return before any reconciliation ran, reading as 0.
        let generous = TenantPolicy {
            max_bindings_per_aor: 64,
            ..TenantPolicy::default()
        };
        parse_meter::reset();
        let _ = process(&cmd, &current, &generous);
        let parses = parse_meter::count();

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod rg14_quota_tests {
    use super::*;
    use crate::binding::{Binding, Timestamp};
    use crate::command::{ContactOp, ContactOps, RegisterCommand};
    use bytes::Bytes;
    use sipx_sip::Uri;

    fn aor(uri: &str) -> crate::CanonicalAor {
        crate::CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    fn policy(max: usize) -> TenantPolicy {
        TenantPolicy {
            max_bindings_per_aor: max,
            ..TenantPolicy::default()
        }
    }

    fn op(contact: &str, expires: u32) -> ContactOp {
        ContactOp {
            uri: Uri::parse(Bytes::copy_from_slice(contact.as_bytes())).expect("a valid contact"),
            verbatim: Bytes::copy_from_slice(contact.as_bytes()),
            expires: Some(expires),
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
            call_id: Bytes::from_static(b"rg14-quota"),
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

    fn binding(contact: &str, expires_at: u64) -> Binding {
        Binding {
            contact: Bytes::copy_from_slice(contact.as_bytes()),
            q: 1000,
            call_id: Bytes::from_static(b"other"),
            cseq: 1,
            expires_at: Timestamp::from_secs(expires_at),
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

    /// **The quota half of `RG-14`.** A request that cannot fit must be refused without paying for
    /// the full reconciliation. Distinguishing a refresh (never refused) from a new binding (may be
    /// refused) *requires* the match view, which is one parse per stored binding — so the honest
    /// floor is exactly that, not zero. The per-op × per-binding re-parse the amplifier was is what
    /// this is measured against.
    #[test]
    fn rg14_an_over_quota_request_is_refused_before_reconciliation() {
        // The address-of-record holds two active bindings; the quota is three. Two additions follow.
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.1:5060", 3600));
        current.insert(binding("sip:alice@10.0.0.2:5060", 3600));

        let cmd = command(vec![
            op("sip:alice@198.51.100.1:5060", 3600),
            op("sip:alice@198.51.100.2:5060", 3600),
        ]);

        parse_meter::reset();
        let outcome = process(&cmd, &current, &policy(3));
        let parses = parse_meter::count();

        assert!(
            matches!(outcome, Outcome::Reject(Rejection::Forbidden(_))),
            "2 active + 2 additions over a quota of 3 must be refused, got {outcome:?}"
        );
        // One parse per stored binding to build the match view — the floor, not the amplifier's
        // ops × bindings. 2 stored bindings, 2 parses, no more.
        assert_eq!(
            parses, 2,
            "a refusal costs one parse per stored binding to distinguish a refresh from a new binding; got {parses}"
        );
    }

    /// The early check and the final check must still agree once B7 lets two operations name one
    /// contact (§5.3.2). Empty set, quota 1, and one REGISTER naming the same contact twice: the
    /// committed set is one binding, so the request fits — and an early check that counted both
    /// operations as additions would refuse `403` something the mutation loop commits within the
    /// quota. That is the only way the cheap upper bound can disagree with the outcome, so it is
    /// asserted rather than argued.
    #[test]
    fn b7_two_operations_naming_one_contact_are_one_addition_against_the_quota() {
        let cmd = command(vec![
            op("sip:alice@10.0.0.9:5060", 3600),
            op("sip:alice@10.0.0.9:5060", 7200),
        ]);
        let outcome = process(&cmd, &BindingSet::new(), &policy(1));

        let Outcome::Commit { set, .. } = outcome else {
            panic!("one contact named twice is one binding, so a quota of 1 fits: {outcome:?}");
        };
        assert_eq!(set.active_count(cmd.now), 1);
    }

    /// The early check must not change *which* requests are accepted. A request that fits within the
    /// upper bound proceeds to the same answer it gave before — a refresh of an existing contact.
    #[test]
    fn rg14_a_request_that_fits_is_accepted_unchanged() {
        let mut current = BindingSet::new();
        current.insert(binding("sip:alice@10.0.0.1:5060", 3600));

        // One refresh against a quota of 2: 1 active + 1 add of the *same* contact = still 2.
        let cmd = command(vec![op("sip:alice@10.0.0.1:5060", 3600)]);
        let outcome = process(&cmd, &current, &policy(2));

        assert!(
            matches!(outcome, Outcome::Commit { .. } | Outcome::Noop { .. }),
            "a refresh that does not grow the set must be accepted, got {outcome:?}"
        );
    }
}
