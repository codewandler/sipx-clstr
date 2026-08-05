---
id: DP-17
title: Apply the membership, key and shard-map sections a node now loads
pillar: Cluster
status: backlog
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, cluster]
note: DP-16 landed the loading and validation half; these sections still parse into fields nothing reads, and MB5 made that third state universal rather than opt-in
---

# Apply the membership, key and shard-map sections a node now loads

## Goal

Finish what `DP-16` started. That story made `membership[]`, `keys[]` and `shardMap` load,
validate and report — but nothing consumes them, so they parse into public struct fields no
runtime path reads. The epic's rule is that **accepted means applied, or refused — there is no
third state**, and [fail-closed-config](../designs/fail-closed-config.md) §66 names exactly this
shape as what the epic exists to remove. `DP-16` also made `rpc` mandatory on call-path members
(MB5), so the third state is now on **every** deployment rather than only those that opted into a
key or a shard map.

## Acceptance

- [ ] `membership[].rpc` and the incarnation fields reach the runtime that needs them — the
      connection-owner RPC endpoint and the incarnation a flow reference carries — or the node
      refuses to start on a document declaring them. No field is accepted and ignored.
- [ ] `keys[]` reaches the affinity-token key set: the mint key mints, verify-only keys verify,
      and a document whose key material cannot be resolved refuses to start rather than running
      with no key (the same argument `FC-3` applied to `tenant[].auth`). This is `AF-8`'s missing
      `keys[]` loader, so the two stories settle their seam explicitly rather than both assuming it.
- [ ] `shardMap` reaches whatever owns shard placement, or is refused; `RG-5` owns the handoff
      runtime, so this story's job is the seam and the refusal, not drain-then-switch.
- [ ] `KY2` and `KY8` — the two start-up rules over a **resolved** secret that `DP-16` could not
      implement because nothing resolved anything — hold, and `cluster-membership` §11's note
      recording them as unclaimed is removed in the same change.
- [ ] The baseline moves back: a fully applied document reports **no** unapplied paths, and
      `fc2_a_fully_applied_document_reports_only_what_no_consumer_reads` returns to asserting
      emptiness. That test's name is the ratchet — while it reads "only what no consumer reads",
      the epic's invariant is not yet true.
- [ ] Failing-first: a document declaring a `keys[]` entry proves the mint key is used, on a test
      that fails before this story.

## Progress

- Filed at `DP-16`'s review, which asked the done-vs-partial question directly: every `DP-16`
  Acceptance item is worded *accepts*/*validated*/*proved* and is met, while only its Goal says
  *apply*. Splitting rather than stretching `DP-16` keeps that honest and gives the runtime work
  a place to be planned.

## Notes

- `driver.rs` was reserved for `RG-17` during `DP-16`'s wave, which is why the apply half was
  out of scope then. `RG-17` has since merged, so that constraint is gone.
- Coordinate with `AF-8` (a tokenless mid-dialog `Route` must not be a silent downgrade), which
  is blocked on exactly the `keys[]` loader this story lands.
