---
id: RG-22
title: Complete GRUU and Outbound reachability through the cluster
pillar: Signalling
status: backlog
priority: 12
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, affinity, transport]
note: M3/M4 · blocked by AF-4 through AF-7, RG-5 and the released kernel GRUU/Outbound surface
---

# Complete GRUU and Outbound reachability through the cluster

## Goal

Reach one registered instance and connection flow through any healthy edge without a global dialog
lookup.

## Acceptance

- [ ] Registration stores instance/reg-id, Path and flow reference and returns public/temporary GRUUs
      with the RFC 5626/5627 lifetimes specified in the location-service spec.
- [ ] A GRUU selects exactly its instance; an expired or invalid temporary GRUU cannot broaden to the
      whole AoR.
- [ ] Requests for a connection-bound contact route through the affinity token and owner RPC; a dead
      owner produces the specified retry/failure, never a silent UDP fallback.
- [ ] A WSS client behind NAT registers through one edge and is called through another with zero
      global dialog lookups.
- [ ] Generic GRUU/Outbound syntax stays in sipx; this story owns registrar and cluster orchestration.
- [ ] Failing-first `OB-5`/`OB-6` scenarios cover the instance and cross-edge flow paths.

## Progress

- Not started.
