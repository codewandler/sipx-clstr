---
id: RT-2
title: Implement the trunk model with breakers and CPS limits
pillar: Signalling
status: ready
priority: 3
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing]
note: unblocked — RT-1 and AF-1 are done, and RT-10 closed the totality holes filed against its inputs; M2 #9
---

# Implement the trunk model with breakers and CPS limits

## Goal
Implement trunks as stateful objects: circuit breakers fed by real transaction outcomes, calls-per-second and concurrency caps, retry budgets.

## Acceptance
- [ ] Breaker transitions are driven by response-code/timeout history and are observable as metrics (DP-3).
- [ ] CPS and concurrency enforcement rejects with the specified responses under load in the harness.
- [ ] Trunk configuration carries a version; its interaction with the token's policy-version 
field is settled against AF-1 before this story closes.

## Progress
- (not started)

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md).
