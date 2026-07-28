---
id: ME-6
title: Specify per-trunk codec and SRTP policy
pillar: Media
status: backlog
priority: 
design: docs/designs/media-control.md
epic: media-control
areas: [media, trunks]
note: 
---

# Specify per-trunk codec and SRTP policy

## Goal
Make codec handling and SRTP selection a declared per-trunk policy rather than a consequence of which branch of the routing logic a call took.

## Acceptance
- [ ] A trunk declares its offered codecs, any transcoding, and its SRTP mode.
- [ ] SRTP selection is per trunk, not a global domain pattern.
- [ ] The default is explicit: no transcoding unless declared.
- [ ] A test asserts the offer sent to a trunk matches its declared policy.

## Progress
- (not started)

## Notes
- babelforce transcodes to a specific codec on one branch of a NAT test and not the other — the policy is an accident of control flow. SRTP is selected by a global domain regex.
- Neither is reviewable, and neither can be changed per carrier without editing routing logic.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-12** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
