//! `RG-17` — §6 K7: a store this node cannot read is not an empty store.
//!
//! [location-service §6.1 K7 and §7 L8](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).
//! The rows here run against a **double** whose reads fail on demand, which is what makes them
//! deterministic and part of every run; `crates/sipx-clstr-node/tests/postgres_read_faults.rs` runs
//! the same shapes against a real database, and `run_read_failure_suite` is the set of rows both
//! sides execute.
//!
//! Why these three shapes and not a mutation. §6 K6's argument — a stale read is fenced by the
//! revision predicate, so it costs a `CasConflict` and a retry rather than a lost update — is sound,
//! and reaches only the commands that *commit*. A query (RFC 3261 §10.2.3), B4's idempotent retry and
//! B1's removal of a contact that is not bound all reach `Outcome::Noop`, which returns before any
//! commit and therefore meets no predicate. An invented `(∅, 0)` under those goes out as a `200 OK`
//! enumerating no bindings — a UA told it is registered nowhere, or told its deregistration was
//! applied, about durable state nobody read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_registrar::{
    BindingSet, CanonicalAor, CasConflict, Change, ContactOp, ContactOps, LocationStore,
    ReadFailure, RegisterCommand, Rejection, Revision, TenantPolicy, Timestamp, apply,
};

const RETRIES: usize = 3;

/// A store that cannot be read, and knows nothing else.
///
/// `commit` is deliberately reachable and deliberately unused: every row below asserts about a
/// command that never commits, so a `commit` that ran at all would mean the read failure had been
/// absorbed somewhere upstream of the decision.
struct Unreadable(ReadFailure);

impl LocationStore for Unreadable {
    fn read(
        &self,
        _tenant: &str,
        _aor: &CanonicalAor,
    ) -> Result<(BindingSet, Revision), ReadFailure> {
        Err(self.0.clone())
    }

    fn commit(
        &self,
        _tenant: &str,
        _aor: &CanonicalAor,
        expected: Revision,
        _set: BindingSet,
    ) -> Result<Revision, CasConflict> {
        panic!("nothing may commit against a store that could not be read (expected {expected:?})")
    }

    fn lookup(
        &self,
        _tenant: &str,
        _aor: &CanonicalAor,
        _now: Timestamp,
    ) -> Result<Vec<sipx_clstr_registrar::Target>, ReadFailure> {
        Err(self.0.clone())
    }

    fn changes(&self) -> Vec<Change> {
        Vec::new()
    }
}

fn refused() -> Unreadable {
    Unreadable(ReadFailure::Unavailable(
        "the connection is gone".to_owned(),
    ))
}

fn undecodable() -> Unreadable {
    Unreadable(ReadFailure::Undecodable(
        "expected a binding set, found a string".to_owned(),
    ))
}

fn aor() -> CanonicalAor {
    CanonicalAor::parse(Bytes::from_static(b"sip:alice@faults.example")).expect("a valid AoR")
}

fn command(contacts: Vec<ContactOp>) -> RegisterCommand {
    RegisterCommand {
        tenant: "t1".to_owned(),
        aor: aor(),
        call_id: Bytes::from_static(b"i1"),
        cseq: 1,
        contacts: ContactOps::Explicit(contacts),
        expires_header: None,
        path: Vec::new(),
        supports_path: false,
        require: Vec::new(),
        received: None,
        flow_ref: None,
        principal: None,
        now: Timestamp::from_secs(100),
    }
}

fn contact(uri: &'static str, expires: u32) -> ContactOp {
    ContactOp {
        uri: sipx_sip::Uri::parse(Bytes::from_static(uri.as_bytes())).expect("a URI"),
        verbatim: Bytes::from_static(uri.as_bytes()),
        expires: Some(expires),
        q: None,
        instance_id: None,
        reg_id: None,
        push: None,
    }
}

