---
id: AF-3
title: Design the connection-owner RPC
pillar: Cluster
status: backlog
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: 
---

# Design the connection-owner RPC

## Goal
Design the one cross-node hop on the signalling path: delivering a request to the edge that owns the target client's connection.

## Acceptance
- [ ] Delivery semantics are specified: at-most-once, bounded queueing, and an explicit failure taxonomy (owner unreachable ≠ flow dead ≠ flow rejected — the future `430 Flow Failed` mapping).
- [ ] Node-to-node authentication and the transport choice are decided with rationale.
- [ ] The failure taxonomy is exercised as harness scenarios.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
