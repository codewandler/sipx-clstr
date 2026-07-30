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
- [ ] `scripts/check-site.py` (or a sibling in the gate) fails when a conformance count quoted in
      `README.md` or under `website/docs/` disagrees with `scripts/check-vectors.py`'s own output —
      proved, shape-only, deferred and the total, each of them.
- [ ] The count of registered spec prefixes is checked the same way; `README.md` said "Ten
      specifications" and the site said "Thirteen prefixes" while the registry held eleven and
      fifteen respectively.
- [ ] The check finds the numbers by shape wherever they appear, including inside the shields.io badge
      URL, rather than by a list of known line numbers — the badge is the copy most likely to be
      missed and the most publicly visible.
- [ ] A *historical* number in a released section is not flagged. `whats-new.md`'s "0.12.0 shipped
      125/549" is a true statement about a past release; only current claims are held. State how the
      two are told apart.
- [ ] **Failing-first:** with a count one merge behind, the gate is red and names the file, the line,
      the published figure and the generated one. Demonstrated before it is made green.
- [ ] The static-reading discipline `CF-19` established is kept: if the generator cannot be run, say so
      on every run rather than passing silently.

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
