---
id: PX-6
title: Implement CANCEL and Timer C
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: blocked by PX-5
---

# Implement CANCEL and Timer C

## Goal
Implement CANCEL propagation and Timer C: upstream CANCEL fans out to open branches, a better final response cancels the rest, and silent INVITE branches are reaped.

## Acceptance
- [ ] Upstream CANCEL propagates to all open branches; branches cancelled before a final response produce `487` per the spec vectors (after a final response, CANCEL is a no-op per §9.2).
- [ ] A first 2xx cancels remaining branches; Timer C expiry cancels a silent branch.
- [ ] All CANCEL/Timer C vectors pass under adversarial schedules in the harness.

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
