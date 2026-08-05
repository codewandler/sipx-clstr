---
id: CF-18
title: A story can read done while its own record says nothing landed
pillar: Foundation
status: done
priority: 2
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [docs, ci]
note: complete — done stories now require an exact changelog citation and a checked Acceptance item; all historical records are reconciled
---

# A story can read done while its own record says nothing landed

## Goal

`AGENTS.md` asserts two things about a closed story: closed stories "roll up" into `CHANGELOG.md`, and
"every story's frontmatter is complete and the board regenerated". Nothing checks either, and both are
false today.

Measured on the tree at `62dcc4f`, over 81 `status: done` stories:

- **9 are cited nowhere in `CHANGELOG.md`** — `DX-2`, `DX-4`, `DX-7`, `DX-8`, `DX-9`, `DX-10`, `FC-2`,
  `RG-13`, `RG-14`. The convention is not in doubt: 89 distinct story IDs *are* cited, in the settled
  form `- **What changed** (`ID`).`, so these nine are misses rather than a different style.
- **3 have not one ticked acceptance box** — `FC-3`, `RG-13`, `RG-14`. Every box is `- [ ]`, on stories
  whose work demonstrably landed and is in the git history. The story record was never updated when it
  closed.
- `RG-13` and `RG-14` fail both, and `RG-13` has read `done` on the board while its ledger said nothing
  about it and its own acceptance claimed nothing was done.

Both properties are mechanically checkable against files already in the repository, and neither needs
a new convention invented for it — which is what makes this worth a gate where the sweep's other
findings were not (see Notes).

## Acceptance

- [x] **Failing-first**: the check is red on the twelve instances above before anything is corrected,
      and its output names each story and which property it failed. A run that reports a count without
      naming the stories cannot be acted on.
- [x] Every `status: done` story is cited by ID in `CHANGELOG.md`. Decide and record what counts as a
      citation — the `(`ID`)` form above is the observed convention — and make the check read that form
      rather than a bare substring, on `CF-15`'s lesson about `name in text`.
- [x] Every `status: done` story has at least one ticked acceptance box. Deliberately, this is the weak
      form: an unticked box on a closed story is legitimate and common — `CF-4` moved an item to
      `CF-3`, `PX-8` records the untaken branch, `CF-5` struck one through — so the check must not
      require *all* boxes ticked. **31 unticked boxes across 14 done stories** are almost all of that
      benign kind; a check demanding a full sweep of ticks would fire on all of them and be suppressed
      within a week.
- [x] The nine missing `CHANGELOG.md` entries are written and the three stories' acceptance boxes are
      reconciled, in the same change as the check. A gate that lands red is a gate that gets commented
      out — the check and the twelve corrections are one story on purpose.
- [x] Whether the board (`docs/stories/README.md`) is regenerated from current frontmatter is checked
      too, or explicitly left out with a reason. It is the third claim in the same `AGENTS.md` bullet
      and the cheapest of the three to verify.
- [x] `scripts/gate.sh` green, and the check runs inside it. If it is added to a workflow instead,
      `scripts/check-site.py` reads invocations rather than mentions — a commented-out or merely
      *named* invocation will not satisfy it.

## Progress

- **Failing-first is captured at the check boundary.** The self-test constructs the ten historical
  story records named above and requires exactly twelve property failures: all nine missing citations
  and the three unchecked Acceptance sections, including both failures for `RG-13` and `RG-14`.
  Every message must name the story file, exact ID and failed property. A near-match fixture proves a
  bare `DX-2` mention and a citation for `DX-20` do not satisfy `DX-2`; a clean fixture proves that a
  grouped parenthetical citation, a qualifier and one checked box among unticked alternatives pass.
- **The live-tree failing run found fifteen defects before repair.** Eleven done stories lacked a
  parenthetical exact-ID citation: the original nine plus `CF-8` and the subsequently closed `FC-3`.
  Four lacked any checked Acceptance item: the original `FC-3`, `RG-13`, `RG-14` set plus subsequently
  closed `KO-18`. `scripts/check-docs.sh` printed all fifteen records and exited 1; after the ledger
  and story reconciliation it reports all 263 tracked Markdown files clean.
- **Citation grammar is narrow and recorded in code.** A citation is a complete `AA-123` token inside
  an outer parenthetical group; code ticks are canonical but optional for early grouped release
  entries. Parentheses are scanned with nesting so a Markdown link inside the same citation does not
  hide the story ID. Bare prose and longer IDs cannot satisfy the record.
- **Historical records are reconciled without pretending every box must be checked.** The changelog's
  `CF-18` entry restores the eleven missing exact-ID records. `FC-3`, `RG-13`, `RG-14` and `KO-18`
  now check the criteria their own Progress and later proofs establish; deliberately unlanded or
  handed-off criteria remain unticked and are explained beside them.
- **Board currency is explicitly left out, with the reason required by Acceptance.** The board's
  static preamble, this story, the accepted design and the check's module contract all say the same
  thing: frontmatter is authoritative; track 0.5.0 regenerates the board after board-visible changes;
  the gate does not fork the external generator into a second implementation that could drift.
- Considered for upstream: **no — repository-local tracking policy.** This checks sipx-clstr story
  frontmatter and its changelog; it adds no protocol-generic parser, transaction behavior or testkit
  capability to the sipx kernel.
- **Full gate green.** Formatting, clippy, transaction drain, the complete workspace suite, optional
  feature combinations, Rust 1.91, provenance, vectors, CRD drift, docs, proof domains and the public
  site all passed. The docs step ran this closure checker on the repaired live tree.

## Notes

- **This is a different defect family from the sweep that found it, and that distinction is the point.**
  [`CF-16`](CF-16-sweep-for-done-stories-that-left-named-deltas-unlanded.md) swept for *a claim of
  ownership unhonoured* — a design or spec asserting that a specific story will do a specific thing,
  where the story closed and the thing never landed. It found 3 instances and **judged that mechanism
  not worth building**: detecting it needs semantic reading, not pattern matching. Measured over this
  corpus, 100 delegation-shaped mentions name a `done` story and 97 of them are honoured or benign
  narration, so a checker on that property runs at roughly 3% precision. `CF-16`'s own text warned
  that "a check nobody can satisfy will be deleted", and that is the check it was warning about.
- What this story checks instead is *closure hygiene*: not whether a story did what someone else said
  it would, but whether a story that claims to be finished left the two records the project already
  requires. Total, no judgment calls, no convention to invent, and it would have caught `RG-13`
  reading `done` for weeks with an empty ledger and an empty acceptance.
- It would **not** have caught `EX-8`, the story the sweep started from: `EX-8` has ticked boxes and a
  CHANGELOG entry, and closed on half its named scope regardless. Say that plainly when closing this,
  so the next reader does not mistake a hygiene gate for a scope gate.
- Suggested by the coordinator during `CF-16`'s implementation wave, from two instances found while
  working the ledger: `DP-10` (`done`, an acceptance item openly ticked "not done", and no CHANGELOG
  entry at all until one was written by hand) and `DP-11` (a commit titled "Ledger for DP-11: **done**"
  that left the story at `status: in-progress` and wrote no CHANGELOG entry).
- A fourth property is available and is the sharpest of them, but needs a convention: a commit whose
  message says it closes story `X` leaves `X` at `status: done`. `DP-11` is the instance. Worth
  considering here, worth declining if the commit-message convention is not stable enough to check.
