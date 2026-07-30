//! The `LocationStore` contract as a runnable suite — one implementation, every backend.
//!
//! [location-service §6.3](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)
//! says the in-memory backend is what the vectors run against first and that PostgreSQL "passes the
//! identical suite". *Identical* is the word that matters: a suite copied per backend drifts, and the
//! copy that drifts is the one that stops catching things. So the rows live here, take a
//! `&dyn LocationStore`, and both backends call the same function.
//!
//! Behind the `test-suite` feature because it is test code that ships in a library: `RG-4`'s backend
//! lives in the driver crate (IO belongs there), and it cannot reach a `#[cfg(test)]` module in this
//! one.
//!
//! Every failure names the row and the backend, because "LS-R-6 failed on postgres" is a bug report
//! and "assertion failed" is not.

use bytes::Bytes;
use sipx_sip::Uri;

use crate::aor::CanonicalAor;
use crate::binding::{Revision, Timestamp};
use crate::command::{ContactOp, ContactOps, Outcome, RegisterCommand, TenantPolicy};
use crate::store::{LocationStore, apply};

const RETRIES: usize = 3;

/// The delay LS-R-3 states between a REGISTER and its retransmission.
///
/// A number rather than zero because a retry at the originating nanosecond proves nothing: it is
/// the one case a deadline comparison also accepts, and the one case that never happens on a real
/// network.
const RETRANSMIT_DELAY_NANOS: u64 = 500_000_000;

/// What went wrong, and where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{backend}: {row} — {detail}")]
pub struct Failure {
    /// Which backend was under test.
    pub backend: String,
    /// The spec row that failed.
    pub row: String,
    /// What was expected and what happened.
    pub detail: String,
}

struct Suite<'a> {
    store: &'a dyn LocationStore,
    backend: String,
    /// The tenant this run owns.
    ///
    /// A parameter rather than a constant so two runs can share a database without interfering. That
    /// is not a testing convenience — §5's serialization domain is `(tenant, aor)` and §6 K1 says
    /// commits for different address-of-records are unordered relative to each other, so isolating by
    /// tenant is the contract's own boundary. Sharing one would have the suite contradict the
    /// independence it is asserting.
    tenant: String,
    failures: Vec<Failure>,
}

impl Suite<'_> {
    fn check(&mut self, row: &str, ok: bool, detail: impl FnOnce() -> String) {
        if !ok {
            self.failures.push(Failure {
                backend: self.backend.clone(),
                row: row.to_owned(),
                detail: detail(),
            });
        }
    }
}

/// Run every backend-agnostic row of the `LS-R` and `LS-K` tables against one store.
///
/// The store must be empty, and the suite uses address-of-records of its own so a caller can point it
/// at a shared database without colliding with anything.
pub fn run_location_store_suite(
    store: &dyn LocationStore,
    backend: &str,
    tenant: &str,
) -> Vec<Failure> {
    let mut suite = Suite {
        store,
        backend: backend.to_owned(),
        tenant: tenant.to_owned(),
        failures: Vec::new(),
    };
    ls_r_first_registration(&mut suite);
    ls_r_refresh_and_retry(&mut suite);
    ls_r_stale_cseq(&mut suite);
    ls_r_new_call_id(&mut suite);
    ls_r_remove_one_of_two(&mut suite);
    ls_r_wildcard(&mut suite);
    ls_r_min_and_max_expires(&mut suite);
    ls_r_quota(&mut suite);
    ls_r_atomicity(&mut suite);
    ls_r_path(&mut suite);
    ls_r_expired_binding_is_absent(&mut suite);
    // §5.3.2 — the multi-contact rows. B6 first, then B7, which is only reachable once B6 holds.
    ls_r_two_removals_and_an_addition(&mut suite);
    ls_r_a_removal_ahead_of_a_refresh(&mut suite);
    ls_r_two_removals_commit_the_empty_set(&mut suite);
    ls_r_a_refresh_ahead_of_a_removal(&mut suite);
    ls_r_a_removal_of_what_this_request_added(&mut suite);
    ls_r_a_second_operation_on_what_this_request_added(&mut suite);
    // B8/B9 and §5.5 — what B6/B7 leave to decide once several operations can reach one binding: the
    // quota measured on the outcome, and the retransmission of such a request being a retry.
    ls_r_quota_measures_the_committed_outcome(&mut suite);
    ls_r_a_retransmission_of_a_contact_named_twice(&mut suite);
    ls_r_operations_that_cancel_out_commit_nothing(&mut suite);
    ls_k_cas_conflict(&mut suite);
    ls_k_revision_survives_empty(&mut suite);
    ls_k_changes(&mut suite);
    ls_l_lookup_order(&mut suite);
    suite.failures
}

