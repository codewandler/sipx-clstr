---
id: CX-6
title: File the ledger rows CX-1 was named for and never filed
pillar: Platform
status: in-progress
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

- [x] Each of the three gaps is a row in `docs/upstream.md` in the established shape — gap, what sipx
      has today (with the file and line at the pinned tag), sipx story or `— (not yet filed)`, what it
      blocks here, and status. Read the kernel before writing the row: `CX-1`'s own lesson was that a
      ledger row can be wrong, and two of its six were. → **five** rows, not three: the three spec
      paragraphs name five distinct kernel changes between them, and each was read in the kernel at
      `v0.10.0` (`f9104b78`) before its row was written. See `## Progress`.
- [x] Each row is decided rather than assumed, per rule 6 and `CX-1`'s precedent with `T-17`. In
      particular the RFC 5393 branch cookie may well be **declined** — `proxy-behavior` §1 already
      says it is implemented here behind a movable seam — and "decided local, with the reason" is a
      valid close for any of the three. → four decided **upstream** (open, unfiled), the branch
      cookie **declined**.
- [x] The two RFC 3966 asks are reconciled into one row or explicitly kept as two. `asserted-identity`
      wants the split for reading a `tel:` URI's number; `number-normalisation` wants it in **public**
      API for lossless rewriting. If one kernel change serves both, one row serves both. → **one
      row**, *A public RFC 3966 split of a `tel:` URI*; both specs' paragraphs say so and say why.
- [x] The three spec paragraphs stop naming `CX-1` and name the row or the sipx story instead, so the
      next reader is pointed at something that can still act. → each now names its rows and links
      `../upstream.md`, and `scripts/check-docs.py` fails the gate if a spec regresses to either
      half of the old shape.
- [x] Whatever sipx declines is recorded in the ledger with the local plan, not dropped — `CX-1`'s
      third acceptance item, applied to these rows. → the branch-cookie row carries the reason and
      the local plan: `crates/sipx-clstr-proxy/src/cookie.rs`, three exported names, one reader.
- [x] `scripts/gate.sh` green.

## Progress

- **Five rows, not three, and the arithmetic is the finding.** The Goal counts three *gaps* because
  three spec paragraphs made the promise. Those paragraphs name five distinct kernel changes: each of
  `asserted-identity` §2 and `number-normalisation` §1 says "**Both** are candidate ledger rows", and
  `proxy-behavior` §1 names two of which one (`Headers` surgery, `S-15`) was already ledgered. Filing
  three would have left two halves of two "both"s pointing at nothing — the same dead letter this
  story exists to clear, one indirection further down. So: *A public RFC 3966 split of a `tel:` URI*
  (one row for two asks), *Replacing a URI's user part*, *A typed `P-Asserted-Identity` /
  `P-Preferred-Identity` value list*, *A parsed `Privacy` header*, *The RFC 5393 loop-detection
  branch cookie*.
- **The Goal's reading of `asserted-identity` §2 is not what §2 says, and the row follows the spec.**
  The Goal describes that paragraph's ask as RFC 3966 `tel:` splitting plus the one-or-two rule. §2's
  two bullets are in fact a parsed `Privacy` header and a typed PAI/PPI value list; the word "3966"
  does not appear in that spec. The `tel:` split reaches it indirectly and demonstrably — §6.1 is the
  seam to `number-normalisation`, whose `p_asserted_identity` is one of four normalised fields, and
  whose `NN-X-6`/`NN-X-9` vectors are `tel` values inside a PAI. That is the evidence for filing one
  `tel:` row serving both specs rather than two.
- **Every citation was read at the pin, not inherited.** `v0.10.0` resolves to `f9104b78` (`Cargo.lock`),
  checked out under `~/.cargo/git/checkouts/sipx-65343d777b6000c5/f9104b7`. The most useful thing the
  reading changed: the RFC 3966 splitter **already exists** in the kernel and is merely private
  (`split_tel_body`, `uri.rs:543`, reached only from `tel_equivalent` at `:527`), so the row asks for
  `pub` rather than for an algorithm — which is a different-sized story upstream. Likewise the
  user-part setter lands on a seam the kernel already maintains (`sip_parts_mut` clears `raw`,
  `uri.rs:274`).
- **The branch cookie is declined, on three grounds** — keyed with the cluster key family (key custody
  is orchestration), inputs are this forwarding engine's routing state (and the ledger already put the
  proxy transaction driver here), and no kernel caller exists (no proxy, no `Max-Breadth`, no loop
  detection). Local plan recorded in the row.
- **A gate check now holds the pattern, not just this instance** (`scripts/check-docs.py`,
  `check_spec_deferrals`): a paragraph under `docs/specs/` that says *ledger row* must link
  `docs/upstream.md`, and no spec paragraph may name a story as the thing that will file or raise a
  row. At the merge base it reported exactly the three offending paragraphs and nothing else.
- **Not done here, deliberately:** the sipx stories themselves. Four rows read `— (not yet filed)`,
  which is this ledger's established state for a row whose upstream story does not exist yet
  (`CX-5`, `RG-15`, `DP-11` all sit there). Filing them is work in the kernel repository.

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
