//! The `PostgreSQL` backend against the same suite the in-memory one passes — `RG-4`.
//!
//! [location-service §6.3](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)
//! promises the PostgreSQL backend "passes the identical suite". This file is what makes that word
//! true: it calls `run_location_store_suite`, the same function the in-memory backend is checked with,
//! and asserts it reports nothing.
//!
//! # Running it
//!
//! ```sh
//! docker run -d --rm -e POSTGRES_PASSWORD=sipx -e POSTGRES_DB=sipx -p 55432:5432 postgres:16-alpine
//! SIPX_CLSTR_TEST_DATABASE_URL=postgres://postgres:sipx@127.0.0.1:55432/sipx \
//!   cargo test -p sipx-clstr-node --features postgres
//! ```
//!
//! Without that variable the tests **skip and say so**. Two things that are deliberately not done: it
//! does not start a database itself (a test that manages containers fails for reasons that have
//! nothing to do with the code), and it does not silently pass (a skip that looks like a pass is how a
//! backend stops being tested without anyone deciding to stop testing it).

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sipx_clstr_node::postgres_store::PostgresStore;
use sipx_clstr_registrar::conformance::run_location_store_suite;
use sipx_clstr_registrar::{
    BindingSet, CanonicalAor, LocationStore, Revision, TenantPolicy, Timestamp, apply,
};

const URL_VAR: &str = "SIPX_CLSTR_TEST_DATABASE_URL";

/// A connected store with `tenant` emptied, or `None` when no database was configured.
///
/// **Each test owns a tenant.** Sharing one made these tests flaky at about one run in five: four of
/// them run concurrently, each truncating the shared tenant, so one wiped another's rows mid-run and
/// a commit came back `CasConflict { current: Revision(0) }` — the row had simply vanished. Isolating
/// by tenant is also the contract's own boundary: §5's serialization domain is `(tenant, aor)`, so a
/// shared tenant had these tests contradicting the independence they exist to assert.
fn store(tenant: &str) -> Option<PostgresStore> {
    let url = std::env::var(URL_VAR).ok()?;
    let store = PostgresStore::connect(&url)
        .unwrap_or_else(|e| panic!("{URL_VAR} is set but connecting failed: {e:?}"));
    store.truncate(tenant).expect("truncate");
    Some(store)
}

/// Announce a skip loudly enough that nobody mistakes it for coverage.
fn skipped(what: &str) {
    println!("SKIPPED {what}: set {URL_VAR} to run the PostgreSQL backend against the suite");
}

