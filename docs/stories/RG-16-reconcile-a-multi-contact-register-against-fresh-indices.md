---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: blocked
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: blocked by RG-25 — the quota cannot be measured on the outcome alone until the contact count is bounded
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

- **Parked after round 3, blocked by [RG-25](RG-25-bound-the-contact-operations-one-register-may-carry.md).**
  Three branches preserved, none merged: `impl/RG-16` (r1), `impl/RG-16-rework` (r2),
  `impl/RG-16-r3` (r3, `7b68929` — the furthest along and the one to resume from).
- **The blocker is upstream of this story and the coordinator caused the last round of it.** Round 3
  was told the §5.5 ruling permitted either a lower-bound pre-check *or* no pre-check at all. It
  reasonably took the second option — and that **deleted `RG-14`'s work bound**, restoring a quadratic
  amplifier: measured 211.5 ms of one core for a single 64 KB datagram carrying ~3500 contacts, against
  1.15 ms with the pre-check. `RG-14`'s Acceptance item 4 had already settled this exact question — "a
  cheap pre-check that cannot disagree with it, not a relocation of the real check" — and the dispatch
  did not read it.
- **The triangle that has to be broken, stated so round 4 does not re-enter it.** §5.5 requires the
  quota to be measured on the *committed outcome*. `RG-14`'s pre-check was sound only because the most
  active bindings a request could reach was `current_active + genuine_additions`; **this story's B6/B7
  invalidated that premise** by letting several operations collapse onto one binding, so the pre-check
  began over-refusing what §5.5 permits. There is no sound *lower* bound to replace it with, and
  removing it restores the amplifier. The only exit is to bound the input — `RG-25`, which is
  `RG-14`'s never-landed item 2.
- **Second finding, independent of the above and still open.** B8's `net_grant` resolves the *future*
  against the view as it stands now, so an operation that will itself be hijacked by an intervening
  operation still counts as superseding. B8's own text says "the effect of the **last** operation of
  this request that resolves to it", and B6 fixes resolution "against the set as the preceding
  operations left it" — the code asks `view.find` now instead. Measured: stored `…;line=1` (Call-ID
  `other`) plus bare `…` (`i2`/1 at 3600); REGISTER `i2`/1 carrying `…;line=2;expires=7200`,
  `…;expires=3600`, `…;line=3;expires=3600` → base rejects `500` (B5/LS-R-22), tip commits
  `[…;line=3 @3600, … @3600]` with op1's requested 7200 neither applied nor aborted. Lower severity
  than r2's findings — nothing is written under the spent token, no deadline is extended, and §5.6's
  response tells the UA the truth — but it is this story's own defect class: a decision resolved
  against a vector state other than the one it is applied to.
- **What round 3 got right and round 4 must keep.** The quota genuinely cannot be exceeded — only two
  `Commit` sites exist and both are gated by the reconciled-set check; probes confirm 9+3 → `403`,
  0+11 → `403`, 9+2-with-one-taken-back → commits 10. **B9 is load-bearing and `LS-R-28` is not a
  relaxed proof**: neutering B9 fails `LS-R-28` and `LS-R-32` on both deliveries, and `LS-R-28` has
  never existed on `main`, so its expectation is new rather than weakened. The `reaped` guard means B9
  can only fire when the durable set is byte-identical, so it does not widen `RG-24`. Nothing normative
  was lost moving the counterfactuals out of the Expect cells. B8 and B9 are separately load-bearing.
  `Binding` derives `PartialEq` over every field, so B9's stated field risk is not live.
- **Also surfaced, worth keeping:** removing the pre-check fixed a *second* over-refusal class nothing
  pins — 9 held plus `x, y, y;expires=0` (committed outcome 10) is `403` at the base and commits at the
  tip. And §5.5's amended prose re-asserts "removals never trip the quota" while the single check still
  refuses one (12 held against a quota of 10, a REGISTER removing one → `403`, at base and tip alike).
  Pre-existing, and this diff strengthened the sentence the code disagrees with.

## Notes

- Validated synthesis finding [**V-05**](../reviews/00-validated-synthesis.md#v-05--multi-contact-register-reconciliation-uses-stale-indices), reproduced by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- **Upstream boundary:** no; binding-set reconciliation is this platform's location-service state.
  URI equivalence remains the kernel primitive and must not be copied locally.