// ------------------------------------------------------------------------------- fixtures ------

fn aor(name: &str) -> CanonicalAor {
    CanonicalAor::parse(Bytes::from(format!("sip:{name}@conformance.example")))
        .unwrap_or_else(|_| unreachable_aor())
}

/// The fixture names are literals that canonicalize; this is unreachable, and returning a value the
/// suite would then fail on is better than a panic inside a library.
fn unreachable_aor() -> CanonicalAor {
    CanonicalAor::parse(Bytes::from_static(b"sip:fallback@conformance.example"))
        .unwrap_or_else(|_| unreachable_aor())
}

fn contact(text: &str, expires: Option<u32>, q: Option<u16>) -> ContactOp {
    let bytes = Bytes::from(text.to_owned());
    ContactOp {
        uri: Uri::parse(bytes.clone()).unwrap_or_else(|_| {
            Uri::sip(sipx_sip::Host::Name(
                sipx_sip::HostName::new(Bytes::from_static(b"invalid.example"))
                    .unwrap_or_else(|_| unreachable_host()),
            ))
        }),
        verbatim: bytes,
        expires,
        q,
        instance_id: None,
        reg_id: None,
        push: None,
    }
}

fn unreachable_host() -> sipx_sip::HostName {
    sipx_sip::HostName::new(Bytes::from_static(b"x")).unwrap_or_else(|_| unreachable_host())
}

fn command(
    tenant: &str,
    who: &str,
    call_id: &str,
    cseq: u32,
    now_secs: u64,
    contacts: Vec<ContactOp>,
) -> RegisterCommand {
    RegisterCommand {
        tenant: tenant.to_owned(),
        aor: aor(who),
        call_id: Bytes::from(call_id.to_owned()),
        cseq,
        contacts: ContactOps::Explicit(contacts),
        expires_header: None,
        path: Vec::new(),
        supports_path: false,
        require: Vec::new(),
        received: None,
        flow_ref: None,
        principal: None,
        now: Timestamp::from_secs(now_secs),
    }
}

fn policy() -> TenantPolicy {
    TenantPolicy::default()
}

fn ca() -> ContactOp {
    contact("sip:a@10.0.0.1", Some(3_600), None)
}
fn cb() -> ContactOp {
    contact("sip:b@10.0.0.2", Some(3_600), None)
}
fn cc() -> ContactOp {
    contact("sip:c@10.0.0.3", Some(3_600), None)
}

// ------------------------------------------------------------------------------- the rows ------

fn ls_r_first_registration(suite: &mut Suite<'_>) {
    let cmd = command(&suite.tenant, "r1", "i1", 1, 0, vec![ca()]);
    let applied = apply(suite.store, &cmd, &policy(), RETRIES);
    suite.check(
        "LS-R-1",
        applied.outcome.commits() && applied.revision == Revision(1),
        || {
            format!(
                "expected a commit at revision 1, got {:?}",
                applied.revision
            )
        },
    );
    let listed = applied.outcome.accepted().map_or(0, |a| a.contacts.len());
    suite.check("LS-R-1", listed == 1, || {
        format!("expected 1 contact in the response, got {listed}")
    });
}

