---
id: EX-10
title: Give `overrides` a schema, or remove it from the composition rule
pillar: Extensions
status: ready
priority: 2
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: found reviewing EX-7 — the one construct that resolves a contested target is in no schema
---

# Give `overrides` a schema, or remove it from the composition rule

## Goal
Make `overrides` expressible. It is the only construct that resolves a contested target when two
quirk profiles both write the same thing, and it exists in the rules and the vectors but in no
schema — so the one part of the composition story that needs data to be checkable is the part that
cannot be written down.

## Acceptance
- [ ] `overrides` appears in the `QuirkProfile` schema, or the rules and vectors that depend on it
      are removed and the contested-target case is resolved some other way.
- [ ] Its shape is self-consistent. The design currently gives
      `overrides = ["<domain-bound-profile-id>"]` — a list of profile ids — while the same sentence
      and rule `G10` both require it to name a **target**. One of those is wrong.
- [ ] Whichever way it resolves, at least one TOML binding example in the design carries the key,
      since both existing examples omit it entirely.
- [ ] `G10` and the vector that asserts it agree with the schema, and with each other.

## Progress
- (not started)

## Notes
- Filed from the independent review of `EX-7`, and deliberately separated from
  [EX-9](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/EX-9-reconcile-the-quirk-profile-seam-with-ME-6.md),
  which reconciled the *media* seam and left this untouched on purpose — they are independent
  defects in the same design and conflating them would have made one diff answer two questions.
- The design's own thesis is that a quirk is bounded because the composition rule is **checkable**.
  A contradiction-resolution rule that cannot be expressed in the specified data is the sharpest
  possible counter-example to that claim, which is why this is worth its own story rather than a
  footnote.
