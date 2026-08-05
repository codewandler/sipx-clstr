//! `RG-17` — a `PostgreSQL` read this node cannot complete, against the real database.
//!
//! [location-service §6.1 K7](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md):
//! a store that cannot be read is not an empty store. The rows here are the two faults that reach the
//! authoritative read — the database refusing the query, and a stored set this node cannot decode —
//! crossed with the two REGISTER shapes that **never commit**, so no later compare-and-swap can
//! discover the fault on their behalf: §10.2.3's query, and the removal of a contact that is not
//! bound.
//!
//! Those two shapes are the point. A mutation eventually meets the revision predicate and a bogus
//! revision-zero commit is fenced by it, which is what
//! `postgres_store.rs` used to argue from; `Outcome::Noop` returns before the commit and has no such
//! fence, so before this story a query against an unreadable row answered `200 OK` with an empty
//! Contact list — a UA is told it is registered nowhere, and a deregistration is told it succeeded,
//! while the durable state is simply unknown.
//!
//! # Running it
//!
//! ```sh
//! docker run -d --rm -e POSTGRES_PASSWORD=sipx -e POSTGRES_DB=sipx -p 55432:5432 postgres:16-alpine
//! SIPX_CLSTR_TEST_DATABASE_URL=postgres://postgres:sipx@127.0.0.1:55432/sipx \
//!   cargo test -p sipx-clstr-node --features postgres
//! ```
//!
//! Without that variable the tests **skip and say so**, for the reason `postgres_store.rs` states: a
//! skip that looks like a pass is how a backend stops being tested without anyone deciding to.

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use postgres::{Client, NoTls};
use sipx_clstr_node::postgres_store::PostgresStore;
use sipx_clstr_registrar::{
    CanonicalAor, ContactOp, ContactOps, RegisterCommand, TenantPolicy, Timestamp, apply,
};

const URL_VAR: &str = "SIPX_CLSTR_TEST_DATABASE_URL";
const RETRIES: usize = 3;

/// The database this run may use, or `None` when none was configured.
fn url() -> Option<String> {
    std::env::var(URL_VAR).ok()
}

/// Announce a skip loudly enough that nobody mistakes it for coverage.
fn skipped(what: &str) {
    println!("SKIPPED {what}: set {URL_VAR} to run the PostgreSQL read faults");
}

fn admin(url: &str) -> Client {
    Client::connect(url, NoTls)
        .unwrap_or_else(|error| panic!("{URL_VAR} is set but connecting failed: {error:?}"))
}

/// A connected store with `tenant` emptied. Each test owns a tenant, for the reason
/// `postgres_store.rs` records: §5's serialization domain is `(tenant, aor)`, so sharing one would
/// have these tests contradict the independence they assert.
fn store(url: &str, tenant: &str) -> PostgresStore {
    let store = PostgresStore::connect(url)
        .unwrap_or_else(|error| panic!("{URL_VAR} is set but connecting failed: {error:?}"));
    store.truncate(tenant).expect("truncate");
    store
}

fn aor(tenant: &str) -> CanonicalAor {
    CanonicalAor::parse(bytes::Bytes::from(format!("sip:{tenant}@faults.example")))
        .expect("a valid AoR")
}