fn ls_r_refresh_and_retry(suite: &mut Suite<'_>) {
    let who = "r2";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );

    let refresh = command(&suite.tenant, who, "i1", 2, 10, vec![ca()]);
    let first = apply(suite.store, &refresh, &policy(), RETRIES);
    suite.check("LS-R-2", first.outcome.commits(), || {
        "B3: a newer CSeq must apply".to_owned()
    });

    let deadline = suite
        .store
        .read(&suite.tenant, &aor(who))
        .0
        .all()
        .first()
        .map(|binding| binding.expires_at);

    // B4 — the same request 500 ms later, which is what a lost `200` produces over UDP. The row
    // states the delay on purpose: comparing deadlines rather than granted durations (§5.3.1 B4.1)
    // would pass this only at zero latency.
    let mut again = refresh.clone();
    again.now = Timestamp::from_nanos(refresh.now.as_nanos() + RETRANSMIT_DELAY_NANOS);
    let again = apply(suite.store, &again, &policy(), RETRIES);
    suite.check(
        "LS-R-3",
        !again.outcome.commits() && again.revision == first.revision,
        || {
            format!(
                "a retry 500 ms later must not write: commits={} revision {:?} → {:?}",
                again.outcome.commits(),
                first.revision,
                again.revision
            )
        },
    );
    // B4.2 — no mutation means the deadline does not move either; a retry that extended the
    // binding would let one ordering token buy a second lifetime.
    let after = suite
        .store
        .read(&suite.tenant, &aor(who))
        .0
        .all()
        .first()
        .map(|binding| binding.expires_at);
    suite.check("LS-R-3", after == deadline, || {
        format!("a retry extended the binding: {deadline:?} → {after:?}")
    });

    // LS-R-22 — the same token asking for a different granted duration is not a retry.
    let before = suite.store.read(&suite.tenant, &aor(who)).1;
    let mut longer = command(
        &suite.tenant,
        who,
        "i1",
        2,
        10,
        vec![contact("sip:a@10.0.0.1", Some(7_200), None)],
    );
    longer.now = Timestamp::from_nanos(refresh.now.as_nanos() + RETRANSMIT_DELAY_NANOS);
    let longer = apply(suite.store, &longer, &policy(), RETRIES);
    suite.check("LS-R-22", longer.outcome.status() == 500, || {
        format!(
            "a different granted duration under a spent token must abort, got {}",
            longer.outcome.status()
        )
    });
    // The status alone would let a backend abort *and* commit. LS-R-4 checks the store did not
    // move for the same reason; a B5 row that does not is half a row.
    let settled = suite.store.read(&suite.tenant, &aor(who)).1;
    suite.check("LS-R-22", settled == before, || {
        format!("the store moved under an aborted request: {before:?} → {settled:?}")
    });

    // LS-R-23 — B4.3. The same spent token, plus a contact it never bound: CA is not refreshed and
    // CB is added, because B1 has no stored ordering token to be stale against.
    let mut both = command(&suite.tenant, who, "i1", 2, 10, vec![ca(), cb()]);
    both.now = Timestamp::from_nanos(refresh.now.as_nanos() + RETRANSMIT_DELAY_NANOS);
    let both = apply(suite.store, &both, &policy(), RETRIES);
    suite.check("LS-R-23", both.outcome.commits(), || {
        format!(
            "B1 must still add the unbound contact, got status {}",
            both.outcome.status()
        )
    });
    let set = suite.store.read(&suite.tenant, &aor(who)).0;
    let ca_deadline = set
        .all()
        .iter()
        .find(|binding| binding.contact == Bytes::from_static(b"sip:a@10.0.0.1"))
        .map(|binding| binding.expires_at);
    suite.check(
        "LS-R-23",
        set.all().len() == 2 && ca_deadline == deadline,
        || {
            format!(
                "expected CA untouched beside a new CB: {} binding(s), CA deadline {:?} → {:?}",
                set.all().len(),
                deadline,
                ca_deadline
            )
        },
    );
}

fn ls_r_stale_cseq(suite: &mut Suite<'_>) {
    let who = "r4";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 2, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );
    let before = suite.store.read(&suite.tenant, &aor(who));

    let applied = apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 10, vec![ca()]),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-4", applied.outcome.status() == 500, || {
        format!("expected 500, got {}", applied.outcome.status())
    });
    let after = suite.store.read(&suite.tenant, &aor(who));
    suite.check("LS-R-4", before.1 == after.1, || {
        format!("the store moved: {:?} → {:?}", before.1, after.1)
    });
}

fn ls_r_new_call_id(suite: &mut Suite<'_>) {
    let who = "r5";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 9, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(&suite.tenant, who, "i2", 1, 10, vec![ca()]),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-5", applied.outcome.commits(), || {
        "B2: a different Call-ID applies regardless of CSeq".to_owned()
    });
}

fn ls_r_remove_one_of_two(suite: &mut Suite<'_>) {
    let who = "r6";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i1",
            2,
            10,
            vec![contact("sip:a@10.0.0.1", Some(0), None)],
        ),
        &policy(),
        RETRIES,
    );
    let contacts: Vec<String> = applied
        .outcome
        .accepted()
        .map(|a| {
            a.contacts
                .iter()
                .map(|c| String::from_utf8_lossy(&c.contact).into_owned())
                .collect()
        })
        .unwrap_or_default();
    suite.check("LS-R-6", contacts == ["sip:b@10.0.0.2".to_owned()], || {
        format!("the complete-set rule: expected only CB, got {contacts:?}")
    });
}

