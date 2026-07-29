//! The LS-R and LS-K vector tables of
//! [location-service §9](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md),
//! row by row.
//!
//! Vectors are normative. `RG-4` runs this same file against the `PostgreSQL` backend — a backend
//! that needs its own version of a row has broken the contract rather than implemented it, so the
//! store is reached only through the `LocationStore` trait here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_registrar::{
    Accepted, CanonicalAor, ContactOp, ContactOps, InMemoryStore, LocationStore, Outcome,
    RegisterCommand, Rejection, Revision, TenantPolicy, Timestamp, apply, process,
};
use sipx_sip::Uri;

const TENANT: &str = "t1";
const RETRIES: usize = 3;

fn aor() -> CanonicalAor {
    CanonicalAor::parse(Bytes::from_static(b"sip:alice@atlanta.example")).expect("a valid AoR")
}

/// CA, CB, CC — the three contacts the LS-R fixture uses.
fn contact(text: &'static str, expires: Option<u32>) -> ContactOp {
    ContactOp {
        uri: Uri::parse(Bytes::from_static(text.as_bytes())).expect("a valid contact URI"),
        verbatim: Bytes::from_static(text.as_bytes()),
        expires,
        q: None,
        instance_id: None,
        reg_id: None,
        push: None,
    }
}

fn ca(expires: Option<u32>) -> ContactOp {
    contact("sip:alice@10.0.0.1:5060", expires)
}
fn cb(expires: Option<u32>) -> ContactOp {
    contact("sip:alice@10.0.0.2:5060", expires)
}
fn cc(expires: Option<u32>) -> ContactOp {
    contact("sip:alice@10.0.0.3:5060", expires)
}

struct Cmd(RegisterCommand);

impl Cmd {
    fn new(call_id: &'static str, cseq: u32, now_secs: u64) -> Self {
        Self(RegisterCommand {
            tenant: TENANT.to_owned(),
            aor: aor(),
            call_id: Bytes::from_static(call_id.as_bytes()),
            cseq,
            contacts: ContactOps::Explicit(Vec::new()),
            expires_header: None,
            path: Vec::new(),
            supports_path: false,
            require: Vec::new(),
            received: None,
            flow_ref: None,
            principal: None,
            now: Timestamp::from_secs(now_secs),
        })
    }

    /// The same command, delivered `millis` later — a retransmission rather than a replay at the
    /// originating nanosecond. LS-R-3 states the delay, so "identical outcome" cannot be satisfied
    /// only by a zero-latency retry.
    fn delayed_by_millis(mut self, millis: u64) -> Self {
        self.0.now = Timestamp::from_nanos(self.0.now.as_nanos() + millis * 1_000_000);
        self
    }

    fn contacts(mut self, ops: Vec<ContactOp>) -> Self {
        self.0.contacts = ContactOps::Explicit(ops);
        self
    }

    fn wildcard(mut self) -> Self {
        self.0.contacts = ContactOps::Wildcard;
        self
    }

    fn expires_header(mut self, expires: u32) -> Self {
        self.0.expires_header = Some(expires);
        self
    }

    fn path(mut self, path: Vec<&'static str>) -> Self {
        self.0.path = path
            .into_iter()
            .map(|p| Bytes::from_static(p.as_bytes()))
            .collect();
        self
    }

    fn supports_path(mut self) -> Self {
        self.0.supports_path = true;
        self
    }

    fn require(mut self, tags: Vec<&str>) -> Self {
        self.0.require = tags.into_iter().map(str::to_owned).collect();
        self
    }

    fn build(self) -> RegisterCommand {
        self.0
    }
}

fn policy() -> TenantPolicy {
    TenantPolicy::default()
}

fn run(store: &InMemoryStore, cmd: &RegisterCommand, policy: &TenantPolicy) -> (Outcome, Revision) {
    let applied = apply(store, cmd, policy, RETRIES);
    (applied.outcome, applied.revision)
}

fn contact_texts(accepted: &Accepted) -> Vec<&str> {
    accepted
        .contacts
        .iter()
        .map(|c| std::str::from_utf8(&c.contact).unwrap_or("?"))
        .collect()
}

// ---------------------------------------------------------------------------- LS-R ------------

