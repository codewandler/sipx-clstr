---
id: RT-10
title: Close the transform-totality holes before RT-2 implements
pillar: Routing
status: done
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
- [x] `T2`, `T3` and `T4` state their behaviour when the input carries a leading `+`. → §5 now
      defines two projections shared by a digit form and a `Literal`: `plus(x)` is true iff `x`
      begins with `+`, and `digits(x)` is `x` with that `+` removed. `T3`'s result carries a leading
      `+` iff either operand does, and its digit run is `digits(literal)` then `digits(input)` — so
      `add_prefix: "0"` on `+4930` is `+04930`, not `0+4930`, and `add_prefix: "+"` on `+4930` is
      `+4930`, not `++4930`. `NN-T-10` and `NN-T-11` pin exactly those two.
- [x] `T2` states whether "leading zeros" is evaluated before or after a leading `+`. → The `+` is
      skipped first and is never counted, deleted or moved; zeros are counted in the digit run only.
      `strip_leading_zeros{max:8}` on `+0049301` is `+49301` (`NN-T-9`).
- [x] `Literal` (§3) admits the empty value. → Now "0–8 bytes drawn from `+` and DIGIT, with `+`
      permitted only in first position", with the empty literal legal **only** as a `replace_prefix`
      value (a strip, which is what §9's `carrier-e164` and §10's `trim` already write) or as the
      whole of `add_prefix`/`ensure_prefix`, where it is a declared no-op. An empty `replace_prefix`
      key stays a load error (`N22`), so "matches everything" still has to be written as
      `add_prefix`, on its own line, where a reviewer sees it.
- [x] Guard evaluation against an `Absent` or `NotANumber` **guarded** field has a rule. → `N31`:
      a guard's condition is defined only over a digit form, so on `Absent`/`NotANumber` it is
      defined to **never hold** — the guard's counterpart to `N10`, which makes the same choice for
      transforms. `NN-E-5` now has a rule behind it, and `NN-G-9` pins the substitution case.
- [x] The `PAssertedIdentity`-absent branch is settled. → `N32`: substitution rewrites the number
      inside an **existing** URI and never creates a field. With no PAI there is no URI to rewrite,
      so the field is left as extracted and traced `Skipped { reason: GuardedFieldAbsent }` — and
      that holds even for `fallback: reject`, because `N18` rejects when a *fallback* cannot supply
      a value, not when the guarded field has nothing to write into. Whether PAI is asserted at all
      stays `RT-7`'s. `NN-G-10` and `NN-G-11` pin both halves.
- [x] A vector covers each newly-defined case, including at least one `+`-carrying input per
      transform. → `NN-T-9` (T2), `NN-T-10` and `NN-T-11` (T3), `NN-T-12` (T4), plus `NN-G-9`,
      `NN-G-10`, `NN-G-11` for the guard rules.

## Progress
- **Closed 2026-07-29.** `N30` now states the totality claim in the form that is actually true:
  every output of every transform is `["+"] 1*DIGIT` or the input unchanged, *regardless of whether
  the input carried a `+`* — and `T1` needs no special case, because §3 permits `+` only in first
  position, so ordinary string-prefix matching already handles it.
- **The termination bound was wrong and is corrected.** `N21` said
  `4 fields × (4 transforms + 1 guard)`. `N4` lets `P-Asserted-Identity` carry two values, each
  transformed and guarded independently, so it is **five** field-instances — 25 steps, not 20. Still
  finite and constant; the sentence carrying the claim was simply miscounted.
- **Two citations were repaired rather than trusted.** RFC 3261 §17.1.1.3 requires the ACK's
  Request-URI, `From` and `Call-ID` to match the original but takes its `To` from the **response
  being acknowledged** — which normally differs by the added tag — so `To` was wrong in that list.
  The same misstatement was copied into
  [routing-trunks](../designs/routing-trunks.md) and is fixed there too. And the E.164 citation now
  points at §6.1 (an international number is at most 15 digits) *and* §6.3 (country codes are drawn
  from zones 1–9, so none begins with `0`) — two clauses, where a single §6.2.1 reference had been
  carrying both claims and covering only geographic numbers.
- **Finished by the coordinator, not by one agent.** The implementing agent's process was killed
  before it added the vectors its own new rules cite, leaving `N31` and `N32` referring to
  `NN-G-9`, `NN-G-10` and `NN-G-11`, which did not exist. `check-docs.sh` does not validate
  internal vector references, so that would have shipped green. The rows were added and every
  `NN-*` reference in the file was then checked to resolve to a real row.

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
