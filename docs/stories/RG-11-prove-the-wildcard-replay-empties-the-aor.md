---
id: RG-11
title: Prove that a replayed credential empties the AoR through the wildcard path
pillar: Signalling
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: RA-R-7 is deferred to this story — it is the only unproved row in the RA family
---

# Prove that a replayed credential empties the AoR through the wildcard path

## Goal
Give `RA-R-7` a test. `RG-10` established in
[registrar-auth](../specs/registrar-auth.md) §7.2 that a captured `Authorization`, reattached to a
REGISTER carrying `Contact: *`, `Expires: 0` and a fresh `Call-ID`, authenticates and removes
**every** binding on the AoR — but that pass was documentation-only, so the row ships asserted and
unproved while its additive sibling `RA-R-6` is proved twice.

## Acceptance
- [ ] A test drives the two halves together end to end: the replayed-credential admission path and
      [location-service](../specs/location-service.md) §W3's wildcard removal branch. Both halves
      have tests today — `wildcard()`'s ordering tests, and `RA-R-6`'s replayed-credential tests —
      and nothing drives them as one request.
- [ ] The assertion is on **what the store holds afterwards**, not only on the status code. The
      point of the row is that the AoR ends up empty; a `200` proves nothing here. `RA-R-6`'s sim
      test is the model — it pins the resulting contact set.
- [ ] `RA-R-7` moves from `deferred` to `proved` in
      [vector-scope.toml](../reference/vector-scope.toml) and
      [conformance.md](../reference/conformance.md), regenerated rather than hand-edited.
- [ ] The test is failing-first in the meaningful sense: it must fail if the ordering guard were
      ever extended to cover the wildcard path, so it pins today's behaviour rather than restating
      the implementation.

## Progress
- (not started)

## Notes
- Filed because `RG-10` registered `RA-R-7`'s deferral against this story before it existed, and
  said so rather than leaving the row unaccounted for. That is the right failure mode — a deferral
  naming a real story is the whole mechanism — but it leaves this ID owing a file, which this is.
- The behaviour is deliberate and accepted (§7.3), so this story proves an accepted exposure rather
  than fixing a defect. That is worth stating in the test's own name: someone reading it later
  should not mistake it for a regression test guarding a bug.
- Relevant: `crates/sipx-clstr-registrar/src/process.rs:56-76` — `wildcard()` rejects only when
  `binding.call_id == cmd.call_id && cmd.cseq <= binding.cseq`, which a fresh `Call-ID` makes false
  for every stored binding.
