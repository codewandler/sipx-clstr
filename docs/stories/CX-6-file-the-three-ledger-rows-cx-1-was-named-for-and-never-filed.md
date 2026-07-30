---
id: CX-6
title: File the ledger rows CX-1 was named for and never filed
pillar: Platform
status: ready
priority: 3
design:
epic:
areas: [upstream, specs]
note: UPSTREAM — three specs name CX-1 as the filer; upstream.md has no row for any of them
---

# File the ledger rows CX-1 was named for and never filed

## Goal

Three specs name `CX-1` in writing as the story that would carry a gap into
[the upstream ledger](../upstream.md):

- [asserted-identity](../specs/asserted-identity.md) §2 — RFC 3966 `tel:` URI splitting, and reading
  RFC 3325 §9.1's one-or-two rule and scheme pairing identically whether the two values arrive on one
  comma-separated line or two header lines (RFC 3261 §7.3.1): "Both are candidate ledger rows for
  `CX-1` to file against sipx".
- [number-normalisation](../specs/number-normalisation.md) §1 — the same RFC 3966 splitter, needed
  *publicly* rather than privately for `N5`/`N7`: "Both are candidate ledger rows for `CX-1` to file
  against sipx; the implementing story (RT-2's trunk model) is blocked on them for `tel:` fields and
  for lossless rewriting". The spec adds that writing a second RFC 3966 splitter here "is precisely
  what rule 6 forbids".
- [proxy-behavior](../specs/proxy-behavior.md) §1 — the RFC 5393 branch-cookie computation (§6):
  "protocol-generic, flagged for CX-1 to raise with the other ledger rows; until decided it is
  implemented here behind a seam that can move".

`CX-1` is `status: done`. `docs/upstream.md` contains **no row** matching RFC 3966, `tel:` splitting,
or the RFC 5393 branch cookie — zero hits for `3966`, `5393` and `branch.cookie` across all sixteen
rows.

This is not `CX-1` failing its acceptance. `CX-1`'s scope was "each **open ledger row** has a sipx
story", and it discharged that: six stories filed, two wrong rows corrected. All three specs above
were written *after* it closed, and each named a story that no longer existed as a live owner. So the
promise was made on `CX-1`'s behalf by documents it never read, which is why nothing noticed.

The cost is concrete and already recorded: `number-normalisation` says `RT-2`'s trunk model is
*blocked* on the splitter, and `AGENTS.md` rule 6 forbids the local workaround. So the blocking
dependency of a future story exists only as a sentence in a spec's §1.

## Acceptance

- [ ] Each of the three gaps is a row in `docs/upstream.md` in the established shape — gap, what sipx
      has today (with the file and line at the pinned tag), sipx story or `— (not yet filed)`, what it
      blocks here, and status. Read the kernel before writing the row: `CX-1`'s own lesson was that a
      ledger row can be wrong, and two of its six were.
- [ ] Each row is decided rather than assumed, per rule 6 and `CX-1`'s precedent with `T-17`. In
      particular the RFC 5393 branch cookie may well be **declined** — `proxy-behavior` §1 already
      says it is implemented here behind a movable seam — and "decided local, with the reason" is a
      valid close for any of the three.
- [ ] The two RFC 3966 asks are reconciled into one row or explicitly kept as two. `asserted-identity`
      wants the split for reading a `tel:` URI's number; `number-normalisation` wants it in **public**
      API for lossless rewriting. If one kernel change serves both, one row serves both.
- [ ] The three spec paragraphs stop naming `CX-1` and name the row or the sipx story instead, so the
      next reader is pointed at something that can still act.
- [ ] Whatever sipx declines is recorded in the ledger with the local plan, not dropped — `CX-1`'s
      third acceptance item, applied to these rows.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

## Notes

- Found by [`CF-16`](CF-16-sweep-for-done-stories-that-left-named-deltas-unlanded.md)'s sweep, as one
  of three instances of a `done` story named by another document as owner of a delta that never
  landed.
- **Sequencing.** `docs/upstream.md` is contended: the coordinator is filing a `sipx-transport`
  per-message-logging row from `DP-11` once `CX-4` releases the file. Coordinate rather than racing —
  three rows and one row in the same table is one edit, not two.
- Ordering against `RT-2`: this story unblocks it and does not depend on it. `RT-2` is where the
  splitter is *consumed*; nothing here waits for the trunk model.
- `CX-5`'s two open rows (the nonce mint, the replay window) are a different kind of finding —
  defects in released kernel code, found by reading it. These three are capability gaps found by
  writing specs against it. Same ledger, different provenance, and the rows should read differently.
- The pattern worth carrying forward: a spec that defers to a story should name a *ledger row* or an
  epic, not a story that may already be closed. Filing against a document that outlives any one story
  is the difference between a deferral and a dead letter.
