---
id: CF-27
title: Validate story status values, instead of denylisting `done`
pillar: Core
status: ready
priority: 3
design:
epic:
areas: [gate]
note: found reviewing CF-24 — the dead-letter gate fails open on a mistyped status, and the template's {{ID}} registers as a live story
---

# Validate story status values, instead of denylisting `done`

## Goal

Close the fail-open edge CF-24's reviewer found: `check-vectors.py`'s `CLOSED_STATUSES` is an
exact-match denylist, so a story whose frontmatter status is mistyped (or any future
closed-meaning status) counts as a **live** owner — the failure direction is "accept", in a gate
whose whole point is refusing dead letters. No other gate validates status values at all.

## Acceptance

- [ ] A story frontmatter `status` outside the five lifecycle values
      (`backlog | ready | in-progress | blocked | done`) is refused by the gate — in
      `check-vectors.py`'s status ingestion or a dedicated `check-docs.py` rule, whichever the
      implementation argues is the right home; the failing-first test feeds a story with
      `status: don` and requires a red gate naming the file.
- [ ] `docs/stories/_TEMPLATE.md` no longer registers `{{ID}}` as a live story in
      `story_statuses()` — excluded by name or by refusing non-conforming ids, with the self-test
      case that proves a deferral naming `{{ID}}` is refused as "no story", not accepted.
- [ ] `check-vectors.py`'s module docstring and `--self-test` success line describe the full case
      set they actually replay (both still describe only the PB-F-1 case).

## Progress

- Filed at CF-24's integration from its independent review's three minor findings
  (fail-open denylist · template ingestion · stale self-test description).

## Notes

- CF-24's `dead_letters()` and its self-test are the code under change; keep the one-problem-per-row
  rule and the spec-author refusal intact.