/// **`RG-17`'s failing-first row.** A query is the shape with nothing downstream of the read.
#[test]
fn ls_k_7_a_query_against_an_unreadable_store_is_refused_rather_than_answered_empty() {
    for store in [refused(), undecodable()] {
        // RFC 3261 §10.2.3 — no `Contact` at all.
        let applied = apply(
            &store,
            &command(Vec::new()),
            &TenantPolicy::default(),
            RETRIES,
        );
        assert_eq!(
            applied.outcome.status(),
            503,
            "a query answered from a store nobody could read is a false 200 ({:?})",
            store.0
        );
        assert!(
            matches!(
                applied.outcome,
                sipx_clstr_registrar::Outcome::Reject(Rejection::Unavailable)
            ),
            "§5.1 S10's refusal, not some other rejection: {:?}",
            applied.outcome
        );
        assert_eq!(
            applied.revision, None,
            "nothing was read, so there is no revision to report — and revision zero is a real, \
             readable state rather than a stand-in for one nobody learned"
        );
    }
}

/// The other never-commits shape: §5.3 B1's removal of a contact that is not bound.
#[test]
fn ls_k_8_a_no_op_deregistration_against_an_unreadable_store_is_refused() {
    for store in [refused(), undecodable()] {
        let applied = apply(
            &store,
            &command(vec![contact("sip:gone@10.0.0.9", 0)]),
            &TenantPolicy::default(),
            RETRIES,
        );
        assert_eq!(
            applied.outcome.status(),
            503,
            "a deregistration that never reached the store must not be reported as done ({:?})",
            store.0
        );
    }
}

/// A mutation takes the identical path — it does **not** wait for the compare-and-swap to discover
/// the fault, which is what the superseded comment in `postgres_store.rs` relied on.
#[test]
fn rg17_a_mutation_refuses_at_the_read_rather_than_at_the_commit() {
    let store = refused();
    let applied = apply(
        &store,
        &command(vec![contact("sip:alice@10.0.0.1", 3_600)]),
        &TenantPolicy::default(),
        RETRIES,
    );
    assert_eq!(applied.outcome.status(), 503);
    assert_eq!(
        applied.conflicts, 0,
        "no CAS attempt may be made against a set that was never read"
    );
}

/// §5.4's wildcard removal is a mutation too, and it is the one whose failure a UA reads as
/// "every device is deregistered".
#[test]
fn rg17_a_wildcard_removal_against_an_unreadable_store_is_refused() {
    let store = undecodable();
    let mut wildcard = command(Vec::new());
    wildcard.contacts = ContactOps::Wildcard;
    wildcard.expires_header = Some(0);
    let applied = apply(&store, &wildcard, &TenantPolicy::default(), RETRIES);
    assert_eq!(applied.outcome.status(), 503);
}

/// §7 L8 — the failure and the empty target set are different answers.
#[test]
fn ls_l_9_a_lookup_that_could_not_read_is_not_an_empty_target_set() {
    let store = refused();
    let found = store.lookup("t1", &aor(), Timestamp::from_secs(10));
    assert!(
        found.is_err(),
        "an outage answered as no targets is a callee reported unavailable by a platform that \
         never looked"
    );

    // And the distinction is only worth anything if the *other* side still reads as an answer.
    let empty = sipx_clstr_registrar::InMemoryStore::new();
    assert_eq!(
        empty.lookup("t1", &aor(), Timestamp::from_secs(10)),
        Ok(Vec::new()),
        "an address-of-record with no bindings is still an answer (L5)"
    );
}

/// Both backends' rows, executed here against the double — the in-memory half of the identical
/// suite `crates/sipx-clstr-node/tests/postgres_store.rs` runs against a real database.
#[test]
#[cfg(feature = "test-suite")]
fn the_read_failure_rows_run_against_a_store_whose_reads_fail() {
    let failures = sipx_clstr_registrar::conformance::run_read_failure_suite(
        &refused(),
        "unreadable-double",
        "t1",
    );
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
