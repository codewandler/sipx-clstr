---
id: EX-11
title: Derive when trunk-bound and domain-bound rule sets actually intersect
pillar: Extensions
status: in-progress
priority: 2
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: found reviewing EX-7, unresolved by EX-9 and EX-10 — asserted, never derived
---

# Derive when trunk-bound and domain-bound rule sets actually intersect

## Goal
Say when a trunk-bound profile and a domain-bound profile ever apply to the **same message**. The
design states that the composed set is "every trunk-bound and domain-bound profile's rules
together" and `QP-C-2` asserts both apply to one message — but under the design's own definitions a
trunk is an egress peer and a domain is the registering side, so egress toward a trunk and egress
toward a registered contact are different messages. Nothing in the text explains when the two sets
meet.

## Acceptance
- [x] The condition under which a trunk-bound and a domain-bound profile both apply to one message
      is **derived** from the definitions of trunk and domain, not asserted.
      → [extension-framework](../designs/extension-framework.md) § *Whether the two attachment
      points ever compose*: four premises, each cited to the document that already fixes it
      (`P1` §*Where it runs*, `P2` proxy-behavior §2/§7 F2 + RT-1 + location-service §5.1 S1,
      `P3` RT-8, `P4` the `B3` bound); a **closed six-row enumeration** of ingress kind × egress
      kind; and the condition stated positively — the two sets meet **iff one leg's peer is
      identified by a `trunk[]` and a `domain[]` entry at once**, which `P2` makes impossible on an
      egress leg and `P3` on the ingress leg.
- [x] `QP-C-2` either follows from that derivation, or is corrected. If the sets never intersect,
      say so — and then the "union" sentence is what needs changing.
      → **Corrected, both of them.** The sets never intersect: the assertion was wrong, not merely
      unproved. The union bullet in § *Binding* now reads "every profile bound to **that one
      object**", and `QP-C-2` becomes the two-leg case that is actually reachable (enumeration row
      4) — the trunk-bound profile writes the forwarded request at H9, the domain-bound one the
      forwarded response at H11, two messages and two composed sets.
- [x] `ValueLeaf::TrunkConfig` in a profile bound to a **domain** — and the mirror case — has a
      stated resolution. `G10`'s closed-world clause requires every `ConfigKey` to type-check but
      does not say against which side, so a domain-bound profile referencing trunk configuration is
      currently well-formed and unresolvable.
      → New rule **`G14`**: a rule's config leaves name the side its binding is on, checked per
      *binding* rather than per profile (`G12`'s shape, and for the same reason). Rejected at
      startup, because a leg evaluates exactly one attachment object and so the leaf is
      unresolvable at *every* evaluation rather than at some. `G10`'s closed-world clause now names
      `G14`, and the alternative — read through to the other leg's trunk — is refused with the
      reason: it makes a domain-bound rule a function of the route (`B3`) and destroys confluence.
- [x] A vector covers the intersecting case if one exists, and the rejection if it does not.
      → No intersecting case exists, so the rows cover both halves of that: `QP-C-4` is the
      configuration the union reading rejects and the derivation accepts (the row that fails if the
      derived condition is violated), `QP-G-17` the override that the union reading would have made
      the *repair* for it, `QP-G-16` the `G14` rejection, and `QP-G-18` the three-way contest
      `EX-10` left unpinned. Enforcement is unchanged and stated in the design: `QP` is **not** a
      registered prefix, because `check-vectors.py` reads rows only out of the spec that owns a
      prefix and the quirk vectors live in a design — demonstrated, not assumed, by putting a
      fabricated `QP-Z-999` row in the table and watching `--check` exit 0.

## Progress
- **Landed 2026-07-30.** The finding is confirmed and the assertion is **wrong**, which was the most
  valuable of the available outcomes: `QP-C-2` asserted an outcome for a configuration that cannot
  arise, and the union sentence described a composed set that never exists.
- The derivation lives in the design, not in a spec, because the design is what owns the claim:
  `docs/specs/hook-framework.md` contains no quirk section at all — no bindings, no trunks, no
  domains, no `G9`–`G13` — so there was no normative text to correct. Checked rather than assumed
  (`grep quirk docs/specs/hook-framework.md` is empty).
- Where the derivation could have been a decision it is not: the location-service half of H9's
  binding is *forced*, because the served domain of the address-of-record is the only configuration
  object identifying that peer, and the alternative would make `domain[].quirks` unreachable on the
  request path — contradicting the design's own binding example.
- One premise is **flagged rather than chosen**, per the story's own posture: `P3`, the
  single-valuedness of the ingress classification, is RT-8's vocabulary. A peer admitted by source
  address as a trunk that also registers in a served domain is the sole route back to a
  cross-attachment contest. The *Risks* entry now carries the three ways RT-8 could close it and
  what each costs, and picks none.
- Two things this derivation *strengthens* rather than changes: `EX-10`'s rejection of a directional
  trunk-beats-domain escape (it was too narrow; it is now a precedence rule over an empty subject),
  and [cluster-config](../specs/cluster-config.md) §7 S4 / §8 V5, which had already encoded the
  per-binding model before it was derived — so nothing outside the design needed editing.
- Considered for upstream: no. Quirk-profile binding is per-peer deployment policy over this
  platform's `trunk[]`/`domain[]` objects and its hook phases; the kernel has no trunks, no domains
  and no hook pipeline for a composition rule to be about. This story adds no type and no code, only
  the argument for a rule that already lives here.

## Notes
- Filed from the independent review of `EX-7` and left open on purpose by both `EX-9` and `EX-10`.
  `EX-10` in particular **removed the escape's dependence** on this question — `quirk_overrides`
  resolves a contest at one attachment point whether or not the two sets ever meet — so this is no
  longer load-bearing for the composition rule. That is what makes it a separate story rather than
  a blocker.
- It still matters for `QP-C-2`, which asserts an outcome for a case nobody has shown can arise. A
  vector asserting behaviour in an unreachable configuration is worse than no vector: it will be
  implemented, and the implementation will never be exercised.
- Related, from `EX-10`'s own risk list and worth folding in if it fits: `winner` is a single
  profile id, and no vector pins a **three-way** contest, where the entry names one winner and the
  other two lose.
  → Folded in: `QP-G-18`, and `G13` now states the *n*-way rule explicitly — one entry names one
  winner, and every other contesting profile's rule for that target is dropped — so a third
  contestant needs no second entry.
- Found while deriving, and **not fixed here**: the `EX-7` spec deltas — the `SyntaxDecl`
  `Replace`/`Field` split and `G9` … `G14` — are named for `EX-8` in the design, but `EX-8`'s
  acceptance covered `EX-6`'s half only and `EX-8` is closed. `media_types_rewritten` is still a flat
  list in hook-framework §6, so those deltas are owned by nobody, and the `QP` prefix cannot be
  registered in the vector gate until they land. Recorded in the design; it needs a story, which this
  one deliberately did not create from inside a design.
