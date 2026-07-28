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

    // B4 — the same command again writes nothing and answers with the current set.
    let again = apply(suite.store, &refresh, &policy(), RETRIES);
    suite.check(
        "LS-R-3",
        !again.outcome.commits() && again.revision == first.revision,
        || {
            format!(
                "an identical retry must not write: commits={} revision {:?} → {:?}",
                again.outcome.commits(),
                first.revision,
                again.revision
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
