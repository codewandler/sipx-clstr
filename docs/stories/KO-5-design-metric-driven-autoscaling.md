---
id: KO-5
title: Design metric-driven autoscaling on SIP signals
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: phase 2; blocked by DP-3
---

# Design metric-driven autoscaling on SIP signals

## Goal
Decide what the cluster scales on and what it must refuse to do: the Prometheus-derived signals per role, the mechanism that acts on them, and the guardrails that keep an incident from being made worse by a scaling decision.

## Acceptance
- [ ] Per role, the scaling signals are named with their capacity ratio and target: registrations and write latency per shard, CPS and in-flight transactions per edge, active dialogs, media sessions per relay, breaker state, overload shed rate. CPU/memory are explicitly rejected as primary signals with the reasoning recorded.
- [ ] The mechanism is chosen and justified — operator-reconciled replicas versus HPA fed by a custom-metrics adapter (Prometheus Adapter / KEDA as integration candidates).
- [ ] Guardrails are specified: stabilization windows and hysteresis; no scale-in while overload control is shedding; a per-zone replica floor; and the invariant gate — a non-zero cross-node dialog-lookup counter or failing probes disable autoscaling entirely, because that is a correctness signal, not a capacity signal.
- [ ] The recording rules for each signal are written down as the contract between DP-3's metrics and the scaler.
- [ ] How the design will be tested is decided: which scenarios (registration storm, trunk failover burst, shedding-induced relief, flapping) run in the harness versus only in a live cluster.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Metric set: [DP-3](DP-3-implement-observability-that-proves-the-invariants.md). Overload control: [RT-3](RT-3-implement-overload-control.md).
