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
    /// The latest committed set and its revision.
    ///
    /// Staleness here is a liveness concern only: a stale read shows up as a [`CasConflict`] and a
    /// retry, never as a lost update (§6 K6).
    fn read(&self, tenant: &str, aor: &CanonicalAor) -> (BindingSet, Revision);

    /// Install `set` iff the stored revision is still `expected`, atomically.
    fn commit(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: BindingSet,
    ) -> Result<Revision, CasConflict>;

    /// The forking-ordered target set for an AoR at `now` (§7).
    fn lookup(&self, tenant: &str, aor: &CanonicalAor, now: Timestamp) -> Vec<Target>;

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
    /// The revision the address-of-record ended at.
    pub revision: Revision,
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
pub fn apply<S: LocationStore + ?Sized>(
    store: &S,
    cmd: &RegisterCommand,
    policy: &TenantPolicy,
    retries: usize,
) -> Applied {
    let mut conflicts = 0;
    loop {
        let (current, revision) = store.read(&cmd.tenant, &cmd.aor);
        let outcome = process(cmd, &current, policy);

        let Outcome::Commit { set, response } = outcome else {
            // A Noop or a Reject writes nothing, so there is no race to lose.
            return Applied {
                outcome,
                revision,
                conflicts,
            };
        };

        match store.commit(&cmd.tenant, &cmd.aor, revision, set.clone()) {
            Ok(committed) => {
                return Applied {
                    outcome: Outcome::Commit { set, response },
                    revision: committed,
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
                        revision: conflict.current,
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
    changes: Vec<Change>,
}

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
    fn read(&self, tenant: &str, aor: &CanonicalAor) -> (BindingSet, Revision) {
        self.inner()
            .rows
            .get(&(tenant.to_owned(), aor.clone()))
            .cloned()
            .unwrap_or_else(|| (BindingSet::new(), Revision::INITIAL))
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
        inner.changes.push(Change {
            tenant: tenant.to_owned(),
            aor: aor.clone(),
            revision: next,
        });
        Ok(next)
    }

    fn lookup(&self, tenant: &str, aor: &CanonicalAor, now: Timestamp) -> Vec<Target> {
        let inner = self.inner();
        let Some((set, _)) = inner.rows.get(&(tenant.to_owned(), aor.clone())) else {
            return Vec::new(); // L5, L7 — an unknown AoR is the empty set, not an error.
        };
        order_targets(set, now)
    }

    fn changes(&self) -> Vec<Change> {
        self.inner().changes.clone()
    }
}
