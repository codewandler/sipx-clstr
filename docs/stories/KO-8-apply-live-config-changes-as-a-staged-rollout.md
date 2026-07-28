---
id: KO-8
title: Apply live config changes as a staged, health-gated rollout
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: blocked by KO-3, KO-4; re-deploy any time, roll out gracefully
---

# Apply live config changes as a staged, health-gated rollout

## Goal
Make re-deploying config a routine, safe act: edit `values.yaml`, `helm upgrade` (or edit the `SipxCluster`) at any time, and the operator picks the change up and converges the running cluster towards it gracefully instead of restarting it.

## Acceptance
- [ ] Every spec field is classified — hot-reloadable (DP-1's reloadable subset: trunks, token keys, shard map, route policy), needs-a-rollout (listeners, transports, profile, image, replicas), or invalid — and the classification is data, checked by a test that fails when a new field is added without one.
- [ ] Hot-reloadable changes are pushed to running nodes with no pod restart: a failing-first test changes a trunk while calls are in flight and asserts no restart and no dropped call.
- [ ] Rollout-class changes produce a staged plan — one role, one zone at a time — using KO-4's drain path; a test asserts edges are never restarted simultaneously and registrations survive.
- [ ] Between stages the rollout gates on health (`e2e-tester` verdict plus DP-3 invariant metrics) and **pauses** on regression: no further stages, the reason recorded in status and as an event.
- [ ] A config change landing mid-rollout supersedes the plan: the operator re-plans from observed state rather than interleaving, proved by a test that applies two edits back to back.
- [ ] An invalid or incompatible change is rejected at admission naming the offending field; the cluster keeps running the last good config and nothing is partially applied.
- [ ] Status makes an in-flight rollout legible: current stage, remaining stages, what changed, and whether it is progressing, paused or converged.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Reloadable subset and drain-then-switch semantics: [DP-1](DP-1-design-roles-and-the-config-schema.md); drain machinery: [KO-4](KO-4-implement-sip-aware-rollout-and-shard-handoff.md); health signals: [ET-5](ET-5-publish-probe-results-as-metrics-and-alerts.md), [DP-3](DP-3-implement-observability-that-proves-the-invariants.md).
