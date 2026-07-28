---
id: RT-7
title: Specify per-trunk asserted identity and privacy policy
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks, privacy]
note: 
---

# Specify per-trunk asserted identity and privacy policy

## Goal
Make P-Asserted-Identity synthesis and privacy handling a per-trunk policy, since what a carrier requires differs per carrier and is a compliance obligation.

## Acceptance
- [ ] A trunk declares whether PAI is asserted, what identity is used, and what happens for an anonymous caller.
- [ ] `Privacy` handling for anonymous callers is declared, including a fallback identity when none is available.
- [ ] Interaction with the anonymous-From rule (RFC 5379 §5.1.4) is specified, not left to ordering.
- [ ] Test vectors per policy combination.

## Progress
- (not started)

## Notes
- One deployment synthesises PAI on egress, adds `Privacy: id` for anonymous callers, and falls back to a fixed number when no identity is available.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-7**). The evidence sits in that deployment's own reference material.
