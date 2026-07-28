---
id: RT-5
title: Implement a per-trunk egress header allowlist
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks, security]
note: confidentiality boundary; blocks babelforce M2
---

# Implement a per-trunk egress header allowlist

## Goal
Let a trunk declare which non-standard headers may leave the platform, so application context can be carried to some peers and withheld from others as configuration rather than code.

## Acceptance
- [ ] A trunk declares `pass_headers`; every header matching the platform's application-header prefix that is not listed is removed on egress.
- [ ] The default is deny — an unlisted header never leaves.
- [ ] A failing-first test proves a confidential header reaches an allow-listed peer and does not reach a peer without it.
- [ ] The allowlist is per trunk, not global, and appears in the config schema (DP-1).

## Progress
- (not started)

## Notes
- babelforce strips every `X-Babelforce-*` header towards carriers except two, which survive towards one specific carrier. Today that is two hand-written regexes in the routing script.
- This is a confidentiality boundary, so it must be enforced by test rather than by review.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-4** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