fn ls_r_wildcard(suite: &mut Suite<'_>) {
    let who = "r7";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );

    let mut wildcard = command(&suite.tenant, who, "i9", 1, 10, vec![]);
    wildcard.contacts = ContactOps::Wildcard;
    wildcard.expires_header = Some(0);
    let applied = apply(suite.store, &wildcard, &policy(), RETRIES);
    suite.check(
        "LS-R-7",
        applied.outcome.commits()
            && applied
                .outcome
                .accepted()
                .is_some_and(|a| a.contacts.is_empty()),
        || "a valid wildcard removes everything and answers with no contacts".to_owned(),
    );
    suite.check(
        "LS-R-7",
        suite
            .store
            .read(&suite.tenant, &aor(who))
            .0
            .all()
            .is_empty(),
        || "the stored set must be empty".to_owned(),
    );

    // W2 — a wildcard without an explicit `Expires: 0`.
    let mut bad = command(&suite.tenant, who, "i8", 2, 20, vec![]);
    bad.contacts = ContactOps::Wildcard;
    bad.expires_header = Some(3_600);
    let applied = apply(suite.store, &bad, &policy(), RETRIES);
    suite.check("LS-R-9", applied.outcome.status() == 400, || {
        format!("expected 400, got {}", applied.outcome.status())
    });
}

fn ls_r_min_and_max_expires(suite: &mut Suite<'_>) {
    let strict = TenantPolicy {
        min_expires: 300,
        max_expires: 7_200,
        ..TenantPolicy::default()
    };

    let brief = command(
        &suite.tenant,
        "r11",
        "i1",
        1,
        0,
        vec![contact("sip:a@10.0.0.1", Some(60), None)],
    );
    let applied = apply(suite.store, &brief, &strict, RETRIES);
    suite.check("LS-R-11", applied.outcome.status() == 423, || {
        format!("expected 423, got {}", applied.outcome.status())
    });
    suite.check("LS-R-11", applied.revision == Revision::INITIAL, || {
        "nothing may commit".to_owned()
    });

    let long = command(
        &suite.tenant,
        "r12",
        "i1",
        1,
        0,
        vec![contact("sip:a@10.0.0.1", Some(86_400), None)],
    );
    let applied = apply(suite.store, &long, &strict, RETRIES);
    let granted = applied
        .outcome
        .accepted()
        .and_then(|a| a.contacts.first().map(|c| c.expires));
    suite.check("LS-R-12", granted == Some(7_200), || {
        format!("E5 lowers silently and states what was granted: got {granted:?}")
    });
}

fn ls_r_quota(suite: &mut Suite<'_>) {
    let who = "r15";
    let capped = TenantPolicy {
        max_bindings_per_aor: 2,
        ..TenantPolicy::default()
    };
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb()]),
        &capped,
        RETRIES,
    );

    let third = apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 2, 10, vec![cc()]),
        &capped,
        RETRIES,
    );
    suite.check("LS-R-15", third.outcome.status() == 403, || {
        format!("expected 403, got {}", third.outcome.status())
    });

    let refresh = apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 3, 20, vec![cb()]),
        &capped,
        RETRIES,
    );
    suite.check("LS-R-15", refresh.outcome.status() == 200, || {
        "a refresh never grows the set, so it never trips the quota".to_owned()
    });
}

fn ls_r_atomicity(suite: &mut Suite<'_>) {
    let who = "r16";
    let strict = TenantPolicy {
        min_expires: 300,
        ..TenantPolicy::default()
    };
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i1",
            1,
            0,
            vec![cb(), contact("sip:c@10.0.0.3", Some(60), None)],
        ),
        &strict,
        RETRIES,
    );
    suite.check("LS-R-16", applied.outcome.status() == 423, || {
        format!("expected 423, got {}", applied.outcome.status())
    });
    suite.check(
        "LS-R-16",
        suite
            .store
            .read(&suite.tenant, &aor(who))
            .0
            .all()
            .is_empty(),
        || "neither contact may commit — atomicity".to_owned(),
    );
}