fn command(tenant: &str, call_id: &str, cseq: u32, contacts: Vec<ContactOp>) -> RegisterCommand {
    RegisterCommand {
        tenant: tenant.to_owned(),
        aor: aor(tenant),
        call_id: bytes::Bytes::from(call_id.to_owned()),
        cseq,
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

fn contact(uri: &str, expires: u32) -> ContactOp {
    ContactOp {
        uri: sipx_sip::Uri::parse(bytes::Bytes::from(uri.to_owned())).expect("a URI"),
        verbatim: bytes::Bytes::from(uri.to_owned()),
        expires: Some(expires),
        q: None,
        instance_id: None,
        reg_id: None,
        push: None,
    }
}

/// Register one contact, so the fault below is in **reading** a row that exists.
fn register(store: &PostgresStore, tenant: &str) {
    let applied = apply(
        store,
        &command(tenant, "fixture", 1, vec![contact("sip:d@10.0.0.1", 3_600)]),
        &TenantPolicy::default(),
        RETRIES,
    );
    assert_eq!(
        applied.outcome.status(),
        200,
        "the fixture registration must succeed before a read fault means anything"
    );
}

/// The decode fault: the row is there and this node cannot read it as a binding set.
///
/// This is a deployment fault rather than a connectivity one — whatever wrote the row and this node
/// disagree about the schema — which is exactly why it must not be answered as absence.
fn make_undecodable(url: &str, tenant: &str) {
    let changed = admin(url)
        .execute(
            "UPDATE location_bindings SET bindings = '\"not a binding set\"'::jsonb
              WHERE tenant = $1",
            &[&tenant],
        )
        .expect("the corruption itself must succeed");
    assert_eq!(
        changed, 1,
        "exactly the fixture row must have been corrupted, or this test proves nothing"
    );
}

/// The same server, a database of this test's own.
///
/// The read-refusal fault below drops the table, and the table name is fixed — so it gets a database
/// nobody else is in, rather than breaking every test running beside it.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = url.split_once('?').map_or((url, ""), |split| split);
    let root = base.rsplit_once('/').map_or(base, |(head, _)| head);
    if query.is_empty() {
        format!("{root}/{name}")
    } else {
        format!("{root}/{name}?{query}")
    }
}

#[test]
fn rg17_a_register_query_cannot_answer_200_from_a_row_it_cannot_decode() {
    let tenant = "rg17-query-decode";
    let Some(url) = url() else {
        skipped("register query against an undecodable row");
        return;
    };
    let store = store(&url, tenant);
    register(&store, tenant);
    make_undecodable(&url, tenant);

    // RFC 3261 §10.2.3 — no `Contact` at all is a *query*. It mutates nothing, so nothing downstream
    // of the read can notice that the read failed.
    let applied = apply(
        &store,
        &command(tenant, "query", 2, Vec::new()),
        &TenantPolicy::default(),
        RETRIES,
    );
    assert_eq!(
        applied.outcome.status(),
        503,
        "a query served from a row this node cannot decode answers 200 with no contacts — \
         it tells the UA it is registered nowhere while the durable state is unknown"
    );
}

#[test]
fn rg17_a_no_op_deregistration_cannot_answer_200_from_a_row_it_cannot_decode() {
    let tenant = "rg17-remove-decode";
    let Some(url) = url() else {
        skipped("no-op deregistration against an undecodable row");
        return;
    };
    let store = store(&url, tenant);
    register(&store, tenant);
    make_undecodable(&url, tenant);

    // §5.3 B1 — removing a contact that is not bound is *ignored*, so this REGISTER is a `Noop` and
    // returns before any commit. Read as an empty set, the removal looks like it succeeded.
    let applied = apply(
        &store,
        &command(tenant, "remove", 2, vec![contact("sip:gone@10.0.0.2", 0)]),
        &TenantPolicy::default(),
        RETRIES,
    );
    assert_eq!(
        applied.outcome.status(),
        503,
        "a deregistration that never reached the store must not be reported as done"
    );
}

#[test]
fn rg17_a_register_query_cannot_answer_200_when_the_database_refuses_the_read() {
    let tenant = "rg17-query-database";
    let Some(url) = url() else {
        skipped("register query against a refused read");
        return;
    };
    let scratch = "rg17_read_faults";
    let scoped = with_database(&url, scratch);

    let mut server = admin(&url);
    server
        .batch_execute(&format!("DROP DATABASE IF EXISTS {scratch} WITH (FORCE)"))
        .expect("a previous run's scratch database must not survive");
    server
        .batch_execute(&format!("CREATE DATABASE {scratch}"))
        .expect("a scratch database");

    let store = store(&scoped, tenant);
    register(&store, tenant);

    // The fault: the query the read issues stops being answerable. A dropped table is the cheapest
    // deterministic spelling of "the database refused"; a lost connection and a permission change
    // arrive at `read` as the same `postgres::Error`.
    admin(&scoped)
        .batch_execute("DROP TABLE location_bindings")
        .expect("the fault itself must succeed");

    let applied = apply(
        &store,
        &command(tenant, "query", 2, Vec::new()),
        &TenantPolicy::default(),
        RETRIES,
    );
    let status = applied.outcome.status();

    drop(store);
    let _ = server.batch_execute(&format!("DROP DATABASE IF EXISTS {scratch} WITH (FORCE)"));

    assert_eq!(
        status, 503,
        "a query the database refused to answer must not come back as a successful empty set"
    );
}
