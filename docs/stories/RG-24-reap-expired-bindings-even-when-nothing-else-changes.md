---
id: RG-24
title: Reap expired bindings even when the REGISTER changes nothing else
pillar: Registrar
status: ready
priority: 3
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: drop_expired runs on a clone that a Noop outcome discards, so an AoR that only ever queries grows without bound
---

# Reap expired bindings even when the REGISTER changes nothing else

## Goal
Make expiry reaping durable, so an address of record cannot accumulate dead bindings indefinitely
just because no REGISTER it receives ever changes anything.

## Acceptance
- [ ] A REGISTER whose outcome is `Noop` — a query-only request, or one whose every operation is
      absorbed by B4 — still persists the removal of bindings that have expired.
- [ ] The reap is not a second write path: it goes through the same compare-and-swap the mutation path
      uses, so a losing writer retries rather than clobbering (`location-service` §6).
- [ ] Reaping alone does not make a retransmission non-idempotent: the response is unchanged, and B9's
      `Commit`→`Noop` downgrade must not be defeated by a reap that changes nothing.
- [ ] **Failing-first test:** an AoR holding one expired and one live binding receives a query-only
      REGISTER; afterwards the store holds only the live binding. Today it holds both.
- [ ] A vector row in `location-service` §9, registered with its test name in the same commit.
- [ ] Both the in-memory and PostgreSQL conformance suites cover it — the shared suite in
      `crates/sipx-clstr-registrar/src/conformance.rs`, not the in-memory test file alone.

## Progress
- (not started)

## Notes
- Found by `RG-16` round 3 and left deliberately unfixed there, correctly — it is pre-existing at that
  story's merge base and outside its Goal.
- Mechanism: `process.rs:69` and `:133` both do `let mut set = current.clone(); set.drop_expired(cmd.now);`.
  When the outcome is `Noop` the clone is discarded, so the reap never reaches the store.
- **Scope it honestly: this is a storage leak, not a routing defect.** `order_targets` calls
  `set.active(now)` and `location-service` `L1` excludes expired bindings against the caller's `now`,
  so a dead binding is never a call target. The quota is likewise computed on the reconciled set after
  `drop_expired`, so an expired row does not block a new registration either.
- The harm is therefore unbounded per-AoR row growth in the durable store for any AoR whose traffic
  never mutates it, plus the scan cost every read pays over rows that can never be used. Low severity,
  real, and cheap to close.
- Considered for upstream: **no.** The location service and its expiry semantics are this platform's;
  `registrar-auth` §2 draws that line.
