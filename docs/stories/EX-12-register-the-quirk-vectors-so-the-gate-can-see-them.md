---
id: EX-12
title: Register the quirk vectors so the gate can see them, and land the EX-7 spec deltas nobody owns
pillar: Platform
status: ready
priority: 2
epic:
areas: [extensions, ci, docs]
note: the QP rows are decorative — a fabricated row passes --check, because no spec owns the prefix
---

# Register the quirk vectors so the gate can see them, and land the EX-7 spec deltas nobody owns

## Goal

The quirk-profile vector rows (`QP-*`) are **not enforced by anything**. `scripts/check-vectors.py`
reads rows only out of the spec that *owns* a prefix (`SPECS`, and the comment at :50 states the rule
explicitly), and `QP` is owned by no spec — the rows live in `docs/designs/extension-framework.md`, a
design record. `EX-11` proved this rather than assumed it: it temporarily inserted a fabricated
`QP-Z-999` row and `python3 scripts/check-vectors.py --check` exited **0**.

So every `QP` row added so far — including the four `EX-11` wrote to pin its own derivation — is
prose that looks like a measurement. `CF-8` made the conformance headline honest by counting *proved*
rows rather than deriving the number; this is the same class of defect one level down, where the rows
themselves are outside the instrument.

The reason they cannot simply be moved is the second half of this story: registering `QP` means
putting quirk bindings into `docs/specs/hook-framework.md` §7/§9, and **the `EX-7` spec deltas are
owned by nobody**. The extension design names `EX-8` for the `SyntaxDecl` `Replace`/`Field` split and
rules `G9`–`G14`, but `EX-8`'s acceptance covered `EX-6`'s half only and `EX-8` is `done`.
`media_types_rewritten` is still a flat `&'static [MediaType]` at `hook-framework.md:332`. One
consequence is concrete: a deployment cannot both anchor media and run an SDP quirk.

## Acceptance

- [ ] **Failing-first**: a fabricated `QP` row is added and `scripts/check-vectors.py --check` is
      shown to **exit 0** at the merge base and to **fail** after. That inverted proof is the whole
      story — the current instrument cannot tell a real row from an invented one.
- [ ] `QP` is owned by exactly one spec and appears in `SPECS`, or the `QP` rows are removed from the
      design and re-expressed under a prefix that is already owned. Rows that no gate reads must not
      remain while looking like vectors.
- [ ] The `EX-7` spec deltas land in `docs/specs/hook-framework.md`: the `SyntaxDecl`
      `Replace`/`Field` split, `media_types_rewritten` given the shape that lets media anchoring and
      an SDP quirk coexist, and rules `G9`–`G14` stated normatively rather than only in the design.
- [ ] Every `QP` row now visible to the gate is either proved by a test or carries a deferral naming
      real work. A row that becomes visible and is silently waived has not been registered, it has
      been hidden somewhere new.
- [ ] `docs/stories/EX-7-*.md:21` no longer describes the composition as a "disjoint union over the
      composed set" — `EX-11` derived that the sets never intersect, so the sentence is false.
- [ ] `scripts/gate.sh` green, and `check-vectors.py` reports a row count that went **up**.

## Progress

- (running log)

## Notes

- Filed by the coordinator at `EX-11`'s integration. `EX-11` disclosed this about its **own** output
  rather than letting the rows pass as proved, which is the only reason it was caught; the fabricated
  `QP-Z-999` was removed before it committed (`grep -c QP-Z-999` → 0).
- Do not close this by adding `QP` to `SPECS` pointing at a design record. `SPECS` maps a prefix to a
  *spec*; aiming it at `docs/designs/` would make the design normative by accident and break the
  two-tree split `AGENTS.md` states.
- The ordering is the interesting part: the vector registration is blocked by the spec deltas, and
  the spec deltas were orphaned because a story closed on half its scope. Worth a look at whether any
  other `done` story left a named delta behind.