#[test]
fn ls_r_1_a_first_registration_commits_at_revision_one() {
    let store = InMemoryStore::new();
    let cmd = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();
    let (outcome, revision) = run(&store, &cmd, &policy());

    assert!(outcome.commits());
    assert_eq!(revision, Revision(1));
    let accepted = outcome.accepted().expect("a 200");
    assert_eq!(contact_texts(accepted), ["sip:alice@10.0.0.1:5060"]);
    assert_eq!(accepted.contacts.first().map(|c| c.expires), Some(3_600));
}

#[test]
fn ls_r_2_a_refresh_applies_and_bumps_the_revision() {
    let store = InMemoryStore::new();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy(),
    );
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i1", 2, 10)
            .contacts(vec![ca(Some(3_600))])
            .build(),
        &policy(),
    );
    assert!(outcome.commits(), "B3: a newer CSeq applies");
    assert_eq!(revision, Revision(2));
}

#[test]
fn ls_r_3_an_identical_retry_writes_nothing() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    let refresh = Cmd::new("i1", 2, 10)
        .contacts(vec![ca(Some(3_600))])
        .build();
    let (_, after_refresh) = run(&store, &refresh, &policy);
    let deadline = store
        .read(TENANT, &aor())
        .0
        .all()
        .first()
        .expect("the stored binding")
        .expires_at;

    // The same request again, 500 ms later — the ordinary UDP retransmission. Same token, same
    // granted *duration*, so B4 holds even though the deadline the second delivery would compute
    // is half a second further out.
    let retry = Cmd::new("i1", 2, 10)
        .delayed_by_millis(500)
        .contacts(vec![ca(Some(3_600))])
        .build();
    let (outcome, revision) = run(&store, &retry, &policy);
    assert!(!outcome.commits(), "B4: an idempotent retry must not write");
    assert_eq!(outcome.status(), 200);
    assert_eq!(revision, after_refresh, "the revision must not move");
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.1:5060"],
        "and it still answers with the current set"
    );

    // B4's remedy is *no mutation*, never an extension: a retry that pushed the deadline out would
    // make one ordering token spendable more than once.
    assert_eq!(
        store
            .read(TENANT, &aor())
            .0
            .all()
            .first()
            .expect("the stored binding")
            .expires_at,
        deadline,
        "a retry must not extend the binding"
    );
}

#[test]
fn ls_r_4_a_stale_cseq_aborts_and_leaves_the_store_untouched() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 2, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    let before = store.read(TENANT, &aor());

    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 10)
            .contacts(vec![ca(Some(3_600))])
            .build(),
        &policy,
    );
    assert!(matches!(outcome, Outcome::Reject(Rejection::StaleSequence)));
    assert_eq!(outcome.status(), 500);
    assert_eq!(
        store.read(TENANT, &aor()),
        before,
        "nothing may have changed"
    );
}

#[test]
fn ls_r_5_a_new_call_id_applies_regardless_of_cseq() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 9, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    // B2 — the UA restarted, and its sequence numbering restarted with it.
    let (outcome, _) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![ca(Some(3_600))])
            .build(),
        &policy,
    );
    assert!(outcome.commits(), "B2: a different Call-ID applies");
}

#[test]
fn ls_r_6_removing_one_binding_answers_with_the_survivors() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy,
    );
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 2, 10).contacts(vec![ca(Some(0))]).build(),
        &policy,
    );
    assert!(outcome.commits());
    // The complete-set rule (§5.6): the response enumerates what is left, not what changed.
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.2:5060"]
    );
}

#[test]
fn ls_r_7_a_valid_wildcard_removes_everything_and_answers_with_no_contacts() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    run(
        &store,
        &Cmd::new("i2", 1, 0).contacts(vec![cb(Some(3_600))]).build(),
        &policy,
    );

    let (outcome, revision) = run(
        &store,
        &Cmd::new("i9", 1, 10).wildcard().expires_header(0).build(),
        &policy,
    );
    assert!(outcome.commits());
    assert!(outcome.accepted().expect("a 200").contacts.is_empty());
    assert_eq!(revision, Revision(3), "the revision still moves");
    assert!(store.read(TENANT, &aor()).0.all().is_empty());
}