fn ls_r_path(suite: &mut Suite<'_>) {
    let who = "r17";
    let mut with_path = command(&suite.tenant, who, "i1", 1, 0, vec![ca()]);
    with_path.path = vec![
        Bytes::from_static(b"<sip:p2.example;lr>"),
        Bytes::from_static(b"<sip:p1.example;lr>"),
    ];
    with_path.supports_path = true;
    let applied = apply(suite.store, &with_path, &policy(), RETRIES);
    let echoed = applied
        .outcome
        .accepted()
        .map(|a| a.path.clone())
        .unwrap_or_default();
    suite.check(
        "LS-R-17",
        echoed
            == vec![
                Bytes::from_static(b"<sip:p2.example;lr>"),
                Bytes::from_static(b"<sip:p1.example;lr>"),
            ],
        || format!("expected the Path echoed topmost-first, got {echoed:?}"),
    );

    // §5.6 — Path without `Supported: path`.
    let mut unsupported = command(&suite.tenant, "r18", "i1", 1, 0, vec![ca()]);
    unsupported.path = vec![Bytes::from_static(b"<sip:p1.example;lr>")];
    let applied = apply(suite.store, &unsupported, &policy(), RETRIES);
    suite.check("LS-R-18", applied.outcome.status() == 421, || {
        format!("expected 421, got {}", applied.outcome.status())
    });
}

fn ls_r_expired_binding_is_absent(suite: &mut Suite<'_>) {
    let who = "r21";
    apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i1",
            9,
            0,
            vec![contact("sip:a@10.0.0.1", Some(100), None)],
        ),
        &policy(),
        RETRIES,
    );
    // At t=200 the binding has lapsed, so an *older* CSeq is not stale against anything.
    let applied = apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 3, 200, vec![ca()]),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-21", applied.outcome.commits(), || {
        format!(
            "an expired binding is absent for every purpose: got {}",
            applied.outcome.status()
        )
    });
}

/// LS-R-24 — two removals and an addition in one REGISTER (§5.3.2 B6).
///
/// The multi-contact rows are here, and not only in the registrar's own vector file, because what
/// they pin is the set that gets **committed** — and a backend is what makes a committed set
/// durable. `RG-16` was a decision-logic defect, but a backend that dropped or reordered the rows of
/// one commit would present the identical symptom, a set the request never described, and the
/// in-memory suite could not tell the two apart. Reconciliation is store-independent, so each row
/// must give the identical answer on every backend; a backend needing its own version of one has
/// broken §6.3's "identical suite" claim rather than implemented it.
fn ls_r_two_removals_and_an_addition(suite: &mut Suite<'_>) {
    // The first removal shortens the set, so a registrar resolving later operations against a view
    // captured before it leaves CB bound.
    let who = "r24";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![
                contact("sip:a@10.0.0.1", Some(0), None),
                contact("sip:b@10.0.0.2", Some(0), None),
                cc(),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-24", applied.outcome.status() == 200, || {
        format!("expected 200, got {}", applied.outcome.status())
    });
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-24", stored == ["sip:c@10.0.0.3".to_owned()], || {
        format!("expected exactly CC committed, got {stored:?}")
    });
}

/// LS-R-25 — remove then refresh (B6). The refresh must land on CB, not on the binding the removal
/// shifted into CB's former index.
fn ls_r_a_removal_ahead_of_a_refresh(suite: &mut Suite<'_>) {
    let who = "r25";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb(), cc()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![
                contact("sip:a@10.0.0.1", Some(0), None),
                contact("sip:b@10.0.0.2", Some(7_200), None),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-25", applied.outcome.status() == 200, || {
        format!("expected 200, got {}", applied.outcome.status())
    });
    let stored = stored_contacts(suite, who);
    suite.check(
        "LS-R-25",
        stored == ["sip:b@10.0.0.2".to_owned(), "sip:c@10.0.0.3".to_owned()],
        || format!("expected CB and CC, each exactly once, got {stored:?}"),
    );
}

/// LS-R-26 — two removals, nothing else: the committed set is empty (B6).
///
/// The stripped-down shape, where a stale view leaves CB's removal resolving one past the end of the
/// set the first removal shortened, so CB survives a request that named it.
fn ls_r_two_removals_commit_the_empty_set(suite: &mut Suite<'_>) {
    let who = "r26";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![
                contact("sip:a@10.0.0.1", Some(0), None),
                contact("sip:b@10.0.0.2", Some(0), None),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-26", applied.outcome.status() == 200, || {
        format!("expected 200, got {}", applied.outcome.status())
    });
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-26", stored.is_empty(), || {
        format!("expected the empty set committed, got {stored:?}")
    });
}

