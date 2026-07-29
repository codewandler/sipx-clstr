---
id: CF-8
title: Bring every spec's vector table under the vector gate
pillar: Platform
status: ready
priority: 1
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [conformance, specs]
note: proved at wave 4 — a fabricated AI row passes the gate, so 145 rows are unenforced prose
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
- [x] The two-part row shape is resolved for `affinity-token` (`AT-n`, `FR-n`) and `hook-framework`
      (`HF-n`). Decide **and record** which way it goes: widen the row grammar to accept two-part
      IDs, or renumber those tables into families. Renumbering churns every existing citation, so
      the reason for the choice matters more than the choice.
      → **Done by `EX-8`**, which widened `ROW` to make the family letter optional
      (`\b({PREFIXES})-(?:([A-Z])-)?(\d+)\b`) and recorded the reason in the module docstring:
      renumbering would churn every citation in the spec that owns the rows. `AT`/`FR` inherit the
      decision and need registration only.
- [ ] `CC` (cluster-config, 48 rows, `DP-1`) and `AI` (asserted-identity, 97 rows, `RT-7`) are
      registered too, along with `NN` (number-normalisation). Both `CC` and `AI` arrived **after**
      this story was filed, which is the argument for doing it now rather than later: each wave
      that ships a spec adds rows the gate cannot see, so the unenforced set grows faster than the
      enforced one.
- [ ] Every newly-registered row is either covered by a test or carries a `vector-scope.toml`
      deferral with a reason and a story — the existing three-way discipline, including that a
      deferred row which *is* covered stays an error.
- [ ] `docs/reference/conformance.md` regenerates to include the new families, and
      `scripts/check-vectors.py --check` is green in `scripts/gate.sh`.
- [ ] The gate's summary line reports the true denominator. At filing it read `75/83 rows proved`
      while silently excluding four specs; it now reads `77/98` and excludes six. After this story
      that number covers everything or the excluded set is explicit.
- [ ] **A fabricated row fails the gate, proved by trying it.** This is the acceptance that matters:
      the defect is not that the count is low, it is that an unregistered prefix is invisible.

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
- **Re-measured 2026-07-29, at wave 4's integration.** `EX-8` registered `HF` and settled the
  grammar question; `DP-1` and `RT-7` then added two more unregistered families. Current state:

  | Spec | Row family | In the gate? | Rows |
  |---|---|---|---|
  | `proxy-behavior.md` | `PB` | yes | — |
  | `e2e-probe.md` | `EP` | yes | — |
  | `registrar-auth.md` | `RA` | yes | — |
  | `hook-framework.md` | `HF` | yes — `EX-8` | 13, all deferred to `EX-3` |
  | `asserted-identity.md` | `AI` | **no** | 97 (`RT-7`) |
  | `cluster-config.md` | `CC` | **no** | 48 (`DP-1`) |
  | `location-service.md` | `LS` | **no** | — |
  | `media-relay.md` | `MR` | **no** | — |
  | `number-normalisation.md` | `NN` | **no** | — |
  | `affinity-token.md` | `AT`, `FR` | **no** | 21 (`FR`) + `AT` |

- **The failure mode is demonstrated, not inferred.** At wave 4's integration the coordinator
  appended a fabricated row — a table line reading `AI-Z-99`, citing no test — to
  `asserted-identity.md` and re-ran `scripts/check-vectors.py --check`. It reported
  `77/98 rows proved` and **exited 0**: byte-identical to the untampered run. An unregistered
  prefix is not under-counted, it is invisible, so a spec author can cite any `AI`, `CC`, `LS`,
  `MR`, `NN`, `AT` or `FR` row as normative and every gate in the project stays green. The file was
  restored and the tree verified clean afterwards.

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