#[test]
fn ls_r_9_a_wildcard_without_expires_zero_is_malformed() {
    let store = InMemoryStore::new();
    let policy = policy();

    let (with_expiry, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .wildcard()
            .expires_header(3_600)
            .build(),
        &policy,
    );
    assert_eq!(with_expiry.status(), 400, "W2");

    let (without_header, _) = run(&store, &Cmd::new("i1", 1, 0).wildcard().build(), &policy);
    assert_eq!(without_header.status(), 400, "W2, absent header");
}

#[test]
fn ls_r_10_a_wildcard_with_a_spent_token_aborts_and_removes_nothing() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 5, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );

    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 5, 10).wildcard().expires_header(0).build(),
        &policy,
    );
    assert!(
        matches!(outcome, Outcome::Reject(Rejection::StaleSequence)),
        "W3"
    );
    assert_eq!(
        store
            .read(TENANT, &aor())
            .0
            .active_count(Timestamp::from_secs(10)),
        1
    );
}

#[test]
fn ls_r_11_too_brief_is_423_with_the_minimum_and_commits_nothing() {
    let store = InMemoryStore::new();
    let policy = TenantPolicy {
        min_expires: 300,
        ..TenantPolicy::default()
    };
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(60))]).build(),
        &policy,
    );
    match outcome {
        Outcome::Reject(Rejection::IntervalTooBrief { min }) => assert_eq!(min, 300),
        other => panic!("expected 423, got {other:?}"),
    }
    assert_eq!(revision, Revision::INITIAL, "nothing committed");
}

#[test]
fn ls_r_12_over_the_maximum_is_silently_lowered_and_the_response_says_so() {
    let store = InMemoryStore::new();
    let policy = TenantPolicy {
        max_expires: 7_200,
        ..TenantPolicy::default()
    };
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(None)])
            .expires_header(86_400)
            .build(),
        &policy,
    );
    assert!(outcome.commits(), "E5 shortens rather than refusing");
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .map(|c| c.expires),
        Some(7_200),
        "the UA must be told what it actually got, or it refreshes too late"
    );
}

#[test]
fn ls_r_13_the_contact_parameter_beats_the_expires_header() {
    let store = InMemoryStore::new();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(1_800))])
            .expires_header(600)
            .build(),
        &policy(),
    );
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .map(|c| c.expires),
        Some(1_800),
        "E1 > E2"
    );
}

#[test]
fn ls_r_14_no_expiry_anywhere_takes_the_tenant_default() {
    let store = InMemoryStore::new();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(None)]).build(),
        &policy(),
    );
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .map(|c| c.expires),
        Some(3_600),
        "E3"
    );
}

#[test]
fn ls_r_15_the_quota_refuses_a_new_binding_but_never_a_refresh() {
    let store = InMemoryStore::new();
    let policy = TenantPolicy {
        max_bindings_per_aor: 2,
        ..TenantPolicy::default()
    };
    run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy,
    );

    let (refused, _) = run(
        &store,
        &Cmd::new("i1", 2, 10)
            .contacts(vec![cc(Some(3_600))])
            .build(),
        &policy,
    );
    assert_eq!(
        refused.status(),
        403,
        "a policy refusal, not a server fault"
    );

    // A refresh cannot grow the set, so it cannot trip the quota.
    let (refresh, _) = run(
        &store,
        &Cmd::new("i1", 3, 20)
            .contacts(vec![cb(Some(3_600))])
            .build(),
        &policy,
    );
    assert_eq!(refresh.status(), 200);
}

#[test]
fn ls_r_16_one_too_brief_contact_fails_the_whole_request() {
    // Atomicity (E6, K2): partial commitment is what §10.3's "completely or not at all" forbids,
    // and it is the failure mode a per-contact loop falls into naturally.
    let store = InMemoryStore::new();
    let policy = TenantPolicy {
        min_expires: 300,
        ..TenantPolicy::default()
    };
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![cb(Some(3_600)), cc(Some(60))])
            .build(),
        &policy,
    );
    assert_eq!(outcome.status(), 423);
    assert_eq!(revision, Revision::INITIAL);
    assert!(
        store.read(TENANT, &aor()).0.all().is_empty(),
        "neither contact may have committed"
    );
}

