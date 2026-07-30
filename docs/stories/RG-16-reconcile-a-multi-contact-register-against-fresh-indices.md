---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: ready
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
- [ ] Binding operations are resolved against the vector as it stands when each operation is applied,
      not against indices computed once from the original vector.
- [ ] The compare-and-swap contract in [location-service](../specs/location-service.md) §6 still holds:
      a losing writer retries against fresh state rather than committing a stale set.
- [ ] **Failing-first test:** one `REGISTER` carrying several contacts — including at least one removal
      alongside an addition, so earlier operations shift the indices later ones use — commits exactly
      the set the request describes. This fails on `86e6b10`.
- [ ] The `LS-R-*` rows covering multi-contact reconciliation are proved rather than deferred, or a
      deferral names this story.

## Progress
- (not started)

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
