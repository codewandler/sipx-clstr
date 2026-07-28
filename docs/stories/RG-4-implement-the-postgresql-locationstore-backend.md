---
id: RG-4
title: Implement the PostgreSQL LocationStore backend
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [location]
note: M1 #10 · the same LS-* vectors on PostgreSQL, unchanged — and two races found
---

# Implement the PostgreSQL LocationStore backend

## Goal
Implement the first production `LocationStore`: serializable per-AoR transactions on PostgreSQL with LISTEN/NOTIFY as the change stream.

## Acceptance
- [x] The backend passes the identical contract-test suite as the in-memory store.
- [x] Missed change notifications are tolerated: the read path bounds staleness by TTL.
- [x] A registration-storm load model (mass re-REGISTER after an outage) is documented with measured headroom, and refresh writes are coalesced if the model demands it.

## Progress
**"Identical" is now literally true.** `sipx_clstr_registrar::conformance::run_location_store_suite`
takes a `&dyn LocationStore` and holds 15 groups of `LS-R`, `LS-K` and `LS-L` rows. Both backends call
that one function; a suite copied per backend drifts, and the copy that drifts is the one that stops
catching things. Every failure names the row *and* the backend, because "LS-R-6 failed on postgres" is
a bug report and "assertion failed" is not.

The suite ships behind the registrar's `test-suite` feature — test code in a library, deliberately:
the backend lives in the **driver** crate, because it is IO and that is where IO belongs, and it cannot
reach a `#[cfg(test)]` module in the registrar. That placement also keeps `tokio` a dependency of
`sipx-clstr-node` and of nothing else, which is the rule `CX-2` wrote down.

**Serializability is a revision predicate, not an isolation level.**
`UPDATE … WHERE tenant = $1 AND aor = $2 AND revision = $5` makes §6 K1 hold at any isolation level
that keeps a single statement atomic — which all of them do. The first write is its own
compare-and-swap: `INSERT … ON CONFLICT (tenant, aor) DO NOTHING`, then fall through to the predicated
update. A `SELECT` followed by an `INSERT` would let two nodes both believing an address-of-record is
new through, one silently overwriting the other's bindings — and a cold start is exactly when both
believe that.

**Verified against a real PostgreSQL 16**: the suite, a binding round trip (contact verbatim with its
parameters, `q`, `Path` in order, `reg_id`, `instance_id`, `flow_ref`, `principal`, both timestamps),
the first-write race, and the fencing of a spent revision.

### Two races, both found by running it, both flaky rather than broken

The dangerous kind: each passed on a re-run.

1. **`CREATE TABLE IF NOT EXISTS` is not atomic against a concurrent `CREATE`.** Four test threads on a
   fresh database produced `duplicate key value violates unique constraint
   "pg_type_typname_nsp_index"`. `IF NOT EXISTS` guards the *outcome*, not the race — and two nodes
   starting together is the ordinary case on a cold deployment and on every rollout. The DDL now runs
   under a session advisory lock.
2. **The tests shared one tenant.** Four of them ran concurrently, each truncating it, so one wiped
   another's rows mid-run and a commit came back `CasConflict { current: Revision(0) }` — the row had
   simply vanished. Each test now owns a tenant, which is also the contract's own boundary: §5's
   serialization domain is `(tenant, aor)`, so a shared tenant had the tests contradicting the
   independence they exist to assert.

Verified by **10 consecutive runs against a freshly dropped table**, all green. The first race hid
because the table existed by the second run; that is why the fix carries a comment saying so.

### Staleness

The read path is **read-through**: no cache, so observed staleness is zero and K5's TTL bound holds
trivially. A cache is an optimisation K5 permits, not one it requires, and adding one before there is a
measured reason to would be adding an invalidation bug for free. The test asserts what actually
matters — a consumer that sees *no* change events still reads the truth, because correctness never
depended on delivery (K4).

`LISTEN`/`NOTIFY` is therefore **not** implemented yet, and the change stream is in-process. Recorded
here rather than quietly skipped: it is a latency optimisation whose absence costs nothing at M1's
single-node scale, and it becomes worth building when a second reader exists to invalidate.

### The registration storm, measured

500 devices, one connection, PostgreSQL 16 in a container on this machine:

| Phase | Elapsed | Rate |
|---|---|---|
| First registration (insert path) | 698 ms | **716/s** |
| Every device refreshes (update path) | 756 ms | **661/s** |

**Flat, which is the finding.** One REGISTER costs one read and one write, and the estate's size does
not change that — a backend scanning the table per registration would show the refresh pass degrade
against the first. The test asserts the *ratio* rather than a rate, loosely enough not to encode this
machine's disk into the suite and tightly enough to catch an accidental O(n) read path.

**Refresh writes are not coalesced, and the model says they need not be yet.** At ~700/s on a single
connection a 10,000-device estate re-registers in about 14 seconds, and a pool multiplies that by its
width because different address-of-records never contend (§10.3's independence, enforced by the
predicate rather than by a lock). Coalescing trades away the guarantee that a `200` follows the commit
— which `RG-3` asserts, because a UA told it is reachable before the write landed has been told
something false. That trade needs a measured reason, and 14 seconds for ten thousand devices is not
one. `RT-3`'s overload work is where it gets revisited with real numbers.

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
- The binding set is one `jsonb` column, not a table of rows: §6 K2 replaces the whole set atomically
  on every commit, so decomposing it would need its own transaction to stay atomic and the revision
  predicate would stop being the only thing enforcing order.
- A read failure returns the empty set rather than widening the trait to `Result`. That is safe
  *because* of the CAS: the commit that follows is predicated on revision 0, so a row that really
  exists produces a conflict and a retry — never a lost update (§6 K6). A commit failure reports a
  conflict against the revision held, so the driver retries and §5.1 S10 answers `503` when it runs
  out, which is the truth.
- CI has a `postgres` job with a real database service. The local gate runs it only when
  `SIPX_CLSTR_TEST_DATABASE_URL` is set, and the tests **announce a skip** otherwise — a skip that
  looks like a pass is how a backend stops being tested without anyone deciding to stop testing it.
