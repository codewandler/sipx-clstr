---
id: CF-24
title: More than half the deferral ledger names a story that has already closed
pillar: Foundation
status: ready
priority: 1
epic: conformance-harness
areas: [gate, conformance]
note: 239 of 428 deferred rows name a done story — the report says "deferred with a reason" and 56% of those reasons are dead letters
---

# More than half the deferral ledger names a story that has already closed

## Goal
Make a deferral that names a closed story fail the gate, and re-point the 239 rows that already do,
so "deferred with a reason" means a reason someone will act on.

## Acceptance
- [ ] `scripts/check-vectors.py` fails when a `[[deferred]]` entry names a story whose `status` is
      `done` — it reads `story` today and never reads that story's status.
- [ ] The same check covers `[[unasserted]]` entries if they name a story.
- [ ] **Failing-first:** the check is red on the current tree, naming all 239 rows and their four
      stories. It goes green only once every one is re-pointed.
- [ ] The 239 rows are re-pointed at the story that will actually cover them, or covered, or
      explicitly re-deferred with a live owner. **A deferral may not name the story that wrote the
      spec** — see the diagnosis below; that is the mechanism that produced most of these.
- [ ] `docs/reference/conformance.md`'s wording is reconsidered: it reports a deferred count as
      though every entry has a live owner, which is the claim this story falsifies.
- [ ] The header tally in `docs/reference/vector-scope.toml` is corrected if the sweep moves rows
      between prefixes; that file keeps its counts exact.

## Progress
- (not started)

## Notes
- **Measured on `4b0bc65`:** 428 deferred rows, of which **239 (56%) name a `done` story**.

  | Story | Rows | Status | Kind |
  |---|---|---|---|
  | `RT-7` — Specify per-trunk asserted identity and privacy | 97 | done, 4/4 ticked, in CHANGELOG | **spec** story |
  | `ME-1` — Specify MediaRelay and the NG adapter contract | 90 | done, 3/3 ticked, in CHANGELOG | **spec** story |
  | `DP-8` — Implement the cluster config loader as a pure function | 51 | done, 9/9 ticked, in CHANGELOG | **implementation** story |
  | `PX-2` — Design the proxy transaction driver | 1 | done, 3/3 ticked, in CHANGELOG | design story |

- **The diagnosis, and it splits in two.** For `RT-7`, `ME-1` and `PX-2` — 188 rows — nothing went
  wrong at close. They are spec and design stories, they wrote their specs, and they closed correctly.
  The defect is that **a spec story registered its rows and deferred them to itself**: the moment it
  closes, every row it registered names a story nobody will pick up. A deferral must name the story
  that will *implement* the row, not the one that wrote it down.
- `DP-8` is the other kind and is a genuine `CF-18` instance: an **implementation** story that closed
  `done`, fully ticked, with 51 rows still deferred to it — including every `CC-K` key-reload row
  (6), every `CC-S` shard-map row (9) and every `CC-V` validation row (10). Those three groups are
  exactly what `AF-6` found it could not tick "and tested" against.
- **Nothing checks this.** `check-vectors.py` reads a deferral's `story` field to print it and never
  reads that story's `status`. `CX-6` added a check for the same shape one file over — a `docs/specs/`
  paragraph deferring to a story — precisely because "a story closes, the ledger does not". This is
  that rule, unenforced in the file where the ledger actually lives.
- **`vector-scope.toml`'s own header states the rule this violates:** "a row listed here that *is*
  covered is an error too, because a stale deferral is how a coverage report starts lying." A row
  whose owner has closed is the same lie in the other direction.
- **Why this is urgent rather than tidy.** `RT-*` and `ME-*` are M2 subsystems — trunk routing and
  the rtpengine NG adapter. 187 of their normative rows currently have no owner, so the conformance
  picture for two of M2's five deliverables is fiction. Anyone reading "428 deferred with a reason"
  before starting M2 work would be misled about what remains.
- Related, same shape, already filed: `CF-19` (documented version banners), `CF-21` (published
  counts), `CF-23` (table integrity). All four are documents making a claim no tool compares against
  anything. Whoever lands the last should state the general rule once in `AGENTS.md`.
- Considered for upstream: **no.** This checks this repository's own ledger.
