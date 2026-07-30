---
id: CF-17
title: The conformance report omits three quarters of the rows it counts
pillar: Foundation
status: ready
priority: 1
epic: conformance-harness
areas: [conformance, specs]
note: CF-8 registered seven prefixes in SPECS and none of their 30 families in FAMILIES, so 395 of 533 rows render nowhere
---

# The conformance report omits three quarters of the rows it counts

## Goal

`docs/reference/conformance.md` says **125 of 533 rows proved**. It then prints tables for `PB`, `EP`,
`RA`, `HF` and `QP` only — 138 rows. The other **395 rows appear in the denominator and in no table**:

| Prefix | Rows | In the report? |
|---|---|---|
| `AI` (asserted-identity) | 97 | no |
| `MR` (media-relay) | 90 | no |
| `LS` (location-service) | 62 | no |
| `CC` (cluster-config) | 54 | no |
| `NN` (number-normalisation) | 52 | no |
| `FR` (affinity-token, flow refs) | 22 | no |
| `AT` (affinity-token) | 18 | no |

The cause is one data structure. `scripts/check-vectors.py` has **twelve** prefixes in `SPECS` and
only **five** represented in `FAMILIES`, and `render()` iterates `FAMILIES` — so a row whose
`(prefix, letter)` pair is not a `FAMILIES` key is counted by every other part of the script and then
silently skipped when the report is written. Thirty families are missing: `AI-A`, `AI-C`, `AI-D`,
`AI-N`, `AI-P`, `AI-S`, `AI-T`, `AI-X`, `AT-`, `CC-D`, `CC-I`, `CC-K`, `CC-R`, `CC-S`, `CC-T`, `CC-V`,
`FR-`, `LS-C`, `LS-H`, `LS-K`, `LS-L`, `LS-R`, `MR-C`, `MR-E`, `MR-F`, `MR-H`, `MR-N`, `MR-P`, `MR-T`,
`MR-X`, `NN-B`, `NN-C`, `NN-E`, `NN-G`, `NN-T`, `NN-X`.

This is `CF-8`'s named-but-unlanded half, and `CF-8` is `done` with both halves ticked:

- "`LS` … and `MR` … are registered in `SPECS`, `ROW`, `TEST_NAME` and `COVERS`, **with their
  families named in `FAMILIES`**" — `[x]`, and no `LS` or `MR` family was ever added.
- "`docs/reference/conformance.md` regenerates to include the new families" — `[x]`, and it does not.

`CF-8` did the hard part correctly: the prefixes are registered, the rows are counted, the deferrals
are in `vector-scope.toml`, and the gate does enforce them. Only the *report* never learned about
them, which is why nothing caught it — `check-vectors.py --check` is green, because being absent from
`FAMILIES` is not a condition the checker tests.

## Acceptance

- [ ] **Failing-first**: a check asserts that every `(prefix, letter)` pair occurring in
      `spec_rows()` is a key of `FAMILIES`, and it is red on the thirty pairs above before it is
      green. Being registered in `SPECS` but unrepresentable in the report must become a gate
      failure, not a silent skip — that is the defect, not the missing titles.
- [ ] All thirty families are named in `FAMILIES`, with section titles in the established form
      (subsystem — what the family covers, and the owning spec section).
- [ ] `docs/reference/conformance.md` regenerates and every one of the 533 rows it counts appears in
      exactly one table. The proved/shape-only/deferred split per row is unchanged — this story adds
      no coverage and must not appear to.
- [ ] The headline number is checked against the rendered tables rather than only computed:
      Σ rows over the emitted sections equals `len(rows)`, or the report says which rows it excluded
      and why. A denominator that counts what the document does not show is the bug being fixed.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

## Notes

- Found by [`CF-16`](CF-16-sweep-for-done-stories-that-left-named-deltas-unlanded.md)'s sweep, as one
  of three instances of a `done` story named by another document as owner of a delta that never
  landed. The naming documents are [asserted-identity](../specs/asserted-identity.md) §13
  ("Registration is deferred to `CF-8`… `docs/reference/vector-scope.toml` and
  `docs/reference/conformance.md` carry no `AI` rows… Until `CF-8`, this table is unenforced"),
  [cluster-config](../specs/cluster-config.md) §12 and
  [number-normalisation](../specs/number-normalisation.md) §11.
- **Those three spec paragraphs are now half-false and are part of this story's diff.** `AI`, `CC`
  and `NN` *are* registered in `check-vectors.py`, and `vector-scope.toml` does carry their rows —
  97 `AI` rows alone. Only the `conformance.md` clause is still true. Correct them to describe the
  remaining gap rather than deleting them, so the reason the rows are deferred survives.
- `render()`'s own comment already records `CF-8` breaking an assumption about the headline number
  ("`CF-8` registered six more specs and the assumption broke: the report claimed 471 proved while
  337 rows had no test at all"). The count was fixed then; the sections were not.
- `MR`'s six families were written down in `CF-8`'s Progress at filing time — `MR-T`, `MR-N`, `MR-E`,
  `MR-X`, `MR-F`, `MR-H`. The spec has since added `MR-C` and `MR-P`, so take the families from the
  rows rather than from that list.
- Related and deliberately separate: `CF-12` made *proved* mean the test compares the row's stated
  value. This story is about rows that are not on the page at all, which no amount of `CF-12` would
  find.
