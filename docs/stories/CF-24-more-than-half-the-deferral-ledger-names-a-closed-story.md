---
id: CF-24
title: More than half the deferral ledger names a story that has already closed
pillar: Foundation
status: done
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
- [x] `scripts/check-vectors.py` fails when a `[[deferred]]` entry names a story whose `status` is
      `done` — it reads `story` today and never reads that story's status.
- [x] The same check covers `[[unasserted]]` entries if they name a story.
- [x] **Failing-first:** the check is red on the current tree, naming all 239 rows and their four
      stories. It goes green only once every one is re-pointed.
- [x] The 239 rows are re-pointed at the story that will actually cover them, or covered, or
      explicitly re-deferred with a live owner. **A deferral may not name the story that wrote the
      spec** — see the diagnosis below; that is the mechanism that produced most of these.
- [x] `docs/reference/conformance.md`'s wording is reconsidered: it reports a deferred count as
      though every entry has a live owner, which is the claim this story falsifies.
- [x] The header tally in `docs/reference/vector-scope.toml` is corrected if the sweep moves rows
      between prefixes; that file keeps its counts exact. (The sweep moved none: 410 rows over the
      same thirteen prefixes, still naming fifteen stories — recounted against the swept file, and
      the header was already exact.)

## Progress

- **2026-07-31 — the check is written and reportedly red on exactly the 239 rows; the ledger sweep
  never started.** An implementor was killed mid-story by an org monthly spend limit. Its work was
  rescued by the coordinator and committed as **`impl/CF-24` at `e74b535`** — `scripts/check-vectors.py`
  only, +223/-22 — with the worktree preserved at `/home/timo/projects/sipx-clstr-CF-24`. That commit
  is **WIP: not gated, not reviewed, and red against the tree by design**, because the 239 rows it
  names are still pointing at closed stories. Resume there rather than from scratch.
- **The agent's last report was "the check is red on exactly the 239 rows", which is the shape
  Acceptance item 3 asks for — unverified by me.** Re-run it at the merge base before trusting it.
- **What remains:** the whole ledger sweep. `docs/reference/vector-scope.toml` is untouched, so no row
  has been re-pointed, `[[unasserted]]` coverage is unconfirmed, `docs/reference/conformance.md`'s
  wording is unreconsidered, and the header tally is uncorrected.
- **`DP-8`'s 51 rows have a confirmed destination.** `DP-16`'s Acceptance explicitly claims
  `CC-K-1`…`CC-K-6`, `CC-S-1`…`CC-S-9`, `CC-V-4`/`CC-V-10`, `CC-R-7`/`CC-R-8` and `CC-I-1`/`CC-I-4`
  and says their deferrals are "re-pointed here from `DP-8`" — so that side of the split is settled and
  needs no judgement call. `RT-7`'s 97 and `ME-1`'s 90 still need an implementing story per subsystem.
- **A sibling gate hole found while this was in flight**, same family and worth landing near it:
  nothing enumerates `docs/specs/*.md` against this script's `SPECS`, so a whole normative spec can
  carry rules no gate ever sees. Filed as
  [`CF-25`](CF-25-a-new-spec-can-carry-normative-rules-no-gate-enumerates.md) — do not absorb it here,
  but the two touch the same file.
- **Fence note for whoever resumes:** `docs/reference/vector-scope.toml` is normally coordinator-owned,
  and I lifted that for this story only, because the file is its subject matter. The rest of the fence
  holds.
- **2026-08-05 — resumed on `impl/CF-24`, merged `main` (clean), swept, green.** Verified the WIP
  check red at the merge base's ledger state: exactly 239 problems, naming `RT-7` 97, `ME-1` 90,
  `DP-8` 51, `PX-2` 1. Then re-pointed every one:
  - `AI-*` 97 → **`RT-2`** — asserted-identity's own coverage note: "`RT-2`'s trunk model is the
    story that lands them".
  - `MR-*` 90 → **`ME-2`** — media-relay §12: "`ME-2`'s tests derive from these"; the trait lands
    with `ME-2` per the spec header.
  - `CC-*` 51 split by consumer: **`DP-16`** 35 (the 21 its Acceptance claims by name, plus the 14
    plain loader-validation rows — `CC-D-1`…`9`, `CC-I-2`/`3`, `CC-V-5`/`8`/`11` — as the loader's
    one live story); **`RT-2`** 9 (`CC-T-1`…`4`, `CC-V-1`/`2`/`3`/`6`/`7` — the `trunk[]` section,
    per the consumer table and RL7/RL8); **`DP-13`** 5 (`CC-R-2`…`6`, its Acceptance matrix);
    **`FC-1`** 2 (`CC-R-9`/`10`, listener exposure per its V-07 item).
  - `PB-C-4` → **`PX-4`** — the reason always said "PX-4's stateless path"; only the `story` field
    had never caught up.
  The 14 loader rows to `DP-16` are the one judgement call: no story's Acceptance claims them, and
  `DP-16` is the only live story working the loader. The new gate makes the call self-correcting —
  the day `DP-16` closes without them, every one goes red again and must be re-pointed or covered.
  `conformance.md`'s preamble now states the enforced rule (live story, not the spec's author) and
  the gate's green line reads "deferred with a live owner".

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
