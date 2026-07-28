---
id: RT-2
title: Implement the trunk model with breakers and CPS limits
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing]
note: 
---

# Implement the trunk model with breakers and CPS limits

## Goal
Implement trunks as stateful objects: circuit breakers fed by real transaction outcomes, calls-per-second and concurrency caps, retry budgets.

## Acceptance
- [ ] Breaker transitions are driven by response-code/timeout history and are observable as metrics (DP-3).
- [ ] CPS and concurrency enforcement rejects with the specified responses under load in the harness.
- [ ] Trunk configuration versions are visible to the token's policy-version field.

## Progress
- (not started)

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md).
