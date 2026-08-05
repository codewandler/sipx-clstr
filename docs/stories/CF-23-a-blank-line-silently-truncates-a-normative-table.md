---
id: CF-23
title: A blank line silently truncates a normative table, and the gate reads it as clean
pillar: Foundation
status: in-progress
priority: 2
epic: conformance-harness
areas: [gate, docs]
note: RG-25 orphaned a hook-phase row into literal pipe text; check-docs.py passed it, and the spec's own sentence still claimed both rows
---

# A blank line silently truncates a normative table, and the gate reads it as clean

## Goal
Make a broken Markdown table fail the gate, in a repository whose specifications are normative and
whose rules are cited by ID out of table rows.

## Acceptance
- [x] `scripts/check-docs.py` fails on an **orphaned table row**: a line beginning with `|` that
      follows a blank line and is not itself followed by a `|---|` separator.
- [x] It also fails on a table whose header and separator are separated, and on a row whose column
      count does not match its header — the same class, and free once the parser exists.
- [x] **Failing-first:** the check is red on `RG-25`'s pre-fix tree, where a paragraph inserted
      between two rows of `location-service` §5.7 left `AfterRegistrarUpdate` rendering as literal
      pipe text. `git show 02a0228` is gone (the branch was amended), so reproduce it by inserting a
      blank line plus a paragraph between any two rows of any spec table.
- [x] The message names the file, the line, and the row that was orphaned — not merely "malformed
      table".
- [x] It runs over `docs/` **and** `website/docs/`, since the published site has the same failure mode
      with a public reader.

## Progress
- Check 5 added to `scripts/check-docs.py` (`table_problems` / `check_tables`), following the
  self-test convention of `check-crd-drift.py`: fixtures replay each defect on every run, and
  `--self-test` runs only them. Five labels: `orphaned row`, `split table`, `headless table`,
  `ragged row`, `ragged table` — each names the file, line, and row.
- Failing-first proved at base `b646c57`: the RG-25 mutation (blank line + paragraph between the
  §5.7 rows) got `docs: clean (261 markdown files checked)` from the old check; the new check
  refuses it as `orphaned row  docs/specs/location-service.md:539` quoting `AfterRegistrarUpdate`.
  The shape is pinned as `ORPHANED_ROW_FIXTURE` in the self-test.
- The sweep found **one real instance already in the tree**: `docs/specs/e2e-probe.md:172`, §7 A7
  — an unescaped `|` inside the `trigger: scheduled | api` code span split the row into 3 cells
  against a 2-column header, and GFM drops the extras, so the second half of the sentence was
  silently missing from the rendered table. Repaired with `\|`, the escape GFM honors inside
  tables (the convention cluster-membership §3/§4 already uses).
- Cell splitting follows GFM: an unescaped `|` delimits even inside a code span; only `\|` is
  literal. Stripping code spans first (as `prose` does for links) would have hidden the A7 defect.
- File set is `markdown_files()` — every tracked markdown file, a superset of `docs/` and
  `website/docs/`, same as the link check.
- The `CF-19`/`CF-21`/`CF-23` "state the general rule once in AGENTS.md" consolidation the Notes
  suggest is left to whoever lands the last of the three — `CF-19`/`CF-21` were still open here.

## Notes
- **Found by `RG-25`'s review, in `RG-25`'s own diff.** The story whose purpose was to add a normative
  rule to `location-service` inserted its explanatory paragraph between the two data rows of §5.7's
  hook-phase table. A Markdown table cannot survive a blank line, so `AfterRegistrarUpdate` stopped
  being a row — while the sentence directly above it still said the spec "names ... exactly the two
  hook phases". `scripts/check-docs.py` reported `docs: clean (245 markdown files checked)`.
- The implementor then swept the class rather than the instance: **six lines of Python found no other
  orphaned row in 245 files**, and it re-read the three other tables its diff touched. The sweep is
  the check; it just needs to live in the gate rather than in one agent's session.
- **Why this matters more here than in an ordinary repo.** Rules are cited by ID — `Q1`, `B7`,
  `LS-R-24` — and a reader follows the citation into a table row. A row that has silently stopped
  being a row is a normative rule that has silently stopped being readable, and `check-vectors.py`
  will not notice either: it parses vector tables specifically, not every table, and a degraded
  *prose* table is invisible to it.
- Related class, already filed: `CF-19` (documented version banners), `CF-21` (published counts).
  All three are the same shape — a document making a claim that no tool compares against anything.
  Whoever lands the last of them should consider whether `AGENTS.md` wants the general rule stated
  once instead of three times.
- Considered for upstream: **no.** This checks this repository's own documents.
