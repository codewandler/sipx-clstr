---
id: CF-27
title: Validate story status values, instead of denylisting `done`
pillar: Core
status: done
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

- [x] A story frontmatter `status` outside the five lifecycle values
      (`backlog | ready | in-progress | blocked | done`) is refused by the gate — in
      `check-vectors.py`'s status ingestion or a dedicated `check-docs.py` rule, whichever the
      implementation argues is the right home; the failing-first test feeds a story with
      `status: don` and requires a red gate naming the file.
- [x] `docs/stories/_TEMPLATE.md` no longer registers `{{ID}}` as a live story in
      `story_statuses()` — excluded by name or by refusing non-conforming ids, with the self-test
      case that proves a deferral naming `{{ID}}` is refused as "no story", not accepted.
- [x] `check-vectors.py`'s module docstring and `--self-test` success line describe the full case
      set they actually replay (both still describe only the PB-F-1 case).
- [x] (from CF-25's review) `SCENARIO_LINE` documents its two-word evasion but not the
      parenthesized form: a scenario cell written `` `owner_rpc_delivers()` `` is invisible to
      the unowned-row guard. Either the pattern covers it or the comment and a self-test case
      say why not.
- [x] (from CF-25's review) the registered-AND-excluded contradiction check filters to
      `docs/specs/` paths, so an excluded design that later gains a registration would not trip
      it — extend the check to every tree the registries name.

## Progress

- Filed at CF-24's integration from its independent review's three minor findings
  (fail-open denylist · template ingestion · stale self-test description).
- Considered for upstream: no — story lifecycle validation and the vector coverage registries are
  repository-specific tracking/gate machinery, not SIP protocol-kernel behavior.
- Failing-first replay against `HEAD` admitted a complete `status: don` story, registered
  `_TEMPLATE.md` as live `{{ID}}`, missed `` `owner_rpc_delivers()` ``, and accepted an excluded
  design after registration; the stale success line still named only PB-F-1.
- `scripts/check-vectors.py --self-test` passes 53 pinned cases, including commented, structured,
  empty, and quoted status spellings; an integration fixture with
  `status: don` exits 1 naming its file, `--check` is green after removing it, and
  `scripts/gate.sh` is green.

## Notes

- CF-24's `dead_letters()` and its self-test are the code under change; keep the one-problem-per-row
  rule and the spec-author refusal intact.
