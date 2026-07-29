---
id: RT-10
title: Close the transform-totality holes before RT-2 implements
pillar: Routing
status: ready
priority: 1
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks]
note: found reviewing RT-6 — N12's totality claim does not hold as written
---

# Close the transform-totality holes before RT-2 implements

## Goal
Make [number-normalisation](../specs/number-normalisation.md) §5 say what its transforms do to a
digit form carrying a leading `+`, so that N12's totality claim is true rather than nearly true.
This is the spec's central promise — a total, pure function — and an implementer hitting an
unspecified case will invent an answer, which is exactly what `RT-6` exists to prevent.

## Acceptance
- [ ] `T2`, `T3` and `T4` state their behaviour when the input carries a leading `+`. Today
      `add_prefix: "0"` on `+4930` yields `0+4930`, which N12 says cannot happen — it is neither a
      digit form nor the input unchanged. `add_prefix: "+"` on `+4930` yields `++4930`.
- [ ] `T2` states whether "leading zeros" is evaluated before or after a leading `+`, so
      `strip_leading_zeros` on `+0049301` has one defined answer.
- [ ] `Literal` (§3) admits the empty value, or §10's own `carrier-e164` and `trim` profiles stop
      using it. Today `Literal` is "1–8 bytes drawn from `+` and DIGIT" while both shipped profiles
      write `replace_prefix: { "+": "" }`, and `N22` implies only an empty *key* is a load error.
- [ ] Guard evaluation against an `Absent` or `NotANumber` **guarded** field has a rule, not just a
      vector. `NN-E-5` asserts an outcome that no rule in §6 produces; §5's `N10` handles the
      symmetric case for transforms explicitly, so the omission reads as an oversight.
- [ ] The `PAssertedIdentity`-absent branch is settled: under `N16` a guard on an absent PAI with a
      field fallback would have the normaliser **create** a `P-Asserted-Identity`, which §2 puts out
      of scope and gives to `RT-7`.
- [ ] A vector covers each newly-defined case, including at least one `+`-carrying input per
      transform.

## Progress
- (not started)

## Notes
- Every malformed value named above **serialises**: `+` is `user-unreserved`, so `0+4930` goes into
  a Request-URI and leaves the platform rather than failing loudly.
- Filed from the independent review of `RT-6`. Not a defect in that diff's structure — the spec is
  sound in shape and its 45 vectors were recomputed by hand and are correct — these are the cases
  its own rules do not reach. `RT-2` is the implementation story that would hit them first.
- Also from that review, smaller and separable: `N21`'s termination bound of
  `4 fields × (4 transforms + 1 guard)` understates itself, because `N4` lets
  `P-Asserted-Identity` carry two independently-transformed values — five field-instances, 25
  steps. Still finite and constant; the sentence carrying the claim is just wrong.
