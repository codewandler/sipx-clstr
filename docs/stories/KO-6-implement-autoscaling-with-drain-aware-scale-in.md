---
id: KO-6
title: Implement autoscaling with drain-aware scale-in
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: phase 2; blocked by KO-4, KO-5
---

# Implement autoscaling with drain-aware scale-in

## Goal
Let the cluster resize itself from the KO-5 signals — scaling out freely, scaling in only through the drain path, and not at all when the guardrails say the cluster is wrong rather than busy.

## Acceptance
- [ ] Scale-out raises replicas from Prometheus-derived signals within the configured bounds and is demonstrated under a load scenario (registration storm, CPS ramp).
- [ ] Every scale-in goes through KO-4's drain path: edges stop accepting and wait for owned connections, registrar replicas hand off shards, media nodes are removed only at zero anchored sessions. A failing-first test proves a replica is never terminated with owned connections still bound.
- [ ] Guardrails hold under test: no scale-in while shedding, per-zone floor respected, and autoscaling disabled when the invariant gate trips (non-zero cross-node lookups or failing probes).
- [ ] Anti-flap: a scenario alternating load across the stabilization window produces a bounded number of scaling actions, and the bound is documented.
- [ ] Every scaling decision is observable: an event and a metric naming the signal, the observed value and the target.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Invariant gate signals come from [DP-3](DP-3-implement-observability-that-proves-the-invariants.md) and [ET-5](ET-5-publish-probe-results-as-metrics-and-alerts.md).
