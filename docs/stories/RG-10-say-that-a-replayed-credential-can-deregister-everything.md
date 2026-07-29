---
id: RG-10
title: Say that a replayed credential can de-register every binding
pillar: Signalling
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: found reviewing RG-9 — §7.2 understates the exposure it exists to state
---

# Say that a replayed credential can de-register every binding

## Goal
Correct [registrar-auth](../specs/registrar-auth.md) §7.2, which describes the accepted digest
exposure as strictly **additive** — the attacker's contact joins the victim's, "the phone that owns
the AoR keeps ringing, so nothing about the victim's experience says this happened". For one variant
that sentence is false, and §7 is the one section whose stated purpose is not to understate.

## Acceptance
- [ ] §7.2 names the removal case: the digest binds neither `Contact` nor `Expires` nor `Call-ID`,
      and this registrar implements wildcard removal
      ([location-service](../specs/location-service.md) §W3, "`Call-ID` differs → remove"), so the
      same replayed `Authorization` on a REGISTER carrying `Contact: *`, `Expires: 0` and a fresh
      `Call-ID` removes **every** binding for the AoR.
- [ ] The operator-facing residual-risk sentence in §7.3 covers loss of service, not only forking.
      It currently describes only "have the AoR fork to them".
- [ ] RFC 3261 §26.1.1 is cited for what it actually says — *"could, for example, de-register all
      existing contacts for a URI and then register their own device"* — which §7 already cites for
      the weaker claim.
- [ ] A vector pins the removal variant, as `RA-R-6` pins the additive one.
- [ ] The decision is re-affirmed or revisited **in light of the corrected impact**. Accepting a
      quiet fork and accepting total de-registration are not the same decision, and `RG-9` only
      argued the first.

## Progress
- (not started)

## Notes
- Filed from the independent review of `RG-9`. That story's technical work was verified correct
  against the pinned kernel (`v0.7.0`, `67c21039`) and every RFC citation in its new §7 resolves;
  this is about the impact statement, not the mechanism.
- Separable, from the same review: §7.3 rejects the registrar-side check in its **weakest** form
  only — "remember the `Contact` first seen under a nonce and refuse a change". The narrower variant
  (accept an equal-`nc` repeat only when the binding fields are also equal, i.e. tell a byte-identical
  retransmission from a reattach) is not considered, and neither stated reason applies to it: RFC
  7616 §3.4 makes `nc` increment per request, so a client legitimately changing its `Contact`
  arrives on a *higher* `nc` and never takes the equal-`nc` branch. The accept decision may still be
  right — the accept branch is kernel-side, so narrowing it is an upstream conversation — but the
  table reads as though the option space were exhausted.