/// LS-R-27 — LS-R-25's operations in the opposite order (B6). The refresh does not move CA, so the
/// removal that follows still resolves to CA.
fn ls_r_a_refresh_ahead_of_a_removal(suite: &mut Suite<'_>) {
    let who = "r27";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca(), cb(), cc()]),
        &policy(),
        RETRIES,
    );
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![
                contact("sip:b@10.0.0.2", Some(7_200), None),
                contact("sip:a@10.0.0.1", Some(0), None),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-27", applied.outcome.status() == 200, || {
        format!("expected 200, got {}", applied.outcome.status())
    });
    let stored = stored_contacts(suite, who);
    suite.check(
        "LS-R-27",
        stored == ["sip:b@10.0.0.2".to_owned(), "sip:c@10.0.0.3".to_owned()],
        || format!("expected CB and CC, each exactly once, got {stored:?}"),
    );
}

/// LS-R-28 — B7. Two §19.1.4-equivalent spellings in one request: the removal applies to the binding
/// the addition just made, rather than aborting under this request's own ordering token.
fn ls_r_a_removal_of_what_this_request_added(suite: &mut Suite<'_>) {
    let who = "r28";
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![cc(), contact("sip:c@10.0.0.3;line=7", Some(0), None)],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-28", applied.outcome.status() == 200, || {
        format!(
            "a contact named twice in one request is not a second write under a spent token: got {}",
            applied.outcome.status()
        )
    });
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-28", stored.is_empty(), || {
        format!("the later operation was the removal, so nothing is bound; got {stored:?}")
    });
}

/// LS-R-29 — B7 the other way: the later operation replaces the binding the earlier one added, so the
/// set holds one binding at the later grant, not two bindings and not an abort.
fn ls_r_a_second_operation_on_what_this_request_added(suite: &mut Suite<'_>) {
    let who = "r29";
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![cc(), contact("sip:c@10.0.0.3", Some(7_200), None)],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-29", applied.outcome.status() == 200, || {
        format!("expected 200, got {}", applied.outcome.status())
    });
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-29", stored == ["sip:c@10.0.0.3".to_owned()], || {
        format!("expected one binding, not two: got {stored:?}")
    });
    let granted = suite
        .store
        .read(&suite.tenant, &aor(who))
        .0
        .all()
        .first()
        .map(|binding| binding.refreshed_at.until(binding.expires_at).as_secs());
    suite.check("LS-R-29", granted == Some(7_200), || {
        format!("the later operation's grant is the one that holds: got {granted:?}s")
    });
}

/// The nine contacts that fill the default quota to one short of its limit.
const NINE: [&str; 9] = [
    "sip:n1@10.0.0.11",
    "sip:n2@10.0.0.12",
    "sip:n3@10.0.0.13",
    "sip:n4@10.0.0.14",
    "sip:n5@10.0.0.15",
    "sip:n6@10.0.0.16",
    "sip:n7@10.0.0.17",
    "sip:n8@10.0.0.18",
    "sip:n9@10.0.0.19",
];

/// LS-R-30 — §5.5. The quota is a test on the committed outcome, so a chain B6/B7 collapses onto one
/// binding is measured by what it commits and not by the candidates it carries.
fn ls_r_quota_measures_the_committed_outcome(suite: &mut Suite<'_>) {
    let fill = |suite: &mut Suite<'_>, who: &str| {
        let applied = apply(
            suite.store,
            &command(
                &suite.tenant,
                who,
                "i1",
                1,
                0,
                NINE.iter()
                    .map(|text| contact(text, Some(3_600), None))
                    .collect(),
            ),
            &policy(),
            RETRIES,
        );
        suite.check("LS-R-30", applied.outcome.status() == 200, || {
            format!(
                "nine bindings fit a quota of ten, got {}",
                applied.outcome.status()
            )
        });
    };

    // `a, b, c` — `a ≡ b` and `b ≡ c`, but `a ≢ c`. B6 resolves each operation against the set the
    // preceding ones left, so `b` replaces what `a` added and `c` replaces it again: one binding.
    let who = "r30";
    fill(suite, who);
    let applied = apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i2",
            1,
            10,
            vec![
                contact("sip:c@10.0.0.3;line=1", Some(3_600), None),
                contact("sip:c@10.0.0.3", Some(3_600), None),
                contact("sip:c@10.0.0.3;line=2", Some(3_600), None),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-30", applied.outcome.status() == 200, || {
        format!(
            "the committed outcome is ten bindings, which the quota permits; got {}",
            applied.outcome.status()
        )
    });
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-30", stored.len() == 10, || {
        format!(
            "three operations collapse onto one binding, got {}",
            stored.len()
        )
    });

    // And the other direction: a chain that genuinely commits two bindings is still refused, so the
    // fix is not "stop asking". `line=1` and `line=2` both carry `line`, so they must agree on it and
    // do not — neither replaces the other.
    let other = "r30b";
    fill(suite, other);
    let refused = apply(
        suite.store,
        &command(
            &suite.tenant,
            other,
            "i2",
            1,
            10,
            vec![
                contact("sip:c@10.0.0.3;line=1", Some(3_600), None),
                contact("sip:c@10.0.0.3;line=2", Some(3_600), None),
            ],
        ),
        &policy(),
        RETRIES,
    );
    suite.check("LS-R-30", refused.outcome.status() == 403, || {
        format!(
            "two operations committing two bindings do exceed the quota, got {}",
            refused.outcome.status()
        )
    });
    let untouched = stored_contacts(suite, other);
    suite.check("LS-R-30", untouched.len() == 9, || {
        format!("a refusal commits nothing, got {}", untouched.len())
    });
}

