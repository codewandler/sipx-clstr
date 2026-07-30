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

#[test]
fn ls_r_23_a_replayed_token_still_adds_a_contact_it_never_bound() {
    // How far B4's "no mutation" reaches, pinned so nobody reads it as "a spent token can never
    // commit" (§5.3.1 B4.3). B4 and B5 are decided per *matched* binding; a contact that matches
    // nothing is B1's, and B1 has no stored `(Call-ID, CSeq)` to compare against. So one request
    // that replays a spent token for CA and carries a new CB refreshes nothing and adds CB.
    //
    // It grants nothing the token was withholding: the same UA can register CB in a request
    // carrying only CB, which B1 accepts identically. What bounds an addition is authentication
    // (§5.1 S3/S4) and the quota (§5.5), never the ordering token.
    let store = InMemoryStore::new();
    let policy = policy();
    let first = Cmd::new("i1", 1, 0).contacts(vec![ca(Some(3_600))]).build();
    let (outcome, revision) = run(&store, &first, &policy);
    assert!(outcome.commits());
    assert_eq!(revision, Revision(1));
    let deadline = store
        .read(TENANT, &aor())
        .0
        .all()
        .first()
        .expect("CA is bound")
        .expires_at;

    let both = Cmd::new("i1", 1, 0)
        .delayed_by_millis(500)
        .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
        .build();
    let (outcome, revision) = run(&store, &both, &policy);
    assert!(outcome.commits(), "B1 adds CB, so the request is a write");
    assert_eq!(outcome.status(), 200);
    assert_eq!(revision, Revision(2), "the addition bumps the revision");
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.1:5060", "sip:alice@10.0.0.2:5060"],
        "the complete set, both bindings (§5.6)"
    );

    // And CA — the binding the spent token actually wrote — is untouched: B4 still means no
    // mutation of the matched binding, which is the guarantee that has to survive.
    assert_eq!(
        store
            .read(TENANT, &aor())
            .0
            .all()
            .iter()
            .find(|b| b.contact == Bytes::from_static(b"sip:alice@10.0.0.1:5060"))
            .expect("CA is still bound")
            .expires_at,
        deadline,
        "the replay must not have refreshed CA"
    );
}

/// Every stored contact of this address-of-record, in stored order.
fn stored_texts(store: &InMemoryStore) -> Vec<String> {
    store
        .read(TENANT, &aor())
        .0
        .all()
        .iter()
        .map(|binding| String::from_utf8_lossy(&binding.contact).into_owned())
        .collect()
}

#[test]
fn ls_r_24_two_removals_and_an_addition_commit_exactly_the_set_the_request_describes() {
    // **`RG-16`'s failing-first test**, and B6 (§5.3.2). One REGISTER, three operations, the first
    // of which *shortens* the set. A registrar that resolves every operation against one view
    // captured before the first mutation has CB's removal pointing past the end of the set it is
    // applied to, so CB is never removed and the request commits a binding set nobody described.
    //
    // Single-contact REGISTER cannot expose this: with one operation there is nothing to shift.
    let store = InMemoryStore::new();
    let policy = policy();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy,
    );
    assert!(outcome.commits(), "the fixture registers CA and CB");

    // A fresh Call-ID, so every operation is B2's — the ordering token is not what is under test.
    let (outcome, revision) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![ca(Some(0)), cb(Some(0)), cc(Some(3_600))])
            .build(),
        &policy,
    );

    assert_eq!(outcome.status(), 200);
    // The complete-set rule (§5.6) makes the response the first place a wrong set shows up.
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.3:5060"],
        "both removals and the addition, each against the set it was stated about"
    );
    // And the store agrees. A response describing a set that was never committed would be a
    // different defect wearing the same symptom, so the committed set is read back rather than
    // inferred from the answer.
    assert_eq!(stored_texts(&store), ["sip:alice@10.0.0.3:5060"]);
    assert_eq!(revision, Revision(2), "one request, one commit (K2)");
}

