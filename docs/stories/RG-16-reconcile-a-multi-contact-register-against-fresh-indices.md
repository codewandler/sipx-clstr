---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: in-progress
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: round 3 on impl/RG-16-r3 — both blocking findings fixed, §5.5 decided on the committed outcome and B8/B9 added; gate and the PostgreSQL suite green
---

# Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once

## Goal

Make `REGISTER` processing commit the binding set the request actually describes when the request
carries more than one contact.

## Acceptance

- [x] Binding operations use a stable identity or a parsed view updated with every mutation; no
      operation applies an index computed against a different vector state.
- [x] An insertion becomes visible to later operations in the same REGISTER, and a removal cannot
      shift a later match onto another binding or off the end of the vector.
- [x] The compare-and-swap contract in [location-service](../specs/location-service.md) §6 still holds:
      a losing writer retries against fresh state rather than committing a stale set.
- [x] **Failing-first vector:** from `{A, B}`, one REGISTER with `A;expires=0, B;expires=0` commits the
      empty set. On `86e6b10` it returns `Commit` with B still present because `get(1)` follows A's
      removal.
- [x] Order-sensitive vectors also cover remove/refresh, refresh/remove, and two operations naming a
      contact first inserted by the same request; every response enumerates the actual final set.
- [x] The new normative `LS-R-*` rows and their test names are registered together.
- [x] Both the in-memory and PostgreSQL conformance suites pass unchanged, and `scripts/gate.sh` is
      green.

## Progress

- **Round 3, on `impl/RG-16-r3` (branched from `impl/RG-16-rework`, `main` merged in with `--no-ff`).**
  The V-05 defect and both findings that held round 2 back are fixed; `scripts/gate.sh` is green and the
  PostgreSQL suite passes against a live database. Round 1 (`impl/RG-16`) and round 2
  (`impl/RG-16-rework`, `16480fe`) remain preserved and unmerged.

### What round 2 got right, and is kept unchanged

- `Reconciling` + `Slot` replace the snapshot-index scheme; the invariant `slots.len() == set.all().len()`
  is held by the three mutators, every `BindingSet` mutator is bounds-checked, and there is no panic
  path reachable from network input.
- **`location-service` §5.3.2 B7** decides the question round 1 tripped over: B2–B5 compare against the
  token of the request that *last wrote* the matched binding, so a binding an earlier operation of the
  same request wrote leaves them nothing to decide and the operation applies. Reviewed as correct —
  "last operation naming a contact wins" is the reading RFC 3261 §10.3 produces, `line` is ignored by
  §19.1.4 (it is not in the user/ttl/method/maddr/transport list), and the `400`-for-duplicates
  alternative has no §10.3 basis and would contradict item 5's "every response enumerates the actual
  final set".
- The six rows are in **`crates/sipx-clstr-registrar/src/conformance.rs`** — the shared suite both
  backends run — not only in the in-memory test file as round 1 had them. Verified load-bearing on
  both: deleting the `written_here` guard fails `LS-R-28` on PostgreSQL *and* in memory.
- `registered_at` is preserved across replacement; `written_here` cannot leak across a CAS retry
  (`Reconciling::new` sets it false for everything read from the store, and `apply` re-runs `process`
  on conflict).

### Round 3 — both findings fixed on `impl/RG-16-r3`

Round 2's substance is unchanged: `Reconciling`/`Slot`, B6/B7, `registered_at` preservation, the
recorded `written_here` flag and the six shared rows are all as they were. What round 3 changed is the
two things that blocked them, plus the minor revision item.

