---
id: RT-6
title: Specify declarative number normalisation
pillar: Signalling
status: in-progress
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, numbering]
note: 
---

# Specify declarative number normalisation

## Goal
Express number normalisation — stripping and prefixing on From, To, Request-URI and P-Asserted-Identity — as declarative, testable configuration instead of regex embedded in routing logic.

## Acceptance
- [x] Normalisation rules are data: which transformations, applied to which fields, in which order.
- [x] A digit-count guard with a fallback field is expressible (e.g. a Request-URI user outside a digit range falls back to the To user).
- [x] E.164 policy for egress — forcing a leading `+` — is a declared rule, not a code path.
- [x] Test vectors cover each rule and their composition.

## Progress
- The deliverable is [number-normalisation](../specs/number-normalisation.md), registered in
  `website/sidebars.js`, with the design record extended in
  [routing-trunks](../designs/routing-trunks.md) (*RT-6: normalisation is data, and its
  vocabulary is closed*).
  - Acceptance 1 → §3 (the `Profile` type), §4 (the closed field set, N1–N9), §5 (the four
    transforms, N10–N13), §7 (the three-phase evaluation and its termination bound, N21).
  - Acceptance 2 → §6 (N14–N20): one guard per field, condition `digits {min,max}`, fallback to
    another field's phase-1 value or `reject`. N17 is the one that matters — the fallback
    substitutes the *number*, never the URI, so the route plan's host survives.
  - Acceptance 3 → §9 (N28–N29): `ensure_prefix: "+"` plus `guard: { e164: global }`, defined in
    the same closed vocabulary rather than as a new keyword; ITU-T E.164 §6.2.1 is the whole
    content of the condition.
  - Acceptance 4 → §11: 45 vectors in six families — NN-X extraction and URI forms, NN-T one per
    transform, NN-G guard and fallback, NN-E the E.164 policy, NN-C whole profiles composed
    (including ingress ∘ egress, NN-C-3), NN-B binding and pipeline placement.
- **Zero Rust.** This is a spec story; the implementation lands with the trunk model (RT-2).
- **Deferred, deliberately:** the `NN-*` rows are not wired into the vector registry
  (`scripts/check-vectors.py`, `docs/reference/vector-scope.toml`) — that registry covers `PB`,
  `EP` and `RA` only, and CF-8 tracks the gap repo-wide for every spec written after PX-1. §11
  records the deferral in the spec itself so the table cannot quietly claim coverage it lacks.
- **Two kernel primitives are needed before the implementation can be lossless**, both flagged in
  §1 for CX-1 to file: (a) user-part surgery on `Uri` — `sipx-sip` exposes `user()`,
  `decoded_user()` and `push_param()` but no way to replace the user part, and (b) structured
  `tel:` access — `Scheme::Tel` is modelled but the body stays `Parts::Opaque` and the
  `telephone-subscriber` split is private to `tel_equivalent`. Neither blocks this spec; both
  block RT-2's implementation of it, and writing either here would be shadow-implementing kernel
  parsing (AGENTS.md #6). No upstream ledger row was added — `docs/upstream.md` is outside this
  story's fence.

## Notes
- One deployment strips a leading `+` and leading zeros from four fields, then falls back to the To user when the Request-URI user is not 3..20 digits.
- Today these are regexes inside route blocks, so they cannot be reviewed or tested independently.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-6**). The evidence sits in that deployment's own reference material.
