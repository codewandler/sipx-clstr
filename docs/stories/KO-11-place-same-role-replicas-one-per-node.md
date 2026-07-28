---
id: KO-11
title: Place same-role replicas one per node when they bind host ports
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: host-networked roles silently fail to schedule without it
---

# Place same-role replicas one per node when they bind host ports

## Goal
When a role binds host-global ports, ensure the operator places its replicas on distinct nodes, and says so plainly when it cannot.

## Acceptance
- [ ] A role that binds host ports gets required anti-affinity across nodes — not preferred.
- [ ] Replica counts that exceed the number of schedulable nodes surface as an explicit, readable status condition rather than as pods stuck Pending.
- [ ] The port arithmetic is validated at admission: two roles on one node may not claim the same port or overlapping media ranges.
- [ ] A failing-first test proves a replica count above the node count is reported rather than silently unscheduled.

## Progress
- (not started)

## Notes
- Every replica of a host-networked role binds the same node-global port, so two replicas cannot share a node. This bounds a role's replica count by the node count — a capacity fact operators need stated, not discovered.
- It also bounds what a single-node local environment can demonstrate.
- Filed from a downstream deployment of this platform (its ledger entry **U-16**).