#[test]
fn ls_r_17_a_path_is_stored_topmost_first_and_echoed_in_order() {
    let store = InMemoryStore::new();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600))])
            .path(vec!["<sip:p2.example;lr>", "<sip:p1.example;lr>"])
            .supports_path()
            .build(),
        &policy(),
    );
    assert!(outcome.commits());
    assert_eq!(
        outcome.accepted().expect("a 200").path,
        vec![
            Bytes::from_static(b"<sip:p2.example;lr>"),
            Bytes::from_static(b"<sip:p1.example;lr>"),
        ],
        "RFC 3327 §5.2 accumulates like Record-Route: topmost is nearest the registrar"
    );
}

#[test]
fn ls_r_18_a_path_from_a_uac_that_did_not_advertise_it_is_refused() {
    let store = InMemoryStore::new();
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600))])
            .path(vec!["<sip:p1.example;lr>"])
            .build(),
        &policy(),
    );
    match outcome {
        Outcome::Reject(Rejection::ExtensionRequired(tag)) => assert_eq!(tag, "path"),
        other => panic!("expected 421, got {other:?}"),
    }
    assert_eq!(revision, Revision::INITIAL, "nothing committed");
}

#[test]
fn ls_r_19_an_equivalent_contact_refreshes_rather_than_duplicating() {
    // §19.1.4 comparison, not byte comparison: `sip:c@h;x=1` and `sip:c@h` are equivalent because
    // an unknown parameter present on one side and absent on the other does not break equivalence.
    let store = InMemoryStore::new();
    let policy = policy();
    let with_param = contact("sip:c@h.example;x=1", Some(3_600));
    let without = contact("sip:c@h.example", Some(3_600));

    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![with_param]).build(),
        &policy,
    );
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 2, 10).contacts(vec![without]).build(),
        &policy,
    );

    assert!(outcome.commits());
    assert_eq!(
        outcome.accepted().expect("a 200").contacts.len(),
        1,
        "one binding, refreshed — not two"
    );
}

#[test]
fn ls_r_20_an_unknown_require_tag_is_420_with_the_offender_named() {
    let store = InMemoryStore::new();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600))])
            .require(vec!["nothing-we-know"])
            .build(),
        &policy(),
    );
    match outcome {
        Outcome::Reject(Rejection::BadExtension(tags)) => {
            assert_eq!(tags, vec!["nothing-we-know".to_owned()]);
        }
        other => panic!("expected 420, got {other:?}"),
    }
}

#[test]
fn ls_r_20_a_require_tag_we_do_implement_is_accepted() {
    let store = InMemoryStore::new();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600))])
            .require(vec!["path"])
            .build(),
        &policy(),
    );
    assert_eq!(outcome.status(), 200);
}

#[test]
fn ls_r_21_an_expired_binding_is_absent_so_an_old_cseq_adds_a_fresh_one() {
    let store = InMemoryStore::new();
    let policy = policy();
    // CA registered for 100s at t=0 under CSeq 9, then let go.
    run(
        &store,
        &Cmd::new("i1", 9, 0).contacts(vec![ca(Some(100))]).build(),
        &policy,
    );

    // At t=200 it is gone. A REGISTER with an *older* CSeq is not stale against anything, because
    // there is nothing left to compare against (§5.3).
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 3, 200)
            .contacts(vec![ca(Some(3_600))])
            .build(),
        &policy,
    );
    assert!(outcome.commits(), "B1: added fresh, not aborted");
    assert_eq!(outcome.accepted().expect("a 200").contacts.len(), 1);
}

#[test]
fn a_register_with_no_contacts_is_a_query_and_changes_nothing() {
    // RFC 3261 §10.2.3. Not in the vector table, but the row it would occupy: a query must answer
    // with the complete set and must not move the revision.
    let store = InMemoryStore::new();
    let policy = policy();
    let (_, after_write) = run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );

    let (outcome, revision) = run(&store, &Cmd::new("i2", 1, 10).build(), &policy);
    assert!(!outcome.commits());
    assert_eq!(revision, after_write);
    assert_eq!(outcome.accepted().expect("a 200").contacts.len(), 1);
}

// ---------------------------------------------------------------------------- LS-K ------------

