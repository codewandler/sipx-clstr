---
id: AF-2
title: Specify flow references and connection ownership
pillar: Cluster
status: backlog
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, transport]
note: 
---

# Specify flow references and connection ownership

## Goal
Specify the connection table and the `flow_ref = signed(node_id, connection_id, generation)` reference stored in location bindings — a client's connection has exactly one owner.

## Acceptance
- [ ] The connection-table schema is specified: transport, remote address, authenticated identity, TLS info, flow generation, last activity.
- [ ] flow_ref format and generation-bump rules (reconnect invalidates old references) are specified with vectors.
- [ ] Binding integration is defined with RG-1 so a lookup yields the owner to RPC.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
