---
id: CF-8
title: Bring every spec's vector table under the vector gate
pillar: Platform
status: backlog
priority:
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [conformance, specs]
note: found at ME-1/AF-2 integration — the gate proves 3 of 7 specs and is silent about the rest
---

# Bring every spec's vector table under the vector gate

## Goal
Make `scripts/check-vectors.py` account for **every** spec that carries a vector table, so a row
nobody executes fails the gate wherever it lives. Today it knows three specs out of seven, which
means most of the platform's normative vectors are prose the gate has never read.

## Acceptance
- [ ] `LS` (location-service) and `MR` (media-relay) are registered in `SPECS`, `ROW`, `TEST_NAME`
      and `COVERS`, with their families named in `FAMILIES`. Both already use the three-part
      `XX-Y-n` row shape, so they need registration only — no renumbering.
- [ ] The two-part row shape is resolved for `affinity-token` (`AT-n`, `FR-n`) and `hook-framework`
      (`HF-n`). Decide **and record** which way it goes: widen the row grammar to accept two-part
      IDs, or renumber those tables into families. Renumbering churns every existing citation, so
      the reason for the choice matters more than the choice.
- [ ] Every newly-registered row is either covered by a test or carries a `vector-scope.toml`
      deferral with a reason and a story — the existing three-way discipline, including that a
      deferred row which *is* covered stays an error.
- [ ] `docs/reference/conformance.md` regenerates to include the new families, and
      `scripts/check-vectors.py --check` is green in `scripts/gate.sh`.
- [ ] The gate's summary line reports the true denominator. It currently reads
      `75/83 rows proved` while silently excluding four specs; after this story that number covers
      everything or the excluded set is explicit.

## Progress
- **Filed 2026-07-29, at the integration of `ME-1` and `AF-2`.** Both stories independently hit the
  same wall and both recorded it as adjacent-not-fixed: the registry files were fenced to another
  story during the implementation wave, so neither could wire itself in even where it wanted to.
- Measured at filing time — `SPECS` in `scripts/check-vectors.py:39-43` holds `PB`, `EP`, `RA`. The
  row families actually present in `docs/specs/`:

  | Spec | Row family | In the gate? | Shape |
  |---|---|---|---|
  | `proxy-behavior.md` | `PB` | yes | three-part |
  | `e2e-probe.md` | `EP` | yes | three-part |
  | `registrar-auth.md` | `RA` | yes | three-part |
  | `location-service.md` | `LS` | **no** | three-part — registration only |
  | `media-relay.md` | `MR` | **no** | three-part — registration only |
  | `affinity-token.md` | `AT`, `FR` | **no** | two-part — needs a grammar decision |
  | `hook-framework.md` | `HF` | **no** | two-part — needs a grammar decision |

- `MR` needs its six families in `FAMILIES`: `MR-T` (trait), `MR-N` (null relay), `MR-E`
  (encoding), `MR-X` (exchange/timers), `MR-F` (faults), `MR-H` (health). `ME-2` is the story its
  deferrals should name.
- `FR` (flow references, 21 rows) and `AT` (`AF-1`'s token vectors) should register together — they
  share one spec file and one construction.

## Notes
- This is the narrow, mechanical half of what [`EX-2`](EX-2-specify-the-rfc-registry.md) specifies
  and [`CF-2`](CF-2-generate-the-conformance-report-from-the-registry.md) reports from. It is filed
  separately because it is worth doing **before** those land: the registry is a bigger design, and
  in the meantime four specs' worth of vectors are unenforced.
  `docs/reference/vector-scope.toml`'s header already anticipates folding into that registry.
- Relevant: `scripts/check-vectors.py` (`SPECS`, `ROW`, `TEST_NAME`, `COVERS`, `FAMILIES`),
  `docs/reference/vector-scope.toml`, `docs/reference/conformance.md` (generated, checked in).
- Found by: [`ME-1`](ME-1-specify-mediarelay-and-the-ng-adapter-contract.md) and
  [`AF-2`](AF-2-specify-flow-references-and-connection-ownership.md).
