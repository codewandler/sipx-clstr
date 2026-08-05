//! The store: the compare-and-swap contract, and the in-memory backend.
//!
//! RFC 3261 §10.3 requires REGISTERs for one address-of-record to be processed **in order** and
//! **atomically**, and independently of other address-of-records. This crate gets that from a per-AoR revision
//! check rather than from any backend's locking, which is what makes the guarantee
//! backend-agnostic: the PostgreSQL backend (`RG-4`) satisfies the same trait with an `UPDATE …
//! WHERE revision = $3` and passes this same vector suite.
//!
//! Specification: [location-service §6](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::aor::CanonicalAor;
use crate::binding::{BindingSet, Revision, Timestamp};
use crate::command::{Outcome, RegisterCommand, Rejection, TenantPolicy};
use crate::lookup::{Target, order_targets};
use crate::process::process;

/// The commit lost a race: someone else wrote this AoR first.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the address-of-record moved to revision {current} while this command held {expected}")]
pub struct CasConflict {
    /// The revision the caller expected.
    pub expected: Revision,
    /// The revision the store is actually at.
    pub current: Revision,
}

/// Why an authoritative read could not answer (§6 K7).
///
/// Distinct from absence on purpose, and that is the whole of this type's job: `Ok((∅, Revision(0)))`
/// says the store was read and holds nothing for this address-of-record, while a `ReadFailure` says
/// the durable state is **unknown**. Answering the second as the first is what lets a query or a
/// no-op removal come back `200 OK` describing bindings nobody looked at.
///
/// Two variants because they are different operational faults — one is connectivity, the other says
/// this node and whatever wrote the row disagree about the schema — and one refusal, because §5.1 S10
/// has the same answer for both and no caller has a remedy that differs between them. The detail is a
/// `String` rather than a backend's own error type: this crate is sans-IO and never learns what a
/// database or a codec is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadFailure {
    /// The backend could not be reached, or refused the query.
    #[error("the location store could not be read: {0}")]
    Unavailable(String),
    /// A stored binding set could not be decoded.
    #[error("a stored binding set could not be decoded: {0}")]
    Undecodable(String),
}

/// A change notification (§6 K4).
///
/// Carries no binding payload on purpose: a consumer re-reads. An event that shipped state would
/// be a second, weaker replication path whose staleness nobody bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Which tenant.
    pub tenant: String,
    /// Which address-of-record.
    pub aor: CanonicalAor,
    /// The revision it moved to.
    pub revision: Revision,
}

/// Where bindings live.
pub trait LocationStore {
    /// The latest committed set and its revision, or why it could not be read.
    ///
    /// Staleness here is a liveness concern only: a stale read shows up as a [`CasConflict`] and a
    /// retry, never as a lost update (§6 K6). **Failure is not staleness**, which is why it is a
    /// [`ReadFailure`] and not an empty set: K6's fencing argument only reaches commands that commit,
    /// and the ones that answer without committing — a query, an idempotent retry, the removal of a
    /// contact that is not bound — would carry the invented absence all the way to the UA (§6 K7).
    ///
    /// An address-of-record with no bindings is `Ok((BindingSet::new(), Revision::INITIAL))`, and
    /// that is the only thing revision zero ever means.
    fn read(&self, tenant: &str, aor: &CanonicalAor)
    -> Result<(BindingSet, Revision), ReadFailure>;

    /// Install `set` iff the stored revision is still `expected`, atomically.
    fn commit(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: BindingSet,
    ) -> Result<Revision, CasConflict>;

    /// The forking-ordered target set for an AoR at `now` (§7), or why it could not be read.
    ///
    /// §7 L8: the failure is not L5's empty set. An empty `Ok` means this platform looked and the
    /// address-of-record has no active binding — the proxy's `480`; an `Err` means it could not look
    /// — the proxy's `503`.
    fn lookup(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        now: Timestamp,
    ) -> Result<Vec<Target>, ReadFailure>;

    /// Every change committed so far, oldest first.
    ///
    /// A real backend streams these (`LISTEN`/`NOTIFY` for PostgreSQL) and delivery is best-effort
    /// by contract; the in-memory backend keeps them so a test can assert what was emitted.
    fn changes(&self) -> Vec<Change>;
}

/// How many times the driver loop re-reads and re-processes before giving up (§5.1 S10).
pub const DEFAULT_CAS_RETRIES: usize = 3;