#[test]
fn the_postgres_backend_passes_the_same_suite_as_the_in_memory_one() {
    let tenant = "suite-postgres";
    let Some(store) = store(tenant) else {
        skipped("postgres conformance");
        return;
    };

    let failures = run_location_store_suite(&store, "postgres", tenant);
    assert!(
        failures.is_empty(),
        "the PostgreSQL backend must pass the identical suite:\n{}",
        failures
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_in_memory_backend_passes_it_too_so_the_suite_itself_is_honest() {
    // If the suite could not pass on the backend it was written against, a green PostgreSQL run
    // would prove nothing about PostgreSQL.
    let store = sipx_clstr_registrar::InMemoryStore::new();
    let failures = run_location_store_suite(&store, "in-memory", "suite-in-memory");
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

#[test]
fn a_binding_survives_a_round_trip_through_the_database_unchanged() {
    // The encoding is the part a second backend adds, and the part a vector suite cannot see: it
    // asserts behaviour, and behaviour would look identical if `Path` or `q` quietly vanished on the
    // way to disk and back.
    let tenant = "round-trip";
    let Some(store) = store(tenant) else {
        skipped("binding round trip");
        return;
    };

    let aor = CanonicalAor::parse(bytes::Bytes::from_static(
        b"sip:roundtrip@conformance.example",
    ))
    .expect("a valid AoR");
    let mut cmd = sipx_clstr_registrar::RegisterCommand {
        tenant: tenant.to_owned(),
        aor: aor.clone(),
        call_id: bytes::Bytes::from_static(b"rt"),
        cseq: 1,
        contacts: sipx_clstr_registrar::ContactOps::Explicit(vec![
            sipx_clstr_registrar::ContactOp {
                uri: sipx_sip::Uri::parse(bytes::Bytes::from_static(
                    b"sip:device@10.0.0.7;transport=tcp",
                ))
                .expect("a URI"),
                verbatim: bytes::Bytes::from_static(b"sip:device@10.0.0.7;transport=tcp"),
                expires: Some(3_600),
                q: Some(700),
                instance_id: Some(bytes::Bytes::from_static(b"\"<urn:uuid:abc>\"")),
                reg_id: Some(3),
                push: None,
            },
        ]),
        expires_header: None,
        path: vec![bytes::Bytes::from_static(b"<sip:p1.example;lr>")],
        supports_path: true,
        require: Vec::new(),
        received: None,
        flow_ref: Some(bytes::Bytes::from_static(b"flow-42")),
        principal: Some(bytes::Bytes::from_static(b"alice")),
        now: Timestamp::from_secs(100),
    };
    cmd.aor = aor.clone();

    let applied = apply(&store, &cmd, &TenantPolicy::default(), 3);
    assert!(applied.outcome.commits(), "the fixture should commit");

    let (set, revision) = store.read(tenant, &aor);
    assert_eq!(revision, Revision(1));
    let binding = set.all().first().expect("one binding");

    assert_eq!(
        binding.contact.as_ref(),
        b"sip:device@10.0.0.7;transport=tcp",
        "the contact must survive verbatim — parameters included"
    );
    assert_eq!(binding.q, 700, "the preference must survive");
    assert_eq!(
        binding.path,
        vec![bytes::Bytes::from_static(b"<sip:p1.example;lr>")],
        "the Path must survive, in order"
    );
    assert_eq!(binding.reg_id, Some(3));
    assert_eq!(
        binding.instance_id.as_deref(),
        Some(&b"\"<urn:uuid:abc>\""[..])
    );
    assert_eq!(binding.flow_ref.as_deref(), Some(&b"flow-42"[..]));
    assert_eq!(binding.principal.as_deref(), Some(&b"alice"[..]));
    assert_eq!(binding.expires_at, Timestamp::from_secs(3_700));
    assert_eq!(binding.registered_at, Timestamp::from_secs(100));
}

#[test]
fn a_first_write_race_produces_one_row_not_two() {
    // The insert path is its own compare-and-swap. Two nodes both believing an address-of-record is
    // new is the common case on a cold start, and a `SELECT` followed by an `INSERT` would let both
    // through — one silently overwriting the other's bindings.
    let tenant = "first-write";
    let Some(store) = store(tenant) else {
        skipped("first-write race");
        return;
    };

    let aor = CanonicalAor::parse(bytes::Bytes::from_static(b"sip:cold@conformance.example"))
        .expect("a valid AoR");

    let first = store.commit(tenant, &aor, Revision::INITIAL, BindingSet::new());
    assert_eq!(first, Ok(Revision(1)));

    let second = store.commit(tenant, &aor, Revision::INITIAL, BindingSet::new());
    let conflict = second.expect_err("the second writer must lose");
    assert_eq!(conflict.expected, Revision::INITIAL);
    assert_eq!(
        conflict.current,
        Revision(1),
        "and must be told where the address-of-record actually is"
    );
}

#[test]
fn the_revision_predicate_is_what_refuses_a_stale_write() {
    let tenant = "revision-predicate";
    let Some(store) = store(tenant) else {
        skipped("revision predicate");
        return;
    };

    let aor = CanonicalAor::parse(bytes::Bytes::from_static(b"sip:fence@conformance.example"))
        .expect("a valid AoR");
    store
        .commit(tenant, &aor, Revision::INITIAL, BindingSet::new())
        .expect("first");
    store
        .commit(tenant, &aor, Revision(1), BindingSet::new())
        .expect("second");

    // Revision 1 is spent; only 2 may write now.
    assert!(
        store
            .commit(tenant, &aor, Revision(1), BindingSet::new())
            .is_err()
    );
    assert_eq!(
        store.commit(tenant, &aor, Revision(2), BindingSet::new()),
        Ok(Revision(3))
    );
}

#[test]
fn a_dropped_change_notification_changes_nothing() {
    // §6 K4/K5: the change stream is **best-effort**, and correctness never depends on it — the
    // guarantee is the staleness *bound*, not the delivery. This backend's read path is read-through:
    // no cache, so observed staleness is zero and the TTL bound holds trivially. A cache is an
    // optimisation K5 permits, not one it requires, and adding one before there is a measured reason
    // would be adding an invalidation bug for free.
    //
    // The assertion is therefore that a consumer which sees *no* events still reads the truth.
    let tenant = "dropped-events";
    let Some(store) = store(tenant) else {
        skipped("dropped change notification");
        return;
    };

    let aor = CanonicalAor::parse(bytes::Bytes::from_static(b"sip:quiet@conformance.example"))
        .expect("a valid AoR");
    store
        .commit(tenant, &aor, Revision::INITIAL, BindingSet::new())
        .expect("a commit");

    // Ignore the stream entirely, the way a consumer whose NOTIFY was lost would.
    let (_, revision) = store.read(tenant, &aor);
    assert_eq!(
        revision,
        Revision(1),
        "an authoritative read is current regardless of what the stream delivered"
    );
}

/// A registration storm: every device re-registering at once after an outage.
///
/// Reported rather than asserted against a threshold. A pass/fail number here would encode this
/// machine's disk into the test suite, and the useful output is the shape — whether one REGISTER is
/// one round trip, and whether refresh writes need coalescing.
#[test]
fn a_registration_storm_is_measured_and_reported() {
    const DEVICES: usize = 500;

    let tenant = "storm";
    let Some(store) = store(tenant) else {
        skipped("registration storm");
        return;
    };
    let policy = TenantPolicy::default();

    let make = |index: usize, cseq: u32, now: u64| {
        let aor = CanonicalAor::parse(bytes::Bytes::from(format!(
            "sip:device-{index}@conformance.example"
        )))
        .expect("a valid AoR");
        sipx_clstr_registrar::RegisterCommand {
            tenant: tenant.to_owned(),
            aor,
            call_id: bytes::Bytes::from(format!("storm-{index}")),
            cseq,
            contacts: sipx_clstr_registrar::ContactOps::Explicit(vec![
                sipx_clstr_registrar::ContactOp {
                    uri: sipx_sip::Uri::parse(bytes::Bytes::from(format!("sip:d{index}@10.1.0.1")))
                        .expect("a URI"),
                    verbatim: bytes::Bytes::from(format!("sip:d{index}@10.1.0.1")),
                    expires: Some(3_600),
                    q: None,
                    instance_id: None,
                    reg_id: None,
                    push: None,
                },
            ]),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(now),
        }
    };

    let started = std::time::Instant::now();
    for index in 0..DEVICES {
        let applied = apply(&store, &make(index, 1, 0), &policy, 3);
        assert!(applied.outcome.commits(), "device {index} should register");
    }
    let cold = started.elapsed();

    // The storm's second half: every device refreshes. This is the shape that matters — a refresh is
    // a write, so an outage that re-registers the estate writes the estate.
    let started = std::time::Instant::now();
    for index in 0..DEVICES {
        let applied = apply(&store, &make(index, 2, 60), &policy, 3);
        assert!(applied.outcome.commits(), "device {index} should refresh");
    }
    let refresh = started.elapsed();

    let per_second = |elapsed: std::time::Duration| {
        if elapsed.is_zero() {
            f64::INFINITY
        } else {
            f64::from(u32::try_from(DEVICES).unwrap_or(u32::MAX)) / elapsed.as_secs_f64()
        }
    };

    println!(
        "registration storm: {DEVICES} devices — first registration {:?} ({:.0}/s), \
         refresh {:?} ({:.0}/s)",
        cold,
        per_second(cold),
        refresh,
        per_second(refresh)
    );

    // What *is* asserted: one REGISTER costs one read and one write, and nothing about the estate's
    // size changes that. A backend that scanned the table per registration would show the refresh
    // pass degrading against the first; this bounds the ratio loosely enough not to be a
    // machine-speed assertion and tightly enough to catch an accidental O(n) read path.
    let ratio = refresh.as_secs_f64() / cold.as_secs_f64().max(f64::MIN_POSITIVE);
    assert!(
        ratio < 4.0,
        "refreshing an estate of {DEVICES} should cost about what registering it did, \
         got {ratio:.1}× ({cold:?} then {refresh:?}) — a per-registration cost that grows with the \
         estate means the read path is scanning"
    );
}
