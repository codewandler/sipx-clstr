---
id: CF-25
title: A new spec can carry normative rules no gate ever enumerates
pillar: Foundation
status: ready
priority: 1
epic: conformance-harness
areas: [gate, conformance, specs]
note: AF-3 added a 296-line normative spec and check-vectors.py stayed green at 154/583 without seeing one rule of it
---

# A new spec can carry normative rules no gate ever enumerates

## Goal

Make an unregistered normative spec impossible to ship silently: every file under `docs/specs/` is
either registered in the checker's spec set or explicitly and visibly excluded, so a spec's rules
cannot exist outside the coverage picture that claims to describe them.

## Acceptance

- [ ] `scripts/check-vectors.py` enumerates `docs/specs/*.md` against its own `SPECS` table
      (`scripts/check-vectors.py:92-118`) and fails on any spec that is registered nowhere. An
      exclusion is a named entry with a reason, not an absence.
- [ ] `EX-12`'s unowned-row guard fires on rule tables whose first cell is not shaped `XX-N`.
      `ANY_ROW_LINE` (`scripts/check-vectors.py:131`) matches only that shape today, so a table
      keyed by, for example, a backticked test-function name is invisible to it.
- [ ] **Failing-first:** on a tree containing `docs/specs/owner-rpc.md` with no `SPECS` entry, the
      check is red and names the file. Demonstrated red before the fix, green after — and green only
      because the spec is registered or explicitly excluded, not because the rule was relaxed.
- [ ] The published conformance count and inventory move with it: a spec that is registered but
      contributes no rows must not read as covered. Coordinate with `CF-21`, which owns holding a
      published count to its generator.
- [ ] `scripts/gate.sh` is green.

## Progress
- (not started)

## Notes

- **Found by the independent review of `AF-3`'s diff, 2026-07-31**, while answering the question
  "can the gate be green with thirteen named, unexecuted scenarios and no registered rows?" The
  answer was yes. `docs/specs/owner-rpc.md` is 296 normative lines with a §10 scenario table, and
  `check-vectors.py --check` passed at **154/583 proved, 19 shape-only, 410 deferred, exit 0** with
  none of it in view. `SPECS` has no `owner-rpc` entry, and `EX-12`'s guard could not fire because
  §10's first cell is a backticked function name rather than `XX-N`.
- **This is a gate hole, not `AF-3`'s defect.** `AF-3` had a sound reason not to register rows — a row
  and the test that executes it should arrive in the same commit, which is `cluster-membership` §11's
  own reasoning and what `CF-8`/`EX-12` paid for. The hole is that *choosing* not to register leaves
  no trace anywhere a tool looks. §11 got this right by mapping onto an already-registered prefix
  whose rows all sit in `vector-scope.toml` with a reason and a story, so `CF-16`'s sweep finds them.
- **Same family as `CF-19`, `CF-21`, `CF-23` and `CF-24`** — a document making a claim no tool
  compares against anything. `CF-24`'s note says whoever lands the last of them should state the
  general rule once in `AGENTS.md`; this story is now part of that set, and it touches the same file
  as `CF-24` (`scripts/check-vectors.py`), so the two should not run concurrently.
- Considered for upstream: **no.** This checks this repository's own spec set and coverage ledger.
