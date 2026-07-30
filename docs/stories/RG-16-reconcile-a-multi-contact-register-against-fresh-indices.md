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

- [ ] Binding operations use a stable identity or a parsed view updated with every mutation; no
      operation applies an index computed against a different vector state.
- [ ] An insertion becomes visible to later operations in the same REGISTER, and a removal cannot
      shift a later match onto another binding or off the end of the vector.
- [ ] The compare-and-swap contract in [location-service](../specs/location-service.md) §6 still holds:
      a losing writer retries against fresh state rather than committing a stale set.
- [ ] **Failing-first vector:** from `{A, B}`, one REGISTER with `A;expires=0, B;expires=0` commits the
      empty set. On `86e6b10` it returns `Commit` with B still present because `get(1)` follows A's
      removal.
- [ ] Order-sensitive vectors also cover remove/refresh, refresh/remove, and two operations naming a
      contact first inserted by the same request; every response enumerates the actual final set.
- [ ] The new normative `LS-R-*` rows and their test names are registered together.
- [ ] Both the in-memory and PostgreSQL conformance suites pass unchanged, and `scripts/gate.sh` is
      green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-05**](../reviews/00-validated-synthesis.md#v-05--multi-contact-register-reconciliation-uses-stale-indices), reproduced by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- **Upstream boundary:** no; binding-set reconciliation is this platform's location-service state.
  URI equivalence remains the kernel primitive and must not be copied locally.
