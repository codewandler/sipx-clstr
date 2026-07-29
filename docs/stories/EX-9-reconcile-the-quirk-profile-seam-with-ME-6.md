---
id: EX-9
title: Reconcile the quirk-profile media seam with the type ME-6 actually landed
pillar: Extensions
status: done
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
- [x] `MediaAssertion` is expressed in terms of `SrtpPolicy` — the enum
      [media-relay](../specs/media-relay.md) §13.1 actually landed, whose variants are `Disabled`,
      `Sdes { suites }` and `DtlsSrtp { role }`. There is no `SrtpMode` type and no `Required`
      variant anywhere in the repository, yet `Srtp(Required)` carries the shipped profile row and
      two vectors.
- [x] The required/optional axis the assertion needs is defined against `SrtpPolicy`, which has no
      such axis today, or the assertion is redefined so it does not need one.
- [x] The duplicated startup check is resolved to one rule with one error content. `G-M5`
      (media-relay) requires the error to name the trunk and the profile; `G11`
      (extension-framework) requires it to name the profile, the trunk, the assertion *and* the
      actual policy. `MR-C-3` and `QP-G-6` are the same check twice.
- [x] `extension-framework` cites `MP11` where it states the direction. It currently states the same
      rule without reference to the spec that made it normative.
- [x] A domain-bound profile carrying a `MediaAssertion` has defined behaviour. `G11` checks against
      "the bound **trunk's**" policy, and the grammar permits the carrying header set to be
      domain-bound.

## Progress

All five Acceptance items are discharged, entirely within
[extension-framework.md](../designs/extension-framework.md); `media-relay.md` was read for ground
truth and left unedited, per its status as the source of truth ME-6 landed.

- **Vocabulary.** `MediaAssertion` dropped the never-landed `SrtpMode`/`Required` pair. It is now
  `enum MediaAssertion { Srtp }`, checked against ME-6's actual `SrtpPolicy`
  (`Disabled` / `Sdes { suites }` / `DtlsSrtp { role }`).
- **The required/optional axis.** Argued rather than merely declared: `SrtpPolicy` has three keying
  mechanisms and no required/optional pair, so `Srtp` is defined as satisfied by *any variant but
  `Disabled`* — "this leg runs some SRTP mode." A mechanism-specific assertion (e.g. "must be
  `DtlsSrtp`") was considered and rejected: it would let a profile select the keying method by which
  profile matched, the exact per-call pattern MP6 forbids and the reason media-relay §13.6 gives for
  MP11 being constrain-not-assign — and it would reintroduce a precedence question between two
  profiles requiring different mechanisms, which media-relay's own `MR-C-4` vector (two SRTP
  requirements agreeing without a precedence rule) rules out. This also matches media-relay §13.6's
  own name for the field it left to EX-7 to spell: `requires_srtp`, i.e. boolean-shaped from the
  start.
- **The duplicated check.** Resolved to media-relay's `G-M5` as the normative statement. `G11`
  in extension-framework now states explicitly that it is the same check as `G-M5`/`MR-C-3`, and
  that **the error content is G-M5's** — naming the trunk and the profile only. The richer error
  (assertion + actual policy) was considered and rejected on the merits, not just for convenience:
  with `MediaAssertion` one variant wide, "the assertion" is now invariant across every violation
  (there is only one), so it adds nothing a reader does not already know from "which check failed";
  and "the actual policy" is exactly the trunk configuration the trunk name already points a reader
  at. No `media-relay.md` amendment story was filed, because there is no substantive gap left to
  amend — the two specs describe one rule with one error shape.
- **`MP11` citation.** Added at the point the direction is stated ("Media policy — a quirk asserts,
  and never configures"), and again in the "constrain-not-assign" argument for the required/optional
  mapping.
- **Domain-bound profiles.** Forbidden, not left undefined. `MediaAssertion` is checked against a
  `TrunkMediaPolicy`, which only a trunk binding has; a profile carrying a non-empty `requires_media`
  bound to a domain now fails the boot under a new rule, `G12`, naming the profile and the domain.
  Added vector `QP-G-11` for it, and extended the `sec-agree-headers` catalogue row's vector list
  path (`QP-G-6`/`QP-G-7`) to the new `SrtpPolicy` vocabulary.
- **Conditional framing removed.** The design no longer reads as if ME-6 might land SRTP as
  something other than a startup-readable per-trunk enum — it did, and the design now says so
  declaratively, citing media-relay §13.1's `TrunkMediaPolicy` field and §13.5's effective-policy
  record. The matching Risk-section entry (`requires_media is one enum wide`) was reworded from
  "presumes"/"if ME-6 makes it per-call" to record the settled shape and note that a future codec or
  transcode assertion would owe the same required/optional-axis argument fresh, not inherit this
  one.
- **Vector-table consistency.** `QP-G-6`/`QP-G-7` reworded for the new vocabulary and error content;
  `QP-G-11` added for the domain-bound case; the catalogue table's `sec-agree-headers` row and the
  "hands to the spec" §7/§9 lists updated to match (G12, `QP-G-11`).

**Deliberately not done:**
- No `docs/specs/media-relay.md` edit and no new story amending it — the review's "if you conclude
  the richer error is right, file it" branch wasn't taken; G-M5's content was judged sufficient once
  the assertion collapsed to one variant, so there's nothing to amend.
- The separable `overrides`-schema gap noted in this story's own Notes section (no schema, and
  self-inconsistent shape) is untouched — it's explicitly out of scope here and needs its own story
  per the note that filed it.
- No `docs/specs/registrar-auth.md` or `docs/specs/number-normalisation.md` changes — out of scope,
  and off limits as concurrent work by other agents in this tree.

**Gate:** `scripts/check-docs.sh` and `scripts/check-provenance.sh` run green (see below); the full
`cargo`-based gate was not run per this story's constraint (documentation-only change, no `cargo`
commands permitted to avoid contending for the target lock with concurrent agents in this tree).

## Notes
- Filed from the independent review of `EX-7`. The review checked the seam in both directions and
  found **no contradiction in substance** — `SdpOp::Set` is confined to `SessionName`, and `m=`
  proto, `a=crypto` and ICE are outside the vocabulary — so this is a naming and typing mismatch,
  not a design conflict.
- Separable, from the same review and worth its own story if this one grows: `overrides` is the only
  construct that resolves a contested target, is required by `G10`, asserted by `QP-G-3` and cited
  in Alternatives — and appears in no schema. Its shape is also self-inconsistent, given as a list
  of profile ids where the rule requires it to name a target.
