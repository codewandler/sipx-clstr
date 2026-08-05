---
id: CF-21
title: Hold every published count to its generator, not to whoever last remembered
pillar: Foundation
status: ready
priority: 2
epic: conformance-harness
areas: [gate, docs]
note: the conformance numbers on the README and the site went stale three times in one session, each time through a green gate
---

# Hold every published count to its generator, not to whoever last remembered

## Goal
Make a published number that disagrees with the tool that produces it a red gate, the same way
`CF-19` made a published version banner that disagrees with the binary a red gate.

## Acceptance
- [x] `scripts/check-site.py` (or a sibling in the gate) fails when a conformance count quoted in
      `README.md` or under `website/docs/` disagrees with `scripts/check-vectors.py`'s own output —
      proved, shape-only, deferred and the total, each of them. **Landed with `DX-14`.**
- [ ] The count of registered spec **prefixes** is checked the same way. `DX-14` derived the count
      of registered *specifications* from the registry, but `SPEC_COUNT` only reads the number in
      front of the word "specifications" — so "across fifteen prefixes"
      (`website/docs/reference/conformance.md`) and "Fifteen prefixes" (`website/docs/whats-new.md`)
      are still hand-copied. Proved at `DX-14`'s review: registering a sixteenth prefix on an
      already-registered spec left both "fifteen" claims green.
- [x] The check finds the numbers by shape wherever they appear, including inside the shields.io badge
      URL, rather than by a list of known line numbers — the badge is the copy most likely to be
      missed and the most publicly visible. **Landed with `DX-14`**, which caught both the badge URL
      and its alt text when the ledger moved to 173/602.
- [x] A *historical* number in a released section is not flagged. `whats-new.md`'s "0.12.0 shipped
      125/549" is a true statement about a past release; only current claims are held. State how the
      two are told apart. **Landed with `DX-14`**: scanning stops at `whats-new.md`'s `## Releases`
      heading, and the skipped line count is printed on every run so the narrowing is visible.
- [x] **Failing-first:** with a count one merge behind, the gate is red and names the file, the line,
      the published figure and the generated one. Demonstrated before it is made green. **Landed
      with `DX-14`**, and demonstrated twice more at its review by fabricating a vector row.
- [x] The static-reading discipline `CF-19` established is kept: if the generator cannot be run, say so
      on every run rather than passing silently. **Landed with `DX-14`**: the site check imports
      `check-vectors.py` by path and refuses to exit 0 if that import fails.

**What is left, and it is the whole of this story now:** the count of registered spec
**prefixes**, and making the published counts *generated* rather than hand-copied.

## Progress
- (not started)

## Notes
- **Filed from three separate corrections in a single integration run**, each of which passed a full
  green gate first: `125/549 → 125/573` after the review backlog registered 24 rows, `→ 128/576` after
  `PX-14` added the `PB-T` family, `→ 129/576` after `DP-13` proved `CC-R-1`. Every implementor
  correctly declined to touch the published copies — with several diffs in flight, each writing a
  different total guarantees both a conflict and a wrong answer — so the number is structurally the
  integrator's, and an integrator is exactly who forgets.
- This is the third instance of one shape. `DX-12` held documented **flags** to what the binary
  accepts. `CF-19` held documented **version output** to what the binary prints. This holds documented
  **counts** to what the generator counts. After it, the rule worth stating in `AGENTS.md` is the
  general one: a number in a published document either comes from a generator or is checked against
  one.
- `docs/reference/conformance.md` is already generated and already gate-checked for staleness. The gap
  is only in the *hand-written* copies that quote it — which are the ones a reader actually meets
  first.
- Considered for upstream: **no.** This checks this repository's published documents against this
  repository's own tooling.

- **Re-scoped 2026-08-05 at `DX-14`'s integration.** Five of this story's six acceptance items
  were implemented by `DX-14`'s check and are ticked above with the evidence; leaving them open
  would have this story claiming work that has landed, which is `CF-18`'s defect in a different
  costume. What remains is one line: the prefix count.
- [ ] **The published counts are rewritten by a generator, not by hand.** `DX-14` made them
      *checked*; they are still hand-edited copies. Since the check is in the gate, every story
      that adds a vector row now turns `README.md` and `website/docs/whats-new.md` red and must
      edit both to reach its own green — `DP-16` hit exactly this and flagged it. So two
      concurrent stories collide on those lines, and the numbers remain a copy of a generated
      figure, which is the shape this story exists to remove. A `--write` mode on the check (or a
      sibling generator the gate runs) settles both.