#[test]
fn ls_k_1_two_writers_serialize_and_neither_update_is_lost() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );

    // Both commands read the same revision, the way two nodes racing on one AoR would.
    let (set, revision) = store.read(TENANT, &aor());
    let first = Cmd::new("i1", 2, 10)
        .contacts(vec![ca(Some(3_600))])
        .build();
    let second = Cmd::new("i2", 1, 10)
        .contacts(vec![cb(Some(3_600))])
        .build();

    let first_outcome = process(&first, &set, &policy);
    let second_outcome = process(&second, &set, &policy);

    let Outcome::Commit { set: first_set, .. } = first_outcome else {
        panic!("the first command should commit");
    };
    let Outcome::Commit {
        set: second_set, ..
    } = second_outcome
    else {
        panic!("the second command should commit");
    };

    // The first commit wins the revision.
    assert_eq!(
        store.commit(TENANT, &aor(), revision, first_set),
        Ok(Revision(2))
    );

    // The second, holding a spent revision, is refused rather than overwriting.
    let conflict = store
        .commit(TENANT, &aor(), revision, second_set)
        .expect_err("the second commit must conflict");
    assert_eq!(conflict.expected, revision);
    assert_eq!(conflict.current, Revision(2));

    // Its driver re-reads and re-processes, and now both bindings exist: no lost update.
    let (outcome, final_revision) = run(&store, &second, &policy);
    assert!(outcome.commits());
    assert_eq!(final_revision, Revision(3));
    assert_eq!(outcome.accepted().expect("a 200").contacts.len(), 2);
}

#[test]
fn ls_k_2_a_conflicting_retry_becomes_a_noop_after_the_re_read() {
    // CAS and idempotency compose: the loop re-presents the command, and B4 turns the second
    // presentation into an answer rather than a second write.
    let store = InMemoryStore::new();
    let policy = policy();
    let cmd = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();

    let (first, first_revision) = run(&store, &cmd, &policy);
    assert!(first.commits());

    let (second, second_revision) = run(&store, &cmd, &policy);
    assert!(!second.commits(), "the same command must not write twice");
    assert_eq!(second.status(), 200);
    assert_eq!(second_revision, first_revision);
}

#[test]
fn ls_k_3_and_k_4_every_commit_emits_one_monotonic_change() {
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    run(
        &store,
        &Cmd::new("i1", 2, 10)
            .contacts(vec![cb(Some(3_600))])
            .build(),
        &policy,
    );
    // A Noop must not emit: a consumer that re-read on every retry would amplify a retry storm.
    run(
        &store,
        &Cmd::new("i1", 2, 10)
            .contacts(vec![cb(Some(3_600))])
            .build(),
        &policy,
    );

    let changes = store.changes();
    assert_eq!(changes.len(), 2, "one event per commit, none per Noop");
    let revisions: Vec<Revision> = changes.iter().map(|c| c.revision).collect();
    assert_eq!(revisions, vec![Revision(1), Revision(2)]);
    // K4 carries no payload — a consumer re-reads — so there is nothing here to go stale.
    assert_eq!(changes.first().map(|c| c.aor.clone()), Some(aor()));
}

#[test]
fn ls_k_3_the_revision_survives_the_set_going_empty() {
    // K3: revisions never reset, including across an empty set. RG-5's handoff fences on this
    // counter, so a reset would un-fence the handoff as well as every cache.
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy,
    );
    let (_, after_wildcard) = run(
        &store,
        &Cmd::new("i9", 1, 10).wildcard().expires_header(0).build(),
        &policy,
    );
    assert_eq!(after_wildcard, Revision(2));

    let (_, after_reregister) = run(
        &store,
        &Cmd::new("i3", 1, 20)
            .contacts(vec![ca(Some(3_600))])
            .build(),
        &policy,
    );
    assert_eq!(after_reregister, Revision(3), "not back to 1");
    assert_eq!(
        store.rows(),
        1,
        "the empty row persists to hold the counter"
    );
}

#[test]
fn ls_k_5_a_multi_binding_replacement_is_one_revision() {
    // K2: no reader observes a mix. With one revision per command that is structural rather than
    // something a reader has to be careful about.
    let store = InMemoryStore::new();
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy(),
    );
    assert!(outcome.commits());
    assert_eq!(revision, Revision(1));
    assert_eq!(store.read(TENANT, &aor()).0.all().len(), 2);
}

