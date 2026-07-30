---
id: EX-12
title: Register the quirk vectors so the gate can see them, and land the EX-7 spec deltas nobody owns
pillar: Platform
status: done
priority: 2
epic:
areas: [extensions, ci, docs]
note: the QP rows are decorative — a fabricated row passes --check, because no spec owns the prefix
---

# Register the quirk vectors so the gate can see them, and land the EX-7 spec deltas nobody owns

## Goal

The quirk-profile vector rows (`QP-*`) are **not enforced by anything**. `scripts/check-vectors.py`
reads rows only out of the spec that *owns* a prefix (`SPECS`, and the comment at :50 states the rule
explicitly), and `QP` is owned by no spec — the rows live in `docs/designs/extension-framework.md`, a
design record. `EX-11` proved this rather than assumed it: it temporarily inserted a fabricated
`QP-Z-999` row and `python3 scripts/check-vectors.py --check` exited **0**.

So every `QP` row added so far — including the four `EX-11` wrote to pin its own derivation — is
prose that looks like a measurement. `CF-8` made the conformance headline honest by counting *proved*
rows rather than deriving the number; this is the same class of defect one level down, where the rows
themselves are outside the instrument.

The reason they cannot simply be moved is the second half of this story: registering `QP` means
putting quirk bindings into `docs/specs/hook-framework.md` §7/§9, and **the `EX-7` spec deltas are
owned by nobody**. The extension design names `EX-8` for the `SyntaxDecl` `Replace`/`Field` split and
rules `G9`–`G14`, but `EX-8`'s acceptance covered `EX-6`'s half only and `EX-8` is `done`.
`media_types_rewritten` is still a flat `&'static [MediaType]` at `hook-framework.md:332`. One
consequence is concrete: a deployment cannot both anchor media and run an SDP quirk.

## Acceptance

- [x] **Failing-first**: a fabricated `QP` row is added and `scripts/check-vectors.py --check` is
      shown to **exit 0** at the merge base and to **fail** after. That inverted proof is the whole
      story — the current instrument cannot tell a real row from an invented one.
      → At `bc36197`, `QP-Z-999` in the design's table: `vectors: 120/498 rows proved …`, exit **0**.
      After, the same row in `hook-framework.md` §9.1: `QP-Z-999: in the spec, covered by no test,
      and not deferred` → `vectors: FAIL — 2 problem(s)`, exit **1**.
- [x] `QP` is owned by exactly one spec and appears in `SPECS`, or the `QP` rows are removed from the
      design and re-expressed under a prefix that is already owned. Rows that no gate reads must not
      remain while looking like vectors.
      → All 31 rows moved to `hook-framework.md` §9.1; `SPECS["QP"]` points at that **spec**, never
      at the design. `grep -c '^| QP-' docs/designs/extension-framework.md` → 0.
- [x] The `EX-7` spec deltas land in `docs/specs/hook-framework.md`: the `SyntaxDecl`
      `Replace`/`Field` split, `media_types_rewritten` given the shape that lets media anchoring and
      an SDP quirk coexist, and rules `G9`–`G14` stated normatively rather than only in the design.
      → §6 `BodyClaim` (`Replace`/`Field`) replacing `media_types_rewritten`, §4 E3 rescoped, §7
      G9–G14, and §8.1 for the types G10–G14 quantify over.
- [x] Every `QP` row now visible to the gate is either proved by a test or carries a deferral naming
      real work. A row that becomes visible and is silently waived has not been registered, it has
      been hidden somewhere new.
      → 31 `[[deferred]]` entries against `EX-3`, each with its own reason; all three `QP` families
      added to `FAMILIES`, so every row renders in `conformance.md` rather than being counted and
      never shown.
- [x] `docs/stories/EX-7-*.md:21` no longer describes the composition as a "disjoint union over the
      composed set" — `EX-11` derived that the sets never intersect, so the sentence is false.
- [x] `scripts/gate.sh` green, and `check-vectors.py` reports a row count that went **up**.
      → 498 → **529** rows (+31); deferred 358 → 389; proved unchanged at 120.

## Progress

- 2026-07-30 — **done.** The ordering the story predicted held: the vector registration was blocked
  by the spec deltas, so the deltas landed first and the prefix second, in one change.
- **The spec half.** `hook-framework` §6 now carries `BodyClaim { Replace(MediaType), Field(MediaType,
  CatalogSdpField) }` in place of the flat `media_types_rewritten`, which is what lets `media-anchor`
  (a whole-body `Replace`) and an SDP quirk (a `Field` write) coexist instead of colliding as a G2
  exclusive claim. §4 E3 was rescoped to match — it named the old field. §7 gained G9–G14 verbatim
  from the design, and §8.1 was written to give G10–G14 something defined to quantify over:
  `QuirkProfile`, the binding form, the elementary-target definition, the media assertion and the
  v1 catalogue. G2 keeps the catalogue invariant and defers its body clause to G9.
- **The vector half.** All 31 `QP` rows moved from `docs/designs/extension-framework.md` into
  `hook-framework` §9.1 and `SPECS["QP"]` points at that spec. `hook-framework` now owns two
  prefixes, which is the mirror of `affinity-token` already being pointed at by both `AT` and `FR`.
- **Every one of the 31 is deferred against `EX-3`**, with a per-row reason. That is the honest
  answer rather than a convenient one: nothing executes a quirk profile — no `carrier-quirks`
  module, no runtime, no catalogue to carry bindings — so no row could be proved without inventing
  an implementation. It is the same position `HF-1` … `HF-13` are in, recorded the same way. The
  three `QP` families were added to `FAMILIES` as well; without that the rows would have counted
  towards the total and rendered in no table, which is exactly the "hidden somewhere new" the
  acceptance forbids.
- **The instrument got a new rule, not just a new entry.** `unowned_rows()` fails the check on any
  table row shaped like a vector whose prefix `SPECS` does not own, scanning `docs/specs` **and**
  `docs/designs`. Registration is no longer something to remember: this defect class cannot recur
  silently. Demonstrated — a `ZZ-Z-999` row appended to the design produced
  `docs/designs/extension-framework.md:1213: a table row with the shape of a vector, but no spec
  owns the prefix \`ZZ\``. `QP` was the only unowned prefix in the repo when the rule was written.
- **A trap worth recording**: `spec_rows()` reads row IDs from *anywhere* in an owning spec, not only
  from table lines. Writing "a fabricated `QP-Z-999` row passed" into §9.1's prose conjured a real
  row and failed the gate. The sentence is now written without spelling an ID, and says why.

## Notes

- Filed by the coordinator at `EX-11`'s integration. `EX-11` disclosed this about its **own** output
  rather than letting the rows pass as proved, which is the only reason it was caught; the fabricated
  `QP-Z-999` was removed before it committed (`grep -c QP-Z-999` → 0).
- Do not close this by adding `QP` to `SPECS` pointing at a design record. `SPECS` maps a prefix to a
  *spec*; aiming it at `docs/designs/` would make the design normative by accident and break the
  two-tree split `AGENTS.md` states.
- The ordering is the interesting part: the vector registration is blocked by the spec deltas, and
  the spec deltas were orphaned because a story closed on half its scope. Worth a look at whether any
  other `done` story left a named delta behind.
