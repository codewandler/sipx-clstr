---
id: EX-1
title: Specify hook phases and the module manifest
pillar: Platform
status: done
priority: 5
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: must land before the proxy API hardens
---

# Specify hook phases and the module manifest

## Goal
Write `docs/specs/hook-framework.md`: the typed hook phases and the module manifest that make extensions declared modules instead of core edits.

## Acceptance
- [x] The ordered phase list is specified, each phase with its typed context and permitted effects.
- [x] The module manifest schema covers hooks used, dependencies, conflicts, methods/headers/option tags consumed and advertised, state needs, and timers.
- [x] Startup graph validation rules (ordering, conflict detection, capability advertisement for `Supported`/`Allow`) are specified, with at least one deliberately invalid module set as a vector.
- [x] Phase boundaries are reviewed against PX-1 so the hook spec and the proxy spec name the same pipeline.

## Progress
- 2026-07-28 — Wrote [hook-framework](../specs/hook-framework.md): 13 ordered phases (H1–H13,
  matching architecture chart 6 exactly), closed per-phase effect enums, the three-class state
  discipline resolving the design's invariant-5 risk (request-scoped scratch / token-carried
  facts / off-hot-path stores whose `StateKeyDomain` deliberately has no dialog or Call-ID
  variant — a dialog-keyed store is unrepresentable), the Rust-const manifest schema, startup
  graph validation G1–G6 (deterministic order with byte-wise lexicographic tiebreak; derived
  `Supported`/`Allow` as the V6 acceptance set), the EX-5 profile boundary, and vectors
  HF-1 … HF-8.
- 2026-07-28 — Phase-alignment review against proxy-behavior (PX-1); the mapping table is
  normative in hook-framework §3. Deltas recorded for the integrator (no proxy-spec edits made):
  1. proxy-behavior §4 V7 says "hook phase" (singular); the hook spec realizes V7 as the
     `BeforeAuth`/`AfterAuth` pair — V7's row could read "hook phases `BeforeAuth`/`AfterAuth`"
     the way F5 names `BeforeForward`.
  2. proxy-behavior §8 has no named hook rows; `ResponseReceived` attaches after R2 (never for
     an R3-absorbed 100) and `BeforeResponseForward` before the `Respond` effect — §8 could
     reference them the way F5 does.
  3. Dialog-forming events are absent from proxy-behavior; `DialogCreated`/`DialogTerminated`
     are defined in the hook spec as edge-local, best-effort observations (2xx to a
     dialog-forming request / 2xx to BYE forwarded) — a note at R5 would tie them in, but the
     proxy spec needs no behavioral change.
  4. Registrar-path phases (`BeforeRegistrarUpdate`/`AfterRegistrarUpdate`, bracketing the
     `LocationStore` CAS) fall outside proxy-behavior's scope; flagged for the RG-1
     location-service spec to use the same phase names (hook-framework §3 requires it).
  5. Token-fact budget: proxy-behavior F4's ≤ 200-byte token parameter is provisional until
     AF-1; hook-framework §5 introduces a module-fact sub-budget (placeholder 64 bytes) that
     AF-1 must allocate explicitly when it fixes the layout.

## Notes
- 2026-07-28 — integrator review passed; cross-references reconciled (see CHANGELOG).
- Design: [extension-framework](../designs/extension-framework.md).
