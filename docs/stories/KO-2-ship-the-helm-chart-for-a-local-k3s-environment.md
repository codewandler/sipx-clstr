---
id: KO-2
title: Ship the Helm chart for a local k3s environment
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: the headline deliverable — helm install on k3s
---

# Ship the Helm chart for a local k3s environment

## Goal
`helm install` with one `values.yaml` stands up a small but genuinely clustered platform on a local k3s cluster: operator, CRDs, RBAC, and a `SipxCluster` resource that produces a working deployment.

## Acceptance
- [ ] The chart installs the operator, its CRDs and RBAC, and renders exactly one `SipxCluster` from `values.yaml`.
- [ ] A documented single-node k3s profile: one PostgreSQL, one rtpengine, multiple edge replicas, HA relaxed and *said* to be relaxed, host networking with a declared UDP port range.
- [ ] Acceptance run, scripted and repeatable: `helm install` on local k3s, then two sipx CLI phones register and call each other through the platform with media flowing.
- [ ] The `e2e-tester` probe deployed by the same chart reports `pass` against the fresh install.
- [ ] `helm upgrade` with a changed `values.yaml` (replica count, listener set) converges without manual steps; `helm uninstall` leaves nothing behind.
- [ ] The README states plainly what the local environment does *not* prove (zone spread, real source preservation, HA of the store).

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Probe: [ET-6](ET-6-run-continuous-probes-against-the-reference-deployment.md). Reference topology it scales down from: [DP-2](DP-2-author-the-3-zone-reference-topology.md).
