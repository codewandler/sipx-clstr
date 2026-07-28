---
id: RG-4
title: Implement the PostgreSQL LocationStore backend
pillar: Signalling
status: backlog
priority: 
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [location]
note: blocked by RG-3
---

# Implement the PostgreSQL LocationStore backend

## Goal
Implement the first production `LocationStore`: serializable per-AoR transactions on PostgreSQL with LISTEN/NOTIFY as the change stream.

## Acceptance
- [ ] The backend passes the identical contract-test suite as the in-memory store.
- [ ] Missed change notifications are tolerated: the read path bounds staleness by TTL.
- [ ] A registration-storm load model (mass re-REGISTER after an outage) is documented with measured headroom, and refresh writes are coalesced if the model demands it.

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