#[test]
fn ls_k_6_an_authoritative_read_sees_the_commit_it_follows() {
    let store = InMemoryStore::new();
    let (_, revision) = run(
        &store,
        &Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build(),
        &policy(),
    );
    assert_eq!(store.read(TENANT, &aor()).1, revision);
}

#[test]
fn s10_exhausted_cas_retries_answer_503_rather_than_looping() {
    /// A store that always reports someone else got there first.
    #[derive(Debug)]
    struct AlwaysConflicts;

    impl LocationStore for AlwaysConflicts {
        fn read(
            &self,
            _tenant: &str,
            _aor: &CanonicalAor,
        ) -> (sipx_clstr_registrar::BindingSet, Revision) {
            (sipx_clstr_registrar::BindingSet::new(), Revision(1))
        }

        fn commit(
            &self,
            _tenant: &str,
            _aor: &CanonicalAor,
            expected: Revision,
            _set: sipx_clstr_registrar::BindingSet,
        ) -> Result<Revision, sipx_clstr_registrar::CasConflict> {
            Err(sipx_clstr_registrar::CasConflict {
                expected,
                current: Revision(99),
            })
        }

        fn lookup(
            &self,
            _tenant: &str,
            _aor: &CanonicalAor,
            _now: Timestamp,
        ) -> Vec<sipx_clstr_registrar::Target> {
            Vec::new()
        }

        fn changes(&self) -> Vec<sipx_clstr_registrar::Change> {
            Vec::new()
        }
    }

    let cmd = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();
    let applied = apply(&AlwaysConflicts, &cmd, &policy(), 3);
    assert!(matches!(
        applied.outcome,
        Outcome::Reject(Rejection::Unavailable)
    ));
    assert_eq!(applied.outcome.status(), 503);
    assert_eq!(
        applied.conflicts, 4,
        "three retries, then the fourth gives up"
    );
}

#[test]
fn commands_for_different_address_of_records_never_serialize_against_each_other() {
    // §10.3's independence requirement: two AoRs are two serialization domains, so a busy one
    // cannot make a quiet one conflict.
    let store = InMemoryStore::new();
    let policy = policy();
    let other = CanonicalAor::parse(Bytes::from_static(b"sip:bob@atlanta.example")).expect("AoR");

    let mut alice = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();
    let mut bob = Cmd::new("i2", 1, 0).contacts(vec![cb(Some(3_600))]).build();
    bob.aor = other.clone();
    alice.aor = aor();

    let alice_revision = apply(&store, &alice, &policy, RETRIES).revision;
    let bob_revision = apply(&store, &bob, &policy, RETRIES).revision;
    assert_eq!(alice_revision, Revision(1));
    assert_eq!(bob_revision, Revision(1), "its own counter, from zero");
}

// ------------------------------------------------------- an edge the spec leaves sharp ---------

#[test]
fn ls_r_22_a_re_presentation_asking_for_a_different_duration_is_not_a_retry() {
    // The edge §5.3.1 keeps sharp. B4.1 makes the *granted duration* the base, so a copy of one
    // REGISTER arriving later — a retransmission, a CAS re-read, a re-presentation at a second
    // node — is a retry however long it took (LS-R-3). What is still refused is a spent token
    // asking for something else: same Call-ID, same CSeq, a different granted lifetime is a second
    // write, and B5 aborts it. Without this the carve-out would be "the token alone", which is the
    // one thing RFC 3261 §10.3 step 7 does not allow.
    let store = InMemoryStore::new();
    let policy = policy();
    let first = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();
    let (outcome, _) = run(&store, &first, &policy);
    assert!(outcome.commits());

    let later = Cmd::new("i1", 1, 0)
        .delayed_by_millis(500)
        .contacts(vec![ca(Some(7_200))])
        .build();
    let (outcome, _) = run(&store, &later, &policy);
    assert_eq!(
        outcome.status(),
        500,
        "same token, a different granted duration: a second write, not a retry"
    );
    assert_eq!(
        store.read(TENANT, &aor()).0.all().len(),
        1,
        "and nothing was written"
    );
}
