---
id: RT-1
title: Design the RoutePlan and shared-cache resolver
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, dns]
note: UPSTREAM option — see docs/upstream.md
---

# Design the RoutePlan and shared-cache resolver

## Goal
Design the `RoutePlan` the proxy driver consumes and the resolver that produces it at proxy throughput: async, shared, TTL- and negative-caching.

## Acceptance
- [ ] The RoutePlan type (attempt list: transport, address, source, priority, weight) and its consumption contract are designed.
- [ ] The resolver design keeps every await off the signalling loop and covers the WS/WSS SRV prefixes the kernel's prefetch misses.
- [ ] The upstream-vs-local decision for the async resolver is made and recorded in the upstream ledger.

## Progress
- (not started)

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md).
