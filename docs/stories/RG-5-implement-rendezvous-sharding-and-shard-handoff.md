---
id: RG-5
title: Implement rendezvous sharding and shard handoff
pillar: Signalling
status: backlog
priority: 
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [location, affinity]
note: 
---

# Implement rendezvous sharding and shard handoff

## Goal
Shard AoRs by rendezvous hashing over `tenant || canonical AoR` and make shard-map changes safe: drain, then switch.

## Acceptance
- [ ] A distribution test shows acceptable balance across shard counts.
- [ ] Shard-map reload follows drain-then-switch; in-flight REGISTER writes are never split across two owners — including the rolling-reload window where nodes hold old and new maps concurrently (harness scenario).

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
