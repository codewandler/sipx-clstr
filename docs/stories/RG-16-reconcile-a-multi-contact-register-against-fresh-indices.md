---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: in-progress
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: release blocker — a REGISTER carrying several contacts can commit the wrong binding set
---

# Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once

## Goal
Make `REGISTER` processing commit the binding set the request actually describes when the request
carries more than one contact.

## Acceptance
- [x] Binding operations are resolved against the vector as it stands when each operation is applied,
      not against indices computed once from the original vector.
- [x] The compare-and-swap contract in [location-service](../specs/location-service.md) §6 still holds:
      a losing writer retries against fresh state rather than committing a stale set.
- [x] **Failing-first test:** one `REGISTER` carrying several contacts — including at least one removal
      alongside an addition, so earlier operations shift the indices later ones use — commits exactly
      the set the request describes. This fails on `86e6b10`.
- [x] The `LS-R-*` rows covering multi-contact reconciliation are proved rather than deferred, or a
      deferral names this story.

## Progress
- **Done.** The mechanism was confirmed before it was fixed: `process::explicit` parsed the stored
  contacts into a `Vec<Option<Uri>>` once, matched every operation by `position` in *that* vector, and
  then used the position as an index into a `BindingSet` it was concurrently mutating. The two
  diverged at the first removal.
- Three failure modes, all reproduced at `2cb22dd` and now pinned:
  - a binding survives its own removal (the index resolves past the end of the shortened set);
  - a refresh lands on whichever binding slid into the index, overwriting a contact the request never
    named and duplicating one it did;
  - a removal removes the *wrong* binding — the CAS-retry case, where the re-read set is longer.
- Spec first: [location-service](../specs/location-service.md) §5.3.2 **B6** fixes the resolution
  point — each operation is matched against the set as the preceding operations left it — and states
  why atomicity (§6 K2) and the CAS loop are unaffected. Vectors `LS-R-24` and `LS-R-25` under it,
  both **proved** in `docs/reference/conformance.md`, neither deferred.
- The fix is structural rather than a re-parse: a `Reconciling` view in `process.rs` owns the set and
  its parsed contacts and is the only thing that mutates either, so they cannot drift.
  `BindingSet::insert_at` (additive — `insert` delegates to it) reports where an insert landed, which
  is what an aligned view needs, since creation order means an insert is not an append.
- `RG-14`'s parse budget is unchanged and still asserted: one parse per stored binding, none per
  operation.

## Notes
- Found by the independent adversarial review of `86e6b10` (`v0.12.0`), finding **V-05**, reproduced
  by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- Considered for upstream: **no.** The location service and its binding reconciliation are this
  platform's ([registrar-auth](../specs/registrar-auth.md) §2 draws the line); the kernel owns digest
  primitives, not binding state.