1. **§5.5 decided against the code, and the pre-check is gone rather than made a lower bound.** §5.5
   says the *committed outcome* decides, so the conservative pre-check is not a cheaper spelling of the
   rule but a stricter one, and it may not refuse what the outcome permits. There is now **one** quota
   check, on the reconciled set, where there were two that had to agree. That is deliberate: two checks
   have now diverged twice — `RG-14`'s first draft refused refreshes (`LS-R-15`), and round 2's upper
   bound refused `x;line=1, x, x;line=2` at nine held bindings where the outcome is ten (`LS-R-30`).
   A lower bound was considered and rejected as unreachable: comparing a candidate against *every*
   preceding one is exact for that chain, but still an upper bound once the request also removes
   something it added (`y, z, y;expires=0` at the boundary), so no cheap bound is sound. `LS-R-30`'s
   second half pins the other direction — `CC;line=1, CC;line=2` genuinely commits two bindings and is
   still `403` — so removing the pre-check did not remove the quota.
2. **B4 now compares the command's net outcome per binding — new §5.3.2 B8 — rather than narrowing
   B7's claim.** §5.3 already states idempotency per *binding* against "the command's requested
   outcome", so the per-operation reading was a misreading that only became visible once B6 let several
   operations reach one binding; B7's text was right and the code was wrong. `net_grant` finds the last
   operation that *resolves to* the binding (via `Reconciling::find`, not raw equivalence, because
   §19.1.4 non-transitivity makes those different sets) and B4 compares against its grant. Asked only
   after the cheap per-operation comparison fails and only when a later operation supersedes this one,
   so an ordinary single-contact retransmission pays nothing. B5 is not softened: what is compared is
   unchanged, and a command asking for something the store does not hold still aborts.
3. **Minor folded in as §5.3.2 B9:** a request whose reconciled set *is* the set it read commits
   nothing. `changed` records that the view was mutated, which is a different question — an addition a
   later removal takes back leaves the durable set untouched. This also amends `LS-R-28`, whose
   revision expectation moved from bumped to unmoved: the first delivery and the retransmission of that
   request are indistinguishable from the durable state (empty set both sides, no binding surviving to
   carry the token), so there is nothing a registrar could compare to answer them differently, and the
   only way the retransmission is idempotent is for neither delivery to write (`LS-R-32`).

New rows `LS-R-30`, `LS-R-31`, `LS-R-32` are in **both** the named tests
(`crates/sipx-clstr-registrar/tests/vectors_register.rs`) and the shared two-backend suite
(`crates/sipx-clstr-registrar/src/conformance.rs`). Verified load-bearing on PostgreSQL *and* in
memory by breaking each fix in turn: neutering B9 fails `LS-R-32` on both (`Revision(0) → Revision(1)`,
then `Revision(2)` on the retransmission — the exact minor defect); neutering B8 fails `LS-R-31` on both
with `got 500`; replacing the outcome check with a candidate count fails `LS-R-30` **and** `LS-R-15` on
both.

### Correction to the round-2 report

Its failing-first disclosure said `LS-R-26` and `LS-R-27` already passed at the merge base. That is
wrong. Measured at the merge base this round ran against, `f949336`, only `LS-R-27` passes:
`LS-R-24`, `25`, `26`, `28`, `29` and the CAS test
(`a_losing_writer_re_reconciles_a_multi_contact_register_against_fresh_state`) all fail, with `LS-R-26`
failing exactly as Acceptance item 4 describes. Round 2's disclosure appears to have been measured
against round 1's tip rather than the merge base — an error in the conservative direction, but the
record should be right. The three new rows fail there too: `LS-R-30` with `403`, `LS-R-31` and
`LS-R-32` on their first assertions, since the base has neither B6/B7 nor B8/B9.

`docs/reference/conformance.md` was regenerated after the merge (138/585). The coordinator is expected
to redo it on `main`.

## Notes

- Validated synthesis finding [**V-05**](../reviews/00-validated-synthesis.md#v-05--multi-contact-register-reconciliation-uses-stale-indices), reproduced by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- **Upstream boundary:** no; binding-set reconciliation is this platform's location-service state.
  URI equivalence remains the kernel primitive and must not be copied locally.
