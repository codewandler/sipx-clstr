---
id: KO-1
title: Specify the SipxCluster CRD and the values.yaml contract
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: blocked by DP-1; the CR spec *is* the config schema
---

# Specify the SipxCluster CRD and the values.yaml contract

## Goal
Specify one desired-state document: a `SipxCluster` custom resource whose spec is DP-1's config schema, rendered 1:1 from a single `values.yaml`, plus the status conditions that make reconciliation auditable.

## Acceptance
- [ ] The CRD spec covers zones, roles and replicas (`edge`, `registrar`, `inbound-proxy`, `outbound-proxy`, `e2e-tester`), listeners per transport with addresses and port ranges, deployment profile, location store, media pool, token keys and membership, trunks and route policy, and probe configuration.
- [ ] `status` is specified: conditions (`Ready`, `ProfileCompatible`, `ShardMapConverged`, `KeysDistributed`), observed shard map, per-role ready counts, last probe verdict.
- [ ] The single-source mechanism between DP-1's config schema and the CRD is decided and recorded — generated one from the other, or one shared definition — with a check that fails when they drift.
- [ ] The `values.yaml` → CR mapping is documented field by field and is 1:1; no value is computed in the chart that the CR cannot express.
- [ ] Validation rules are normative: an incompatible profile/role set, a media pool without a port range, or a zone with no edge is rejected at admission, not at call time.
- [ ] CRD versioning and upgrade policy is stated.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Config schema: [DP-1](DP-1-design-roles-and-the-config-schema.md). Profile compatibility: [EX-5](EX-5-implement-deployment-profiles-with-compatibility-checking.md).
