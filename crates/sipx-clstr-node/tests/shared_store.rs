//! Two nodes, one set of registrations (`RG-12`).
//!
//! `RG-4` built the `PostgreSQL` location service and proved it satisfies the store contract. What it
//! did not do — and what nothing did until this story — is make it reachable from a running node:
//! `driver::run` opened `InMemoryStore::new()` unconditionally. So two nodes were two islands, each
//! answering only for whoever happened to reach it, and "clustered registrar" was a claim with no
//! mechanism behind it.
//!
//! The discriminating test is `rg12_a_binding_written_by_one_node_is_visible_to_another`: a binding
//! written through one handle must be readable through a *different* handle to the same database.
//! That cannot pass on an in-process store, which is exactly the difference between two nodes and a
//! cluster.
//!
//! ```sh
//! SIPX_CLSTR_TEST_DATABASE_URL=postgres://postgres:sipx@127.0.0.1:55432/sipx \
//!   cargo test -p sipx-clstr-node --features postgres --test shared_store
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_node::driver::{NodeError, StoreChoice, open_store};
use sipx_clstr_registrar::{Binding, CanonicalAor, Timestamp};

const URL_VAR: &str = "SIPX_CLSTR_TEST_DATABASE_URL";
/// These rows are about *where* a binding lives, and every read below is against a store that is up.
/// §6 K7's failure path has its own tests (`postgres_read_faults.rs`), and absorbing a failure here
/// would let this file's emptiness assertions pass for the wrong reason.
const READS: &str = "the store under test must be readable";

fn dsn() -> Option<String> {
    std::env::var(URL_VAR).ok()
}

/// Announce a skip loudly enough that nobody mistakes it for coverage.
fn skipped(what: &str) {
    println!("SKIPPED {what}: set {URL_VAR} to run against a real database");
}

fn aor(uri: &str) -> CanonicalAor {
    CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
}

fn binding(contact: &str) -> Binding {
    Binding {
        contact: Bytes::copy_from_slice(contact.as_bytes()),
        q: 1000,
        call_id: Bytes::from_static(b"rg12-call"),
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

/// The default is what a single node needs, and it is what it had before this story.
#[test]
fn rg12_the_default_store_is_still_in_process() {
    assert_eq!(StoreChoice::default(), StoreChoice::InMemory);
    let store = open_store(&StoreChoice::InMemory).expect("the in-process store always opens");
    let (set, _revision) = store
        .read("t", &aor("sip:alice@example.test"))
        .expect(READS);
    assert!(
        set.all().is_empty(),
        "a fresh in-process store holds nothing"
    );
}

/// **A configured store that cannot be reached stops the node.**
///
/// Worth its own test. A registrar that fell back to an in-process store on a failed connection
/// would come up healthy, answer `200` to every REGISTER, and serve bindings no peer can see —
/// and nothing would say so. The failure would surface much later, as calls that do not arrive.
#[test]
#[cfg(feature = "postgres")]
fn rg12_an_unreachable_store_refuses_to_start() {
    let choice = StoreChoice::Postgres {
        // Port 1 is reserved; nothing listens there.
        dsn: "postgres://postgres:nope@127.0.0.1:1/absent".to_owned(),
    };
    match open_store(&choice) {
        Err(NodeError::LocationStoreUnreachable(why)) => {
            assert!(!why.is_empty(), "the refusal must say what went wrong");
        }
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("an unreachable store must not open, and must not fall back to memory"),
    }
}

/// A node asking for a backend this binary was not built with is refused, not quietly given another.
#[test]
#[cfg(not(feature = "postgres"))]
fn rg12_a_backend_not_compiled_in_is_refused() {
    let choice = StoreChoice::Postgres {
        dsn: "postgres://ignored".to_owned(),
    };
    assert!(matches!(
        open_store(&choice),
        Err(NodeError::LocationStoreUnreachable(_))
    ));
}

/// **The failing-first test for this story.** A binding written through one handle is visible through
/// another to the same database — which is what "two nodes, one registrar" means, and which an
/// in-process store cannot do at all.
#[test]
#[cfg(feature = "postgres")]
fn rg12_a_binding_written_by_one_node_is_visible_to_another() {
    let Some(dsn) = dsn() else {
        skipped("cross-node binding visibility");
        return;
    };
    let tenant = "rg12-cross-node";
    let alice = aor("sip:alice@example.test");

    // First, the red half — demonstrated rather than asserted in a comment. Two in-process stores
    // are what this node had before this story, and they do not share a single binding. If this
    // block ever passes, the test below has stopped discriminating and proves nothing.
    {
        let island_a = open_store(&StoreChoice::InMemory).expect("opens");
        let island_b = open_store(&StoreChoice::InMemory).expect("opens");
        let (set, revision) = island_a.read(tenant, &alice).expect(READS);
        let mut written = set.clone();
        written.insert(binding("sip:alice@198.51.100.7:5060"));
        island_a
            .commit(tenant, &alice, revision, written)
            .expect("the write lands locally");
        let (seen, _) = island_b.read(tenant, &alice).expect(READS);
        assert!(
            seen.all().is_empty(),
            "two in-process stores must NOT share bindings — that is the defect this story closes"
        );
    }

    // Two independently opened handles: separate connections, exactly as two nodes would have.
    let node_a = open_store(&StoreChoice::Postgres { dsn: dsn.clone() }).expect("node A opens");
    let node_b = open_store(&StoreChoice::Postgres { dsn: dsn.clone() }).expect("node B opens");

    sipx_clstr_node::postgres_store::PostgresStore::connect(&dsn)
        .expect("connect to truncate")
        .truncate(tenant)
        .expect("truncate");

    // Node A takes the registration.
    let (set, revision) = node_a.read(tenant, &alice).expect(READS);
    assert!(set.all().is_empty(), "the tenant starts empty");
    let mut updated = set.clone();
    updated.insert(binding("sip:alice@198.51.100.7:5060"));
    node_a
        .commit(tenant, &alice, revision, updated)
        .expect("node A commits the binding");

    // Node B, which never saw the REGISTER, must be able to route to her. This is the whole story.
    let (seen, _revision) = node_b.read(tenant, &alice).expect(READS);
    let contacts: Vec<String> = seen
        .all()
        .iter()
        .map(|b| String::from_utf8_lossy(&b.contact).into_owned())
        .collect();
    assert!(
        contacts.iter().any(|c| c.contains("198.51.100.7")),
        "node B must see the binding node A stored; saw {contacts:?}"
    );

    // And the compare-and-swap contract holds *across* them: node B writing against a revision it
    // read before node A's next commit is refused rather than merged. Two registrars racing for one
    // AoR cannot interleave into a set neither client asked for.
    let (b_set, b_revision) = node_b.read(tenant, &alice).expect(READS);
    let (a_set, a_revision) = node_a.read(tenant, &alice).expect(READS);

    let mut a_next = a_set.clone();
    a_next.insert(binding("sip:alice@198.51.100.8:5060"));
    node_a
        .commit(tenant, &alice, a_revision, a_next)
        .expect("node A's second write lands");

    let mut b_next = b_set.clone();
    b_next.insert(binding("sip:alice@203.0.113.9:5060"));
    assert!(
        node_b.commit(tenant, &alice, b_revision, b_next).is_err(),
        "a write against a stale revision must be refused, not merged"
    );
}
