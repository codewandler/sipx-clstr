---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: in-progress
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: round 3 — §5.5 settles the quota question against the code; the fix is process.rs, not an amendment
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

- **Unparked for round 3.** The blocker I recorded — "needs a §5.5 ruling" — was already answered by
  §5.5's own text and I had over-deferred it: "a REGISTER whose **committed outcome** would exceed it
  fails `403`" and "refreshes, replacements and removals never grow the set and never trip the quota".
  The committed outcome is authoritative, so a conservative pre-check may not refuse what the outcome
  permits. **No amendment; the fix is in `process.rs`.** Round 3 dispatched from
  `impl/RG-16-rework` with that ruling and both findings.

- **Parked 2026-07-30 after two rework rounds. Both branches are preserved and neither is merged:
  `impl/RG-16` (round 1) and `impl/RG-16-rework` (round 2, `16480fe`).** The original V-05 defect *is*
  fixed on the rework branch. It is unmerged because the fix currently trades that defect for a
  different reachable one, and merging would swap a silent wrong answer for a loud wrong refusal.

### What round 2 got right, and should be kept

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

### Why it is not merged — two findings, both measured at the branch tip

1. **The S8 quota pre-check refuses a REGISTER whose committed outcome fits the quota, at the default
   policy.** With nine bindings held and `TenantPolicy::default()` (max 10), one REGISTER carrying
   `x;line=1, x, x;line=2` is answered
   `Forbidden("the address-of-record already holds its maximum bindings")`, where the committed
   outcome is 10 active. `adding` counts a candidate unless it is equivalent to an already-**counted**
   one, which is an *upper* bound — and after B6/B7 the loop can collapse several operations onto one
   binding, so the pre-check must be a *lower* bound. §5.5 makes the quota a test on the **committed
   outcome** and says in terms that "refreshes, replacements and removals never grow the set and never
   trip the quota"; here two replacements trip it, and a `403` is something the UA cannot retry out of.
   The non-transitivity argument written above the check is inverted for that chain: the loop commits
   one binding, so comparing against *skipped* candidates would have been exact.
2. **A verbatim retransmission of `LS-R-29`'s own shape is answered `500 StaleSequence`** — so the very
   request B7 exists to legalise is not idempotent. `CC;3600, CC;7200` commits one binding at 7200 on
   first delivery and `500` on retransmission, because B4 is evaluated per operation against that
   operation's granted duration while §5.3's idempotency rule is stated per *binding* against the
   command's requested outcome — which under B6/B7 is 7200, exactly what is stored. Not a regression
   (the base answers `500` there too), but **B7's own new text claims this case is handled**, so the
   spec now asserts the opposite of the behaviour.

### What would settle it

- **A ruling on §5.5 first, because it decides where the fix goes.** If §5.5 means strictly "the
  committed outcome decides", finding 1 is a `process.rs` fix — make the pre-check a lower bound by
  comparing candidates against *skipped* ones. If a conservative upper-bound refusal is acceptable
  policy, then §5.5 needs amending plus a vector, and `process.rs` is already right. That call belongs
  to whoever owns §5.5, not to an implementor.
- Finding 2 needs a direction chosen: either make B4 compare net outcome per binding, or narrow B7's
  text to stop claiming the retransmission case is handled.
- Minor, worth folding in: a retransmission of `LS-R-28`'s shape commits and bumps the revision though
  the durable set is identical, where B4.2 says a retry leaves the revision as it is. Cost is a
  spurious revision bump and change event, which §6 K4/K5 make best-effort anyway.
- `docs/reference/conformance.md` on the branch was regenerated against a pre-merge `main` (131/579,
  60 sections) and `main` has since moved (129/576, 61 sections). Regenerate at the merge, do not
  resolve toward either side.

### Correction to the round-2 report

Its failing-first disclosure said `LS-R-26` and `LS-R-27` already passed at the merge base. Measured at
the real merge base `3b9bf4b`, only `LS-R-27` passes: `LS-R-24`, `25`, `26`, `28`, `29` and the CAS
test all fail, with `LS-R-26` failing exactly as Acceptance item 4 describes. The disclosure appears to
have been measured against round 1's tip rather than the merge base — an error in the conservative
direction, but the record should be right.

## Notes

- Validated synthesis finding [**V-05**](../reviews/00-validated-synthesis.md#v-05--multi-contact-register-reconciliation-uses-stale-indices), reproduced by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- **Upstream boundary:** no; binding-set reconciliation is this platform's location-service state.
  URI equivalence remains the kernel primitive and must not be copied locally.