/// What applying a command actually did.
#[derive(Debug, Clone)]
pub struct Applied {
    /// What was decided.
    pub outcome: Outcome,
    /// The revision the address-of-record ended at, when one was learned.
    ///
    /// `None` only when the authoritative read failed (§6 K7): nothing was read, so there is no
    /// revision to report and nothing may be inferred from its absence. Reporting `Revision::INITIAL`
    /// there would be the same invention this field exists to stop — revision zero is a real,
    /// readable state, and a failed read is not it.
    pub revision: Option<Revision>,
    /// How many CAS conflicts were absorbed on the way.
    ///
    /// Reported rather than hidden so a scenario can assert that a race *happened* — a
    /// serialization test that silently never raced would pass for the wrong reason.
    pub conflicts: usize,
}

/// Apply a REGISTER: read, decide, commit, and on conflict do it again.
///
/// The loop is safe to repeat because §5.3's idempotency rule makes a re-presented command a
/// no-op rather than a second write — which is the whole reason that rule is written as precisely
/// as it is.
///
/// Returns the outcome actually applied, and the revision the AoR ended at.
///
/// A read that fails ends this before `process` is ever called (§6 K7): the decision function is
/// pure in its inputs, so handing it an invented empty set produces a confidently wrong answer
/// rather than an error. §5.1 S10's `503` is that refusal, and it is reached identically by every
/// command shape — a query and an absent-contact removal never commit, so nothing downstream of the
/// read could have noticed.
pub fn apply<S: LocationStore + ?Sized>(
    store: &S,
    cmd: &RegisterCommand,
    policy: &TenantPolicy,
    retries: usize,
) -> Applied {
    let mut conflicts = 0;
    loop {
        let (current, revision) = match store.read(&cmd.tenant, &cmd.aor) {
            Ok(found) => found,
            Err(failure) => {
                tracing::error!(
                    tenant = %cmd.tenant,
                    aor = %cmd.aor.as_str(),
                    %failure,
                    "the location store could not be read; refusing the REGISTER"
                );
                return Applied {
                    outcome: Outcome::Reject(Rejection::Unavailable),
                    revision: None,
                    conflicts,
                };
            }
        };
        let outcome = process(cmd, &current, policy);

        let Outcome::Commit { set, response } = outcome else {
            // A Noop or a Reject writes nothing, so there is no race to lose.
            return Applied {
                outcome,
                revision: Some(revision),
                conflicts,
            };
        };

        match store.commit(&cmd.tenant, &cmd.aor, revision, set.clone()) {
            Ok(committed) => {
                return Applied {
                    outcome: Outcome::Commit { set, response },
                    revision: Some(committed),
                    conflicts,
                };
            }
            Err(conflict) => {
                conflicts += 1;
                if conflicts > retries {
                    // S10 — retries exhausted. Originated by the registrar UAS, so it is not a
                    // branch response and proxy-behavior's R8 (503 → 500) does not apply to it.
                    return Applied {
                        outcome: Outcome::Reject(Rejection::Unavailable),
                        revision: Some(conflict.current),
                        conflicts,
                    };
                }
            }
        }
    }
}

/// The in-memory backend: the same trait, trivially serialized.
///
/// The backend the contract tests and the spec's vectors run against first. `RG-4` adds
/// PostgreSQL and must pass this identical suite — a backend that needs its own version of a
/// vector has broken the contract rather than implemented it.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Keyed by (tenant, canonical AoR) — the serialization domain, and nothing wider.
    rows: HashMap<(String, CanonicalAor), (BindingSet, Revision)>,
    /// A bounded change feed, not a log (`RG-13`).
    changes: std::collections::VecDeque<Change>,
}

/// The most recent changes retained by each store (`RG-13`).
///
/// The change feed exists for the consumer `AF-*` / `RG-5` will add — a shard replication or a hook
/// stream — and that consumer is not built yet. Until it is, the feed is a test instrument, and an
/// *unbounded* one is a memory leak on the registration hot path: every commit pushed and nothing
/// ever drained. Bounded rather than removed, because removing it would make the eventual consumer
/// reinvent the mechanism, and because a healthy cluster can afford a short window of recent history.
/// `Change::Dropped` marks where the window slid past entries a fast consumer had not yet seen.
pub const CHANGE_FEED_CAPACITY: usize = 1024;

impl InMemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A poisoned lock means a test panicked while holding it. Recovering the data beats taking
    /// the process down: this crate forbids panicking on anything a network can cause, and a
    /// second panic here would bury the first one's message.
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// How many address-of-record rows exist, including ones whose set is empty.
    ///
    /// Empty rows persist: revisions are monotonic per AoR and never reset (§6 K3), so the row is
    /// what remembers the counter.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.inner().rows.len()
    }
}

