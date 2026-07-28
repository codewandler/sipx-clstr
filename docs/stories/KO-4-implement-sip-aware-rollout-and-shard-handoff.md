---
id: KO-4
title: Implement SIP-aware rollout, draining and key rotation
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy, affinity]
note: blocked by KO-3; the reason an operator exists
---

# Implement SIP-aware rollout, draining and key rotation

## Goal
Make the lifecycle events that a template cannot express safe: rolling an edge, resizing registrar shards, and rotating token keys, without dropping registrations or breaking connection ownership.

## Acceptance
- [ ] Edge rollout drains before terminating: the pod stops accepting new registrations and calls, clients re-register elsewhere, a bounded drain window is honoured, and only then is the pod terminated.
- [ ] Registrar replica changes execute drain-then-switch on the shard map per DP-1 — no silent rehash; `ShardMapConverged` reflects the real state throughout.
- [ ] Token key rotation is two-phase (distribute, then activate): a failing-first test shows verification never precedes distribution, and mid-rotation traffic verifies under both keys.
- [ ] Media nodes are never removed while sessions are anchored on them.
- [ ] A rollout of the whole cluster on local k3s keeps registrations alive and new calls succeeding throughout, asserted by the `e2e-tester` probe running during the rollout.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Ownership semantics: [cluster-affinity](../designs/cluster-affinity.md); key distribution: [AF-6](AF-6-design-config-first-membership-and-key-distribution.md); shard handoff: [RG-5](RG-5-implement-rendezvous-sharding-and-shard-handoff.md).