/// LS-R-31 — B8. A verbatim retransmission of LS-R-29's request is B4's retry, because the outcome
/// the command asks of that binding is the *later* operation's grant, which is what the store holds.
fn ls_r_a_retransmission_of_a_contact_named_twice(suite: &mut Suite<'_>) {
    let who = "r31";
    let first = command(
        &suite.tenant,
        who,
        "i2",
        1,
        10,
        vec![cc(), contact("sip:c@10.0.0.3", Some(7_200), None)],
    );
    let applied = apply(suite.store, &first, &policy(), RETRIES);
    suite.check("LS-R-31", applied.outcome.status() == 200, || {
        format!(
            "the first delivery is a 200, got {}",
            applied.outcome.status()
        )
    });

    let mut again = first.clone();
    again.now = Timestamp::from_nanos(first.now.as_nanos() + RETRANSMIT_DELAY_NANOS);
    let again = apply(suite.store, &again, &policy(), RETRIES);
    suite.check("LS-R-31", again.outcome.status() == 200, || {
        format!(
            "a retransmission of this request is a retry, not a stale sequence; got {}",
            again.outcome.status()
        )
    });
    suite.check(
        "LS-R-31",
        !again.outcome.commits() && again.revision == applied.revision,
        || {
            format!(
                "B4.2 — a retry commits nothing and leaves the revision: commits={} {:?} → {:?}",
                again.outcome.commits(),
                applied.revision,
                again.revision
            )
        },
    );
    let granted = suite
        .store
        .read(&suite.tenant, &aor(who))
        .0
        .all()
        .first()
        .map(|binding| binding.refreshed_at.until(binding.expires_at).as_secs());
    suite.check("LS-R-31", granted == Some(7_200), || {
        format!("the retry must not rewrite the binding: got {granted:?}s")
    });
}

/// LS-R-32 — B9. LS-R-28's request cancels itself out, so neither delivery writes and the revision
/// stays put; the two deliveries are indistinguishable from the durable state.
fn ls_r_operations_that_cancel_out_commit_nothing(suite: &mut Suite<'_>) {
    let who = "r32";
    // Both deliveries built up front: the second is the first with a later `now`, which is what a
    // retransmission is, and building them here keeps `suite` free to be checked against.
    let delivery = command(
        &suite.tenant,
        who,
        "i2",
        1,
        10,
        vec![cc(), contact("sip:c@10.0.0.3;line=7", Some(0), None)],
    );
    let mut retransmission = delivery.clone();
    retransmission.now = Timestamp::from_nanos(delivery.now.as_nanos() + RETRANSMIT_DELAY_NANOS);
    let before = suite.store.read(&suite.tenant, &aor(who)).1;

    let first = apply(suite.store, &delivery, &policy(), RETRIES);
    suite.check(
        "LS-R-32",
        first.outcome.status() == 200 && first.revision == before,
        || {
            format!(
                "the reconciled set is the set that was read, so nothing commits: status {} revision {:?} → {:?}",
                first.outcome.status(),
                before,
                first.revision
            )
        },
    );

    let retry = apply(suite.store, &retransmission, &policy(), RETRIES);
    suite.check(
        "LS-R-32",
        retry.outcome.status() == 200 && retry.revision == before,
        || {
            format!(
                "and the retransmission is answered identically: status {} revision {:?}",
                retry.outcome.status(),
                retry.revision
            )
        },
    );
    let stored = stored_contacts(suite, who);
    suite.check("LS-R-32", stored.is_empty(), || {
        format!("nothing is bound either way, got {stored:?}")
    });
}

/// Every contact this address-of-record holds, in stored order.
fn stored_contacts(suite: &Suite<'_>, who: &str) -> Vec<String> {
    suite
        .store
        .read(&suite.tenant, &aor(who))
        .0
        .all()
        .iter()
        .map(|binding| String::from_utf8_lossy(&binding.contact).into_owned())
        .collect()
}

