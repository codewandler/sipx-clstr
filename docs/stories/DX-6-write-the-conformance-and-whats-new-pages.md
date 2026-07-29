---
id: DX-6
title: Write the conformance and what's-new pages
pillar: Foundation
status: done
priority: 2
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: publish the methodology and the legend, not a second copy of a generated table
---

# Write the conformance and what's-new pages

## Goal

Give `website/docs/reference/conformance.md` and `website/docs/whats-new.md` their content: how
correctness is measured here, and where the project stands release by release.

## Acceptance

- [x] `reference/conformance.md` explains the method: every normative rule carries a numbered
      vector; a vector is proved by a test whose name encodes the row, or deferred in
      `docs/reference/vector-scope.toml` **with a reason and a story ID**; `check-vectors.py`
      regenerates the report and fails the gate if the committed copy is stale.
      → "How a rule becomes a measurement", the four numbered steps.
- [x] It states that a deferred row that *is* covered also fails — the report is a measurement,
      not a claim. → "Three ways it fails, and the third is the point".
- [x] It links the generated table by absolute GitHub URL and does **not** duplicate it, so the
      numbers cannot drift. → "The report itself"; the reason for not copying it is stated on the
      page rather than left to the reader.
- [x] Any count quoted in prose is read from `docs/reference/conformance.md` at the time of
      writing, not remembered. → `conformance.md` quotes **no** count at all; `whats-new.md`
      quotes one, *77 of 98 rows proved, 21 deferred*, read from the report's own header line and
      pinned to the 0.9.0 entry that reports it, so it is a historical statement rather than a
      live one.
- [x] `whats-new.md` gives the current state and the recent releases, each bullet leading with
      what changed *for a user*, deferring detail to `CHANGELOG.md` on GitHub. → "Releases",
      0.9.0 back to 0.5.0 with everything earlier collapsed into one paragraph.
- [x] `whats-new.md` names what is still missing, not only what landed. → "Where this actually
      is" (open registrar, in-memory bindings) and "What is still missing" (the table, plus the
      vector-gate coverage gap).

## Progress

- **DX-6 complete.** Both pages authored; the placeholder admonition is gone from each and the
  frontmatter is untouched.
- **Counts, and where each was read.** `conformance.md` deliberately carries none — the page
  publishes the method and the legend and links the generated table, which is the whole point of
  the story's `note:`. The single count on `whats-new.md` was read from
  `docs/reference/conformance.md` line 10 (`**77 of 98 rows proved**; 21 deferred`) and corroborated
  against the `0.9.0` CHANGELOG entry that states the same figure; it is written as a fact *about
  that release*, so it does not rot when the report moves.
- **The coverage gap is published, not buried.** Four specs are registered with the checker
  (`PB`, `EP`, `RA`, `HF`); six are not (`LS`, `MR`, `NN`, `AT`/`FR`, `CC`, `AI`), verified by
  reading `SPECS` in `scripts/check-vectors.py` against `docs/specs/`. Both pages say so and point
  at `CF-8`.
- **Failing-first.** There is no unit test to write inside a two-file write set, so the red→green
  was run against the rule these pages most plausibly break: `conformance.md` was first written
  linking the generated report relatively (`../../../docs/reference/conformance.md`),
  `scripts/check-docs.py` failed with `site link … docs/ is not published`, and the absolute GitHub
  URL turned it green.
- **Gate:** `python3 scripts/check-docs.py` → `docs: clean (166 markdown files checked)`;
  `npm run build` in `website/` → `[SUCCESS] Generated static files in "build"`. Both rendered
  pages were grepped out of `build/` to confirm the MDX survived.
- **Considered for upstream: no.** These are this platform's own conformance methodology and its
  own release history — there is nothing protocol-generic here for the sipx kernel to hold.
- Not done here, and deliberately: `website/docs/reference/configuration.md` is still a
  placeholder (`DX-5`), and both new pages link to it only indirectly.

## Notes

- Source of truth for the numbers: `docs/reference/conformance.md` (generated) and `CHANGELOG.md`.
- The current release is 0.9.0; check the CHANGELOG rather than assuming.
- Prefer no number to a stale number — a count in prose is the thing most likely to rot.