#[test]
fn ls_r_25_a_removal_ahead_of_a_refresh_does_not_move_the_refresh_onto_another_binding() {
    // The other half of B6, and the more damaging one: with a stale view the removal of CA leaves
    // CB's operation resolving to the entry that slid into CB's index — so CB's refresh overwrites
    // CC, the set loses a binding it was never asked to drop, and gains a duplicate of one it was.
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600)), cc(Some(3_600))])
            .build(),
        &policy,
    );
    let cc_before = store
        .read(TENANT, &aor())
        .0
        .all()
        .iter()
        .find(|binding| binding.contact == Bytes::from_static(b"sip:alice@10.0.0.3:5060"))
        .expect("CC is bound")
        .clone();

    let (outcome, _) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![ca(Some(0)), cb(Some(7_200))])
            .build(),
        &policy,
    );

    assert_eq!(outcome.status(), 200);
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.2:5060", "sip:alice@10.0.0.3:5060"],
        "CA gone, CB and CC still bound exactly once each"
    );
    // The refresh landed on CB, not on whatever the removal shifted into CB's former place.
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .expect("CB is listed")
            .expires,
        7_200
    );
    // CC was named by no operation, so nothing about it may have moved.
    assert_eq!(
        store
            .read(TENANT, &aor())
            .0
            .all()
            .iter()
            .find(|binding| binding.contact == Bytes::from_static(b"sip:alice@10.0.0.3:5060"))
            .expect("CC is still bound"),
        &cc_before,
        "CC is untouched, down to its Call-ID and deadline"
    );
}

#[test]
fn a_losing_writer_re_reconciles_a_multi_contact_register_against_fresh_state() {
    // §6 K1/K6 under B6. Reconciling against the set as each operation leaves it is a claim about
    // one `process` call; the CAS contract is the claim that the *whole* sequence re-runs against
    // what the winner committed. A writer that re-applied only its staged set would drop the
    // interloper's binding — a lost update dressed as a retry.
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy,
    );

    // The losing writer reads revision 1 and reconciles its three operations against that set.
    let (stale, stale_revision) = store.read(TENANT, &aor());
    let loser = Cmd::new("i2", 1, 10)
        .contacts(vec![
            ca(Some(0)),
            cb(Some(0)),
            contact("sip:alice@10.0.0.4:5060", Some(3_600)),
        ])
        .build();
    let Outcome::Commit { set: staged, .. } = process(&loser, &stale, &policy) else {
        panic!("the multi-contact command should commit");
    };

    // Meanwhile another node registers CC and wins the revision.
    let (winner, winner_revision) = run(
        &store,
        &Cmd::new("i9", 1, 5).contacts(vec![cc(Some(3_600))]).build(),
        &policy,
    );
    assert!(winner.commits());
    assert_eq!(winner_revision, Revision(2));

    // The loser's staged set is refused rather than overwriting — it was reconciled against a set
    // that no longer exists.
    let conflict = store
        .commit(TENANT, &aor(), stale_revision, staged)
        .expect_err("the spent revision must conflict");
    assert_eq!(conflict.current, Revision(2));

    // Its driver re-reads and re-runs the whole request against the winner's set: CA and CB gone,
    // CD added, and CC — which this request never named — still bound.
    let (outcome, revision) = run(&store, &loser, &policy);
    assert!(outcome.commits());
    assert_eq!(revision, Revision(3));
    assert_eq!(
        stored_texts(&store),
        ["sip:alice@10.0.0.3:5060", "sip:alice@10.0.0.4:5060"],
        "the retry committed against fresh state, not the stale set it staged"
    );
}

#[test]
fn ls_r_26_two_removals_commit_the_empty_set() {
    // B6 stripped to the case that carries nothing else: two bindings, one REGISTER removing both.
    // A registrar resolving both operations against a view captured before the first mutation has
    // CB's removal pointing one past the end of the set it just shortened, so CB survives a request
    // that named it — the UA believes it is deregistered and the proxy keeps forking to it.
    let store = InMemoryStore::new();
    let policy = policy();
    let (outcome, _) = run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600))])
            .build(),
        &policy,
    );
    assert!(outcome.commits(), "the fixture registers CA and CB");

    let (outcome, revision) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![ca(Some(0)), cb(Some(0))])
            .build(),
        &policy,
    );

    assert_eq!(outcome.status(), 200);
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        Vec::<&str>::new(),
        "the response lists no contacts, because nothing is bound (§5.6)"
    );
    assert_eq!(
        stored_texts(&store),
        Vec::<String>::new(),
        "the committed set is empty"
    );
    assert_eq!(revision, Revision(2), "one request, one commit (K2)");
}

