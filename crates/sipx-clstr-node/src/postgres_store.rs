//! The `PostgreSQL` `LocationStore` — `RG-4`.
//!
//! [location-service §6.2](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md):
//! one row per `(tenant, aor)` holding the revision and the binding set, with per-address-of-record
//! serializability enforced by a **revision predicate** rather than by an isolation level:
//!
//! ```sql
//! UPDATE … SET bindings = $4, revision = revision + 1
//!  WHERE tenant = $1 AND aor = $2 AND revision = $3
//! ```
//!
//! That is what makes §6's K1 hold at any isolation level that keeps a single statement atomic —
//! which every one of them does. Fencing rather than serializing is the whole point: two nodes racing
//! on one address-of-record are ordered by the counter, and two nodes on *different* ones never
//! contend at all, which is what §10.3's independence requirement asks for.
//!
//! It lives in this crate rather than the registrar because it is **IO**, and this crate is the driver
//! layer. That also keeps the registrar's default build free of a database driver.

use std::sync::Mutex;

use postgres::{Client, NoTls};
use sipx_clstr_registrar::{
    BindingSet, CanonicalAor, CasConflict, Change, LocationStore, Revision, Target, Timestamp,
    order_targets,
};

/// A lock id for schema application, so concurrent nodes serialize instead of colliding.
///
/// Any constant works, as long as every node uses the same one. Written as a positive `i64` literal
/// because that is the type `pg_advisory_lock` takes, and a `u64 as i64` cast would be a silent wrap
/// waiting to be copied into somewhere it matters.
const SCHEMA_LOCK: i64 = 0x_1f3a_5c7e_9b2d_4068_i64;

/// The schema. Idempotent, so a node can apply it on start without a migration runner.
///
/// `bindings` is `jsonb` rather than a table of rows: the whole set is replaced atomically on every
/// commit (§6 K2), so there is nothing to gain from decomposing it and a great deal to lose — a
/// multi-row update would need its own transaction to stay atomic, and the revision predicate would
/// no longer be the only thing enforcing order.
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS location_bindings (
    tenant   text   NOT NULL,
    aor      text   NOT NULL,
    revision bigint NOT NULL,
    bindings jsonb  NOT NULL,
    PRIMARY KEY (tenant, aor)
);
";

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The database refused or could not be reached.
    #[error("postgres: {0}")]
    Database(#[from] postgres::Error),
    /// A stored binding set could not be decoded.
    ///
    /// Distinct from a database error because it means *this* node and whatever wrote the row
    /// disagree about the schema — a deployment problem, not a connectivity one.
    #[error("a stored binding set could not be decoded: {0}")]
    Decode(#[from] serde_json::Error),
}

/// A `LocationStore` backed by `PostgreSQL`.
///
/// The client is behind a mutex because [`LocationStore`] is a synchronous `&self` trait and
/// `postgres::Client` needs `&mut`. A real deployment puts a pool here; one connection is enough to
/// prove the contract, and `RG-4`'s job is the contract.
pub struct PostgresStore {
    client: Mutex<Client>,
    /// Committed changes, for the §6 K4 stream.
    ///
    /// In-process for now. `LISTEN`/`NOTIFY` is the real mechanism (§6.2) and is best-effort by
    /// contract, so nothing about correctness waits on it — K5's staleness bound is the guarantee.
    changes: Mutex<Vec<Change>>,
}

// `postgres::Client` is not `Debug`, and the workspace requires public types to be. Written out
// rather than skipped: what a reader wants here is which table and how many changes are buffered, not
// the connection's internals — and a connection string must never print itself.
impl std::fmt::Debug for PostgresStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let buffered = self
            .changes
            .lock()
            .map(|changes| changes.len())
            .unwrap_or_default();
        f.debug_struct("PostgresStore")
            .field("table", &"location_bindings")
            .field("buffered_changes", &buffered)
            .finish_non_exhaustive()
    }
}

impl PostgresStore {
    /// Connect, and apply the schema.
    ///
    /// **`CREATE TABLE IF NOT EXISTS` is not atomic against a concurrent `CREATE`.** Two nodes
    /// starting together — the ordinary case on a cold deployment, and on a rollout — both find the
    /// table absent, both create it, and one gets
    /// `duplicate key value violates unique constraint "pg_type_typname_nsp_index"`. The `IF NOT
    /// EXISTS` guards the *outcome*, not the race.
    ///
    /// So the DDL runs under a session advisory lock. It is not a distributed-locking scheme and does
    /// not need to be: it serializes schema application among whoever is connected, which is exactly
    /// the window in which the collision happens.
    ///
    /// Found by four test threads racing on a fresh database — and it passed on the *second* run,
    /// because by then the table existed. A flake that hides on retry is the failure mode this comment
    /// exists to stop someone re-introducing.
    pub fn connect(url: &str) -> Result<Self, StoreError> {
        let mut client = Client::connect(url, NoTls)?;
        client.execute("SELECT pg_advisory_lock($1)", &[&SCHEMA_LOCK])?;
        let applied = client.batch_execute(SCHEMA);
        // Released whether or not the DDL succeeded: a held lock would wedge every other node.
        let unlocked = client.execute("SELECT pg_advisory_unlock($1)", &[&SCHEMA_LOCK]);
        applied?;
        unlocked?;
        Ok(Self {
            client: Mutex::new(client),
            changes: Mutex::new(Vec::new()),
        })
    }

