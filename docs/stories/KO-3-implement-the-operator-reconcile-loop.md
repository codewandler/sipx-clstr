---
id: KO-3
title: Implement the operator reconcile loop
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: blocked by KO-1
---

# Implement the operator reconcile loop

## Goal
Project a `SipxCluster` onto the cluster resources it implies — workloads per role, services, config, secrets, disruption budgets, network policies, scrape config — and report honest status.

## Acceptance
- [ ] Reconciliation is idempotent and convergent: re-running against an unchanged CR performs no writes; a hand-edited managed object is corrected back.
- [ ] Generated objects match the DP-2 reference topology's intent for the same input — SIP constraints preserved (host networking or a source-preserving L4 path for public UDP, long-lived flows pinned to their owning edge, media on dedicated/host-network nodes or declared-not-managed, management interfaces private by NetworkPolicy).
- [ ] Status conditions are set from observed state, not from intent: `Ready`, `ProfileCompatible`, `ShardMapConverged`, `KeysDistributed`, plus per-role ready counts.
- [ ] An invalid CR (incompatible profile set, zone without an edge, media pool without a port range) fails validation with a message naming the offending field, and no partial deployment is created.
- [ ] Tests run against a real API server (envtest-class or a k3s in CI), covering create, update, drift correction and delete.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Lifecycle sequencing lands in [KO-4](KO-4-implement-sip-aware-rollout-and-shard-handoff.md).
