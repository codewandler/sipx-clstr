---
id: RG-13
title: Bound the location store's growth — the change log and the row set both grow forever
pillar: Signalling
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: changes is a Vec nothing in the shipped path ever drains, in both backends
---

# Bound the location store's growth — the change log and the row set both grow forever

## Goal

Stop the location store growing without bound on the registration hot path. Every commit pushes to a
`changes: Vec<Change>` that nothing in the shipped path ever drains, and the row set is monotonic by
design with no cap, so a long-running registrar's memory is a function of total REGISTER traffic
rather than of live registrations.

## Acceptance

- [ ] The change log is bounded, drained, or gated out of the shipped build. `InMemoryStore` pushes a
      `Change` on every commit and `changes()` is called only from `conformance.rs`, a vector test and
      a sim test — plus a pass-through in `blocking_store.rs`. `PostgresStore` has the identical
      structure. Decide whether it is a test facility (then it should not be compiled into the
      production store), a hook feed (then it needs a consumer and a bound), or a replication log
      (then it needs a design, and `AF-*` probably owns it).
- [ ] **Failing-first**: a test drives N registration refreshes for one address-of-record and asserts
      the store's retained-change count does not grow with N. It fails today — it grows linearly and
      forever.
- [ ] The row set has a bound or a documented reason it does not need one. `store.rs` keeps empty rows
      deliberately — revisions must stay monotonic per [location-service](../specs/location-service.md)
      §6 K3 — so `rows` grows with the number of distinct addresses-of-record *ever seen*, not the
      number currently registered. That is correct for the compare-and-swap contract and unbounded for
      an open registrar.
- [ ] Expired bindings are reaped without needing a write to their own address-of-record. Today
      `drop_expired` runs only when an AoR is next written, and `lookup` filters at read time without
      writing back, so an AoR registered once and never touched again keeps its bindings resident
      forever.
- [ ] Whatever bound is chosen is expressed in the spec, not only in code — a cap that exists only as
      a constant is a policy nobody can find.
- [ ] `cargo test -p sipx-clstr-registrar` green, and the Postgres suite green when
      `SIPX_CLSTR_TEST_DATABASE_URL` is set.

## Progress

- (running log)

## Notes

- **Rough scale.** Each `Change` holds a `String` tenant plus a `CanonicalAor` (≤512 B). Every REGISTER
  refresh commits, so ten thousand phones on a sixty-second refresh is roughly 167 entries per second —
  about fourteen million entries a day, in both backends, released never.
- **Why this is reachable by anyone today.** The shipped binary cannot enable authentication (`FC-3`)
  and does not enforce `tenant[].domains` (`FC-4`), so any peer that can reach the port can mint rows
  and change-log entries for arbitrary addresses-of-record. `MAX_KEY_BYTES` bounds each key; nothing
  bounds the count. The two fail-closed stories reduce the exposure; they do not make the structures
  bounded, which is why this is its own story.
- **What is already right and should not be "fixed".** Keeping empty rows is a deliberate consequence
  of §6 K3's monotonic revisions, and the comment says so. The answer is a reaper or a cap that
  respects K3, not deleting rows.
- Considered for upstream: no. The kernel has no location service.
- Related: `RG-14` is the other unbounded thing on this path — the per-request work, rather than the
  retained state.
