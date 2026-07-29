---
id: EX-10
title: Give `overrides` a schema, or remove it from the composition rule
pillar: Extensions
status: in-progress
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
- [x] `overrides` appears in the `QuirkProfile` schema, or the rules and vectors that depend on it
      are removed and the contested-target case is resolved some other way.
      → Second branch. `QuirkProfile` gains no field; the profile-level, trunk-over-domain
      `overrides` is gone from the binding rules, from `G10` and from `QP-G-3`, and the contested
      target is resolved by a **binding-level** override entry (`quirk_overrides`) under new rule
      `G13`.
- [x] Its shape is self-consistent. The design currently gives
      `overrides = ["<domain-bound-profile-id>"]` — a list of profile ids — while the same sentence
      and rule `G10` both require it to name a **target**. One of those is wrong.
      → Both readings were wrong at once, and the entry now carries the two separately:
      `target` (what is won) and `winner` (who wins). A *target* is also newly defined as
      elementary — one catalogue row at one message class — so an override can name one.
- [x] Whichever way it resolves, at least one TOML binding example in the design carries the key,
      since both existing examples omit it entirely.
      → The `#### Binding` example carries a `[[trunks.trunk-b.quirk_overrides]]` entry, with the
      SDP form of `target` given beneath it.
- [x] `G10` and the vector that asserts it agree with the schema, and with each other.
      → `G10` now checks disjointness over elementary targets *after* `G13`'s overrides apply;
      `QP-G-2`/`QP-G-3` are rewritten to the binding-level form, and `QP-G-12` … `QP-G-15` cover
      the same-attachment contest, the override that outlived its contest, a `winner` that is not
      contesting the target, and the fact that an override never suppresses `G11`.

## Progress
- Resolved by **relocation, not deletion**: the escape survives, the profile-level form does not.
  A contest is a property of the composition at one attachment point, which only a binding knows;
  a profile that names another profile knows where it applies, which the design's own rule forbids,
  and a shipped versioned catalogue profile cannot name one deployment's other profiles at all.
- New rule `G13` and vectors `QP-G-12` … `QP-G-15`; `G10`, `QP-G-2`, `QP-G-3`, the confluence
  bullet, the *Alternatives* precedence bullet and the spec-delta list all moved with it.
- Interaction with `G12` stated explicitly and made structural: an override deletes *rules*, and
  `requires_media` is an assertion with no target, so no override can suppress `G11` or `G12`
  (`QP-G-15`).
- Left open deliberately: when a trunk-bound and a domain-bound rule set actually intersect. This
  answer does not depend on it either way; the union sentence and `QP-C-2` still owe a derivation,
  and a *Risks* entry now says so. Worth its own story.

## Notes
- Filed from the independent review of `EX-7`, and deliberately separated from
  [EX-9](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/EX-9-reconcile-the-quirk-profile-seam-with-ME-6.md),
  which reconciled the *media* seam and left this untouched on purpose — they are independent
  defects in the same design and conflating them would have made one diff answer two questions.
- The design's own thesis is that a quirk is bounded because the composition rule is **checkable**.
  A contradiction-resolution rule that cannot be expressed in the specified data is the sharpest
  possible counter-example to that claim, which is why this is worth its own story rather than a
  footnote.