#[test]
fn ls_r_27_a_refresh_ahead_of_a_removal_leaves_the_removal_resolving_to_its_own_contact() {
    // LS-R-25's operations in the opposite order, because the two fail differently. There the
    // removal came first and dragged the refresh onto another binding; here the refresh comes first
    // and must not move CA, so the removal that follows still resolves to CA rather than to
    // whatever a stale view or an unaligned re-parse would put in its place.
    let store = InMemoryStore::new();
    let policy = policy();
    run(
        &store,
        &Cmd::new("i1", 1, 0)
            .contacts(vec![ca(Some(3_600)), cb(Some(3_600)), cc(Some(3_600))])
            .build(),
        &policy,
    );
    let cc_before = store
        .read(TENANT, &aor())
        .0
        .all()
        .iter()
        .find(|binding| binding.contact == Bytes::from_static(b"sip:alice@10.0.0.3:5060"))
        .expect("CC is bound")
        .clone();

    let (outcome, _) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![cb(Some(7_200)), ca(Some(0))])
            .build(),
        &policy,
    );

    assert_eq!(outcome.status(), 200);
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        ["sip:alice@10.0.0.2:5060", "sip:alice@10.0.0.3:5060"],
        "CA gone, CB and CC still bound exactly once each"
    );
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .expect("CB is listed")
            .expires,
        7_200,
        "the refresh landed on CB and was granted what it asked for"
    );
    assert_eq!(
        store
            .read(TENANT, &aor())
            .0
            .all()
            .iter()
            .find(|binding| binding.contact == Bytes::from_static(b"sip:alice@10.0.0.3:5060"))
            .expect("CC is still bound"),
        &cc_before,
        "CC was named by no operation, so nothing about it may have moved"
    );
}

#[test]
fn ls_r_28_a_removal_of_a_contact_this_request_just_added_applies_rather_than_aborting() {
    // **B7's failing-first test.** Two §19.1.4-equivalent spellings in one REGISTER: the UA
    // registers its current contact and deregisters what it believes is a line-tagged predecessor.
    // The `line` parameter appears in only one of the two URIs, so §19.1.4 ignores it and they are
    // the same contact.
    //
    // Once B6 is honoured, the second operation matches the binding the first one *just inserted* —
    // and that binding carries this request's own Call-ID and CSeq, because this request wrote it.
    // Running B4/B5 against that token reads "same token, stored state is not what is asked for"
    // and aborts the whole request `500`, so the UA ends up unregistered and its retry with a fresh
    // CSeq fails identically. B2–B5 are ordering rules about the *previous* writer; there is no
    // previous writer here, and B6 has already fixed the order.
    let store = InMemoryStore::new();
    let policy = policy();

    let (outcome, revision) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![
                cc(Some(3_600)),
                contact("sip:alice@10.0.0.3:5060;line=7", Some(0)),
            ])
            .build(),
        &policy,
    );

    assert_eq!(
        outcome.status(),
        200,
        "a request naming one contact twice is not a second write under a spent token"
    );
    assert_eq!(
        stored_texts(&store),
        Vec::<String>::new(),
        "the last operation naming the contact wins, and it was a removal"
    );
    assert_eq!(
        contact_texts(outcome.accepted().expect("a 200")),
        Vec::<&str>::new(),
        "the response enumerates the set that actually holds (§5.6)"
    );
    // B9 — the two operations cancel, so the reconciled set is the set that was read and there is
    // nothing to commit. LS-R-32 is why the revision has to stay put here rather than at 1: the
    // retransmission of this request is indistinguishable from its first delivery, so a revision bump
    // on either one is a bump on every one.
    assert_eq!(revision, Revision::INITIAL);
}

#[test]
fn ls_r_29_a_second_operation_on_a_contact_this_request_added_replaces_it() {
    // The other half of B7, and the shape that shows the answer is *the last operation wins* rather
    // than *ignore the duplicate*: one contact twice, with different granted durations. Before B7
    // this aborted `500` for the reason LS-R-28 did; before B6 it committed the contact twice,
    // because the second operation never saw the first one's insert.
    let store = InMemoryStore::new();
    let policy = policy();

    let (outcome, revision) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![cc(Some(3_600)), cc(Some(7_200))])
            .build(),
        &policy,
    );

    assert_eq!(outcome.status(), 200);
    assert_eq!(
        stored_texts(&store),
        ["sip:alice@10.0.0.3:5060"],
        "one binding, not two"
    );
    assert_eq!(
        outcome
            .accepted()
            .expect("a 200")
            .contacts
            .first()
            .expect("CC is listed")
            .expires,
        7_200,
        "the later operation's grant is the one that holds"
    );
    assert_eq!(revision, Revision(1));
}

