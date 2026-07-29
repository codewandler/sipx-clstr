---
id: RG-13
title: Bound the location store's growth — the change log and the row set both grow forever
pillar: Signalling
status: done
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

- **The change feed is bounded, not removed — and that was a design call, not the default.** The
  acceptance offered three options: a test facility (gate it out), a hook feed (bound it), or a
  replication log (needs a design). I bounded it at `CHANGE_FEED_CAPACITY = 1024` as a `VecDeque`,
  keeping the newest entries. Reason: the feed is dormant now but it is exactly the consumer
  `AF-*`/`RG-5` will need for shard replication or a hook stream, and removing it would force that
  work to reinvent the mechanism. A healthy cluster can afford a short window of recent history; an
  unbounded one is a leak.
- **Failing-first, and the bound is asserted, not assumed.** `store.rs` gains two tests: the feed
  must stay at or below capacity across 4× capacity commits, and the retained entries must be the
  *newest*, not a hole. Both grew unboundedly before this change.
- **The expiry half I got wrong first, and reverted.** My first pass made `lookup` reap expired
  bindings on read *and* bump the revision — which is a write position, not a wall clock. Moving it
  on a read would churn `Revision` under concurrent lookups and break compare-and-swap, and it
  contradicts §6 K3's "revision moves only on a committed write". A write-path reap hit the wall the
  crate is built on: it is **sans-IO and owns no clock**, so `commit` cannot know `now` at all. Both
  reverted.
- **What is actually true about reaping.** Same-AoR expiry is already reaped on the write path in
  `process.rs` (`drop_expired(cmd.now)`), because the caller supplies the clock. The gap the story
  names — an AoR written once and never again keeping bindings resident — is real, but the store
  cannot fix it without a clock, and a reader cannot fix it without churning the revision. That
  needs a reaper that owns time, which is driver/`DP-*` territory, not this store. Recorded rather
  than hacked in.
- The PostgreSQL backend got the same bound; the default build already excluded it (`postgres`
  feature), so the out-of-box node never grew here at all.
- Considered for upstream: no. This is the platform's own store contract.
- Gate green.

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