impl LocationStore for InMemoryStore {
    /// Infallible in practice, and typed fallible anyway: the contract is §6 K7's, not this
    /// backend's, and a trait whose shape depended on which store happened to be behind it would be
    /// exactly the drift §6.3 exists to prevent. Absence is `Ok`, because absence is what this store
    /// actually knows.
    fn read(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
    ) -> Result<(BindingSet, Revision), ReadFailure> {
        Ok(self
            .inner()
            .rows
            .get(&(tenant.to_owned(), aor.clone()))
            .cloned()
            .unwrap_or_else(|| (BindingSet::new(), Revision::INITIAL)))
    }

    fn commit(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: BindingSet,
    ) -> Result<Revision, CasConflict> {
        let mut inner = self.inner();
        let key = (tenant.to_owned(), aor.clone());
        let current = inner
            .rows
            .get(&key)
            .map_or(Revision::INITIAL, |(_, revision)| *revision);

        if current != expected {
            return Err(CasConflict { expected, current });
        }

        let next = current.next();
        inner.rows.insert(key, (set, next));
        inner.changes.push_back(Change {
            tenant: tenant.to_owned(),
            aor: aor.clone(),
            revision: next,
        });
        if inner.changes.len() > CHANGE_FEED_CAPACITY {
            let _ = inner.changes.pop_front();
        }
        Ok(next)
    }

    fn lookup(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        now: Timestamp,
    ) -> Result<Vec<Target>, ReadFailure> {
        let inner = self.inner();
        let Some((set, _)) = inner.rows.get(&(tenant.to_owned(), aor.clone())) else {
            // L5, L7 — an unknown AoR is the empty set, and it is an *answer*: this store looked.
            // L8's failure is the other thing, and this backend has no way to reach it.
            return Ok(Vec::new());
        };
        Ok(order_targets(set, now))
    }

    fn changes(&self) -> Vec<Change> {
        self.inner().changes.iter().cloned().collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::binding::Binding;

    fn aor(uri: &str) -> CanonicalAor {
        CanonicalAor::parse(uri.to_owned()).expect("a well-formed AoR")
    }

    fn binding(contact: &str) -> Binding {
        Binding {
            contact: bytes::Bytes::copy_from_slice(contact.as_bytes()),
            q: 1000,
            call_id: bytes::Bytes::from_static(b"rg13-call"),
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

    /// **`RG-13`'s failing-first test.** A registration storm must not make the change feed grow
    /// without bound. It grew linearly and forever before this story: every commit pushed, and
    /// nothing in the shipped path ever drained.
    #[test]
    fn rg13_the_change_feed_is_bounded() {
        let store = InMemoryStore::new();
        let aor = aor("sip:alice@example.test");

        // More refreshes than the feed holds.
        let total = CHANGE_FEED_CAPACITY * 4;
        for i in 0..total {
            let (set, revision) = store.read("t", &aor).expect("the in-memory store reads");
            let mut next = set.clone();
            next.insert(binding(&format!("sip:alice@10.0.0.{i}:5060")));
            store
                .commit("t", &aor, revision, next)
                .expect("each refresh commits");
        }

        let feed = store.changes();
        assert!(
            feed.len() <= CHANGE_FEED_CAPACITY,
            "the change feed must be bounded; it holds {} after {total} commits",
            feed.len()
        );
    }

    /// A bounded feed is still the *recent* history — a consumer reads the newest entries, not a
    /// hole. The oldest entries slide out past the capacity marker.
    #[test]
    fn rg13_a_bounded_feed_keeps_the_newest_changes() {
        let store = InMemoryStore::new();
        let aor = aor("sip:alice@example.test");
        for i in 0..(CHANGE_FEED_CAPACITY + 10) {
            let (set, revision) = store.read("t", &aor).expect("the in-memory store reads");
            let mut next = set.clone();
            next.insert(binding(&format!("sip:alice@10.0.0.{i}:5060")));
            store
                .commit("t", &aor, revision, next)
                .expect("each refresh commits");
        }
        let feed = store.changes();
        assert_eq!(feed.len(), CHANGE_FEED_CAPACITY);
        // The newest commit is present, and the oldest retained is at least as new as the tail of
        // what was written — never the oldest of all.
        let newest = feed.last().expect("non-empty");
        assert_eq!(
            newest.revision,
            Revision(1 + (CHANGE_FEED_CAPACITY + 10) as u64 - 1),
            "the newest change must be the most recent commit"
        );
    }
}