    /// Remove every row of a tenant — for tests that need a known starting point.
    pub fn truncate(&self, tenant: &str) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "DELETE FROM location_bindings WHERE tenant = $1",
                &[&tenant],
            )?;
            Ok(())
        })
    }

    fn with_client<T>(
        &self,
        f: impl FnOnce(&mut Client) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut guard = self
            .client
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    fn read_inner(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
    ) -> Result<(BindingSet, Revision), StoreError> {
        self.with_client(|client| {
            let rows = client.query(
                "SELECT revision, bindings FROM location_bindings WHERE tenant = $1 AND aor = $2",
                &[&tenant, &aor.as_str()],
            )?;
            let Some(row) = rows.first() else {
                return Ok((BindingSet::new(), Revision::INITIAL));
            };
            let revision: i64 = row.get(0);
            let bindings: serde_json::Value = row.get(1);
            let set: BindingSet = serde_json::from_value(bindings)?;
            Ok((set, Revision(u64::try_from(revision).unwrap_or(0))))
        })
    }

    fn commit_inner(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: &BindingSet,
    ) -> Result<Result<Revision, CasConflict>, StoreError> {
        let payload = serde_json::to_value(set)?;
        let next = expected.next();
        let next_i64 = i64::try_from(next.0).unwrap_or(i64::MAX);
        let expected_i64 = i64::try_from(expected.0).unwrap_or(i64::MAX);

        let affected = self.with_client(|client| {
            if expected == Revision::INITIAL {
                // The row may not exist yet. `ON CONFLICT DO NOTHING` makes the insert *itself* the
                // compare-and-swap: two nodes both believing the address-of-record is new race here,
                // and exactly one row appears. A `SELECT` then `INSERT` would let both through.
                let affected = client.execute(
                    "INSERT INTO location_bindings (tenant, aor, revision, bindings)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (tenant, aor) DO NOTHING",
                    &[&tenant, &aor.as_str(), &next_i64, &payload],
                )?;
                if affected > 0 {
                    return Ok(affected);
                }
                // The row existed after all, so fall through to the predicated update — which will
                // find revision ≠ 0 and refuse, exactly as a conflict should.
            }
            let affected = client.execute(
                "UPDATE location_bindings
                    SET bindings = $4, revision = $3
                  WHERE tenant = $1 AND aor = $2 AND revision = $5",
                &[&tenant, &aor.as_str(), &next_i64, &payload, &expected_i64],
            )?;
            Ok(affected)
        })?;

        if affected == 0 {
            let (_, current) = self.read_inner(tenant, aor)?;
            return Ok(Err(CasConflict { expected, current }));
        }

        let mut changes = self
            .changes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        changes.push(Change {
            tenant: tenant.to_owned(),
            aor: aor.clone(),
            revision: next,
        });
        Ok(Ok(next))
    }
}

impl LocationStore for PostgresStore {
    fn read(&self, tenant: &str, aor: &CanonicalAor) -> (BindingSet, Revision) {
        // A read failure is a *liveness* problem, not a correctness one (§6 K6): returning the empty
        // set makes `process` decide as though the address-of-record were new, and the commit that
        // follows is predicated on revision 0 — so a row that actually exists produces a
        // `CasConflict` and a retry, never a lost update. The alternative would be widening the
        // trait to `Result` for a case the CAS already fences.
        match self.read_inner(tenant, aor) {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%tenant, aor = %aor, %error, "location read failed; treating as absent");
                (BindingSet::new(), Revision::INITIAL)
            }
        }
    }

    fn commit(
        &self,
        tenant: &str,
        aor: &CanonicalAor,
        expected: Revision,
        set: BindingSet,
    ) -> Result<Revision, CasConflict> {
        match self.commit_inner(tenant, aor, expected, &set) {
            Ok(outcome) => outcome,
            Err(error) => {
                // A failed write must never look like a successful one. Reporting a conflict against
                // the revision we hold sends the driver round its retry loop, and when the retries
                // run out §5.1 S10 answers `503` — which is the truth.
                tracing::error!(%tenant, aor = %aor, %error, "location commit failed");
                Err(CasConflict {
                    expected,
                    current: expected,
                })
            }
        }
    }

    fn lookup(&self, tenant: &str, aor: &CanonicalAor, now: Timestamp) -> Vec<Target> {
        let (set, _) = self.read(tenant, aor);
        // The same ordering function the in-memory backend uses. A backend that ordered targets its
        // own way would make the fork order depend on which store a node happens to be talking to,
        // and §7 L3 exists to stop exactly that.
        order_targets(&set, now)
    }

    fn changes(&self) -> Vec<Change> {
        self.changes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