/// Nine contacts, so the default quota of ten has room for exactly one more binding.
///
/// Spelled out rather than generated because `contact` takes a `&'static str`: these are the bytes a
/// request would carry, and a row pinning a quota boundary should show the set that fills it.
const NINE: [&str; 9] = [
    "sip:alice@10.0.0.11:5060",
    "sip:alice@10.0.0.12:5060",
    "sip:alice@10.0.0.13:5060",
    "sip:alice@10.0.0.14:5060",
    "sip:alice@10.0.0.15:5060",
    "sip:alice@10.0.0.16:5060",
    "sip:alice@10.0.0.17:5060",
    "sip:alice@10.0.0.18:5060",
    "sip:alice@10.0.0.19:5060",
];

/// Fill `store` with [`NINE`] — one binding short of the default quota.
fn fill_to_one_short_of_the_quota(store: &InMemoryStore, policy: &TenantPolicy) {
    let (outcome, _) = run(
        store,
        &Cmd::new("i1", 1, 0)
            .contacts(NINE.iter().map(|text| contact(text, Some(3_600))).collect())
            .build(),
        policy,
    );
    assert_eq!(outcome.status(), 200, "nine bindings fit a quota of ten");
    assert_eq!(stored_texts(store).len(), 9, "the fixture holds nine");
}

/// The lifetime the one stored binding was granted, in seconds — B4.1's comparison base.
fn only_granted_secs(store: &InMemoryStore) -> u64 {
    let (set, _) = store.read(TENANT, &aor());
    let binding = set.all().first().expect("the stored binding");
    binding.refreshed_at.until(binding.expires_at).as_secs()
}

#[test]
fn ls_r_30_the_quota_is_a_test_on_the_committed_outcome_not_on_the_candidates() {
    // §5.5 makes the quota a test on the **committed outcome** — "a REGISTER whose committed outcome
    // would exceed it fails `403 Forbidden`", and "refreshes, replacements and removals never grow
    // the set and never trip the quota". A check computed before §5.3's operations are applied cannot
    // know that outcome, and the one this row retired was an *upper* bound on additions: it counted a
    // candidate unless it was equivalent to one already counted, so a chain B6 collapses onto a
    // single binding was counted as several. The answer was a `403` no UA can retry out of, for a
    // request the outcome permits.
    //
    // The premise is §19.1.4's non-transitivity, so pin it rather than assume it. `line` is not in
    // §19.1.4's user/ttl/method/maddr/transport list, so a `line` present in only one of two URIs is
    // ignored — the bare spelling is equivalent to both tagged ones — while two URIs that both carry
    // `line` must agree on it, so the tagged spellings are not equivalent to each other.
    let tagged_one = Uri::parse(Bytes::from_static(b"sip:alice@10.0.0.3:5060;line=1"))
        .expect("a valid contact URI");
    let bare = Uri::parse(Bytes::from_static(b"sip:alice@10.0.0.3:5060")).expect("a valid URI");
    let tagged_two = Uri::parse(Bytes::from_static(b"sip:alice@10.0.0.3:5060;line=2"))
        .expect("a valid contact URI");
    assert!(tagged_one.equivalent(&bare), "a ≡ b");
    assert!(bare.equivalent(&tagged_two), "b ≡ c");
    assert!(
        !tagged_one.equivalent(&tagged_two),
        "a ≢ c — equivalence is not transitive, which is the whole difficulty"
    );

    let store = InMemoryStore::new();
    let policy = policy();
    fill_to_one_short_of_the_quota(&store, &policy);

    // The chain `a, b, c`. B6 resolves each operation against the set the preceding ones left, so
    // `b` replaces the binding `a` added and `c` replaces it again: three operations, one binding.
    let (outcome, _) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![
                contact("sip:alice@10.0.0.3:5060;line=1", Some(3_600)),
                contact("sip:alice@10.0.0.3:5060", Some(3_600)),
                contact("sip:alice@10.0.0.3:5060;line=2", Some(3_600)),
            ])
            .build(),
        &policy,
    );
    assert_ne!(
        outcome.status(),
        403,
        "the committed outcome is ten active bindings, which the quota permits"
    );
    assert_eq!(outcome.status(), 200);
    assert_eq!(
        stored_texts(&store).len(),
        10,
        "three operations collapsed onto one binding, so the set grew by one"
    );

    // The other direction, and why a lower bound cannot simply be assumed: a chain that genuinely
    // commits two bindings must still be refused at the boundary. `a` and `c` are not equivalent, so
    // neither replaces the other and the outcome would be eleven.
    let full = InMemoryStore::new();
    fill_to_one_short_of_the_quota(&full, &policy);
    let (refused, revision) = run(
        &full,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![
                contact("sip:alice@10.0.0.3:5060;line=1", Some(3_600)),
                contact("sip:alice@10.0.0.3:5060;line=2", Some(3_600)),
            ])
            .build(),
        &policy,
    );
    assert_eq!(
        refused.status(),
        403,
        "two operations that commit two bindings do exceed the quota"
    );
    assert_eq!(
        stored_texts(&full).len(),
        9,
        "a refusal commits nothing (K2)"
    );
    assert_eq!(revision, Revision(1), "and does not move the revision");
}

