---
id: KO-12
title: Publish the chart as an OCI artifact
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy, ci]
note: blocked by KO-2; how deployments consume the chart without a checkout
---

# Publish the chart as an OCI artifact

## Goal
Publish the chart as a versioned OCI artifact so a deployment can install it by reference instead of from a checkout of this repository.

## Acceptance
- [ ] CI publishes a versioned chart to an OCI registry on release.
- [ ] `helm install` from the registry reference works, and a GitOps controller can track it.
- [ ] Chart version and appVersion come from the release, not from a hand edit.
- [ ] The chart is never built differently for local use — a checkout install and a registry install produce the same manifests, asserted by a test.
- [ ] The registry reference is documented where a consuming deployment will look for it.

## Progress
- (not started)

## Notes
- Blocked by KO-2. Until this lands, a consuming deployment must point at a checkout path, which makes the chart version implicit and unpinnable.
- Filed from a downstream deployment of this platform, which resolves the chart via a path variable for exactly this reason (its ledger entry **U-17**).