fn ls_k_cas_conflict(suite: &mut Suite<'_>) {
    let who = "k1";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );

    // Two commands read the same revision, the way two nodes racing on one AoR would.
    let (set, revision) = suite.store.read(&suite.tenant, &aor(who));
    let second = command(&suite.tenant, who, "i2", 1, 10, vec![cb()]);
    let outcome = crate::process::process(&second, &set, &policy());
    let Outcome::Commit { set: staged, .. } = outcome else {
        suite.check("LS-K-1", false, || {
            "the second command should commit".to_owned()
        });
        return;
    };

    // First writer wins the revision.
    let first_write = suite
        .store
        .commit(&suite.tenant, &aor(who), revision, staged.clone());
    suite.check("LS-K-1", first_write.is_ok(), || {
        format!("the first commit at the read revision must succeed: {first_write:?}")
    });

    // A second commit holding the same, now-spent revision is refused rather than overwriting.
    let conflict = suite
        .store
        .commit(&suite.tenant, &aor(who), revision, staged);
    suite.check("LS-K-1", conflict.is_err(), || {
        "a spent revision must not overwrite".to_owned()
    });
    if let Err(conflict) = conflict {
        suite.check("LS-K-1", conflict.expected == revision, || {
            format!("the conflict should report what was held: {conflict:?}")
        });
    }
}

fn ls_k_revision_survives_empty(suite: &mut Suite<'_>) {
    let who = "k3";
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );
    let mut wildcard = command(&suite.tenant, who, "i9", 1, 10, vec![]);
    wildcard.contacts = ContactOps::Wildcard;
    wildcard.expires_header = Some(0);
    let emptied = apply(suite.store, &wildcard, &policy(), RETRIES);

    let again = apply(
        suite.store,
        &command(&suite.tenant, who, "i3", 1, 20, vec![ca()]),
        &policy(),
        RETRIES,
    );
    suite.check("LS-K-3", again.revision > emptied.revision, || {
        format!(
            "revisions never reset, including across an empty set: {:?} then {:?}",
            emptied.revision, again.revision
        )
    });
}

fn ls_k_changes(suite: &mut Suite<'_>) {
    let who = "k4";
    let before = suite.store.changes().len();
    apply(
        suite.store,
        &command(&suite.tenant, who, "i1", 1, 0, vec![ca()]),
        &policy(),
        RETRIES,
    );
    let refresh = command(&suite.tenant, who, "i1", 2, 10, vec![ca()]);
    apply(suite.store, &refresh, &policy(), RETRIES);
    // A Noop must not emit: a consumer re-reading on every retry would amplify a retry storm.
    apply(suite.store, &refresh, &policy(), RETRIES);

    let emitted = suite.store.changes().len() - before;
    suite.check("LS-K-4", emitted == 2, || {
        format!("one event per commit and none per Noop: got {emitted}")
    });
}

fn ls_l_lookup_order(suite: &mut Suite<'_>) {
    let who = "l1";
    // Two contacts at distinct q, so the order is decided by preference rather than by chance.
    apply(
        suite.store,
        &command(
            &suite.tenant,
            who,
            "i1",
            1,
            0,
            vec![
                contact("sip:low@10.0.0.9", Some(3_600), Some(500)),
                contact("sip:high@10.0.0.8", Some(3_600), Some(1_000)),
            ],
        ),
        &policy(),
        RETRIES,
    );

    let found = suite
        .store
        .lookup(&suite.tenant, &aor(who), Timestamp::from_secs(10));
    let order: Vec<String> = found
        .iter()
        .map(|target| String::from_utf8_lossy(&target.contact).into_owned())
        .collect();
    suite.check(
        "LS-L-1",
        order
            == [
                "sip:high@10.0.0.8".to_owned(),
                "sip:low@10.0.0.9".to_owned(),
            ],
        || format!("descending q: expected high then low, got {order:?}"),
    );

    let empty = suite
        .store
        .lookup(&suite.tenant, &aor(who), Timestamp::from_secs(100_000));
    suite.check("LS-L-4", empty.is_empty(), || {
        format!("every binding expired: expected none, got {}", empty.len())
    });

    let unknown = suite
        .store
        .lookup(&suite.tenant, &aor("nobody-here"), Timestamp::from_secs(10));
    suite.check("LS-L-7", unknown.is_empty(), || {
        "an unknown address-of-record is the empty set".to_owned()
    });
}