#[test]
fn ls_r_31_a_retransmission_of_a_contact_named_twice_is_a_retry_not_a_stale_sequence() {
    // LS-R-29's own shape, delivered twice. §5.3's idempotency rule is stated per **binding** against
    // "the command's requested outcome", and under B6 the outcome this command requests for that
    // binding is the *later* operation's grant — 7200, which is exactly what the first delivery
    // stored. So the re-presentation is B4's retry: `200` with the current set, nothing committed,
    // the revision where it was.
    //
    // Deciding B4 against each operation's own grant instead compares the first operation's 3600
    // against a stored 7200, calls that a second write under a spent token and aborts the whole
    // request `500` — which would make the one request B7 exists to legalise the one request a UA
    // cannot retransmit.
    let store = InMemoryStore::new();
    let policy = policy();

    let (first, revision) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .contacts(vec![cc(Some(3_600)), cc(Some(7_200))])
            .build(),
        &policy,
    );
    assert_eq!(first.status(), 200);
    assert_eq!(revision, Revision(1));
    assert_eq!(
        only_granted_secs(&store),
        7_200,
        "the later operation's grant is what holds"
    );

    let (retry, after) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .delayed_by_millis(500)
            .contacts(vec![cc(Some(3_600)), cc(Some(7_200))])
            .build(),
        &policy,
    );
    assert_eq!(
        retry.status(),
        200,
        "a retransmission of this request is a retry, not a stale sequence"
    );
    assert!(!retry.commits(), "B4's remedy is no mutation");
    assert_eq!(
        after, revision,
        "a retry leaves the revision as it is (B4.2)"
    );
    assert_eq!(
        stored_texts(&store),
        ["sip:alice@10.0.0.3:5060"],
        "still one binding"
    );
    assert_eq!(
        only_granted_secs(&store),
        7_200,
        "and it was not rewritten by the retry"
    );
}

#[test]
fn ls_r_32_operations_that_cancel_out_commit_nothing_and_leave_the_revision_alone() {
    // LS-R-28's shape: an addition and a removal of one contact in a single request. The reconciled
    // set is the set that was read, so there is nothing to commit — and B4.2's "a retry leaves the
    // revision as it is" has to hold for the *second* delivery as well.
    //
    // The two deliveries are indistinguishable from the store's side: the set is empty before and
    // after each, and no binding survives to carry the ordering token, so nothing exists that could
    // tell them apart. The revision therefore has to stay put on both, which is what makes this
    // request idempotent rather than a revision bump per retransmission.
    let store = InMemoryStore::new();
    let policy = policy();
    let shape = || {
        vec![
            cc(Some(3_600)),
            contact("sip:alice@10.0.0.3:5060;line=7", Some(0)),
        ]
    };

    let (first, after_first) = run(
        &store,
        &Cmd::new("i2", 1, 10).contacts(shape()).build(),
        &policy,
    );
    assert_eq!(first.status(), 200);
    assert!(
        !first.commits(),
        "the reconciled set is the set that was read"
    );
    assert_eq!(
        after_first,
        Revision::INITIAL,
        "nothing durable changed, so no revision bump and no change event"
    );

    let (again, after_second) = run(
        &store,
        &Cmd::new("i2", 1, 10)
            .delayed_by_millis(500)
            .contacts(shape())
            .build(),
        &policy,
    );
    assert_eq!(again.status(), 200);
    assert_eq!(
        after_second,
        Revision::INITIAL,
        "and the retransmission is answered identically"
    );
    assert_eq!(
        stored_texts(&store),
        Vec::<String>::new(),
        "nothing is bound either way"
    );
}
