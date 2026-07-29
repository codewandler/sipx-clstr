---
id: EX-9
title: Reconcile the quirk-profile media seam with the type ME-6 actually landed
pillar: Extensions
status: ready
priority: 1
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions, media]
note: found reviewing EX-7 — it is written against SrtpMode; ME-6 landed SrtpPolicy
---

# Reconcile the quirk-profile media seam with the type ME-6 actually landed

## Goal
Make [extension-framework](../designs/extension-framework.md) §on media assertions refer to the
type that exists. `EX-7` and `ME-6` were written concurrently in the same implementation wave and
agree on the *direction* of their seam — a profile may **require** an SRTP mode and may never
**assign** one — but not on the vocabulary, so the seam cannot presently be implemented as written.

## Acceptance
- [ ] `MediaAssertion` is expressed in terms of `SrtpPolicy` — the enum
      [media-relay](../specs/media-relay.md) §13.1 actually landed, whose variants are `Disabled`,
      `Sdes { suites }` and `DtlsSrtp { role }`. There is no `SrtpMode` type and no `Required`
      variant anywhere in the repository, yet `Srtp(Required)` carries the shipped profile row and
      two vectors.
- [ ] The required/optional axis the assertion needs is defined against `SrtpPolicy`, which has no
      such axis today, or the assertion is redefined so it does not need one.
- [ ] The duplicated startup check is resolved to one rule with one error content. `G-M5`
      (media-relay) requires the error to name the trunk and the profile; `G11`
      (extension-framework) requires it to name the profile, the trunk, the assertion *and* the
      actual policy. `MR-C-3` and `QP-G-6` are the same check twice.
- [ ] `extension-framework` cites `MP11` where it states the direction. It currently states the same
      rule without reference to the spec that made it normative.
- [ ] A domain-bound profile carrying a `MediaAssertion` has defined behaviour. `G11` checks against
      "the bound **trunk's**" policy, and the grammar permits the carrying header set to be
      domain-bound.

## Progress
- (not started)

## Notes
- Filed from the independent review of `EX-7`. The review checked the seam in both directions and
  found **no contradiction in substance** — `SdpOp::Set` is confined to `SessionName`, and `m=`
  proto, `a=crypto` and ICE are outside the vocabulary — so this is a naming and typing mismatch,
  not a design conflict.
- Separable, from the same review and worth its own story if this one grows: `overrides` is the only
  construct that resolves a contested target, is required by `G10`, asserted by `QP-G-3` and cited
  in Alternatives — and appears in no schema. Its shape is also self-inconsistent, given as a list
  of profile ids where the rule requires it to name a target.
