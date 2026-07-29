---
id: EX-11
title: Derive when trunk-bound and domain-bound rule sets actually intersect
pillar: Extensions
status: ready
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
- [ ] The condition under which a trunk-bound and a domain-bound profile both apply to one message
      is **derived** from the definitions of trunk and domain, not asserted.
- [ ] `QP-C-2` either follows from that derivation, or is corrected. If the sets never intersect,
      say so — and then the "union" sentence is what needs changing.
- [ ] `ValueLeaf::TrunkConfig` in a profile bound to a **domain** — and the mirror case — has a
      stated resolution. `G10`'s closed-world clause requires every `ConfigKey` to type-check but
      does not say against which side, so a domain-bound profile referencing trunk configuration is
      currently well-formed and unresolvable.
- [ ] A vector covers the intersecting case if one exists, and the rejection if it does not.

## Progress
- (not started)

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
