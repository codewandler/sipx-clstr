---
id: KO-2
title: Ship the Helm chart for a local k3s environment
pillar: Cluster
status: blocked
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
- **`in-progress` → `blocked` (coordinator, this run).** It was sitting at the top of the board's
  **Now** list as "the headline deliverable" while five of its six acceptance items cannot be
  attempted, which makes the board read as though the chart is being worked when nothing can move it.
  Its own note below already said why — "There is no image to run and no CRD to validate against, so
  the rendered resource is currently unserved" — so this is a status catching up with a fact the story
  had already recorded.

  **What blocks it, precisely:** acceptance item 1 installs "the operator, its CRDs and RBAC", and the
  operator is [`KO-3`](KO-3-implement-the-operator-reconcile-loop.md) (`backlog`).
  [`KO-1`](KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md) is done and pinned the
  custom-resource contract; the remaining absence is the reconciler and the objects it owns. Item
  4's probe verdict additionally wants [`ET-4`](ET-4-implement-the-probe-control-api.md) (`backlog`), since
  `ET-2` and `ET-3` built the engine and the echo endpoint but nothing exposes a verdict to a chart.

  **What would unblock it:** `KO-3`, followed by the probe control/deployment path. `KO-16` keeps
  the skeleton's metadata honest while that implementation is absent.

  What *is* done stays done: the chart skeleton, `helm lint`/`helm template` passing, and `KO-14`
  bringing the default set to the config schema.

- **Started 2026-07-28.** `deploy/helm/` holds the chart skeleton: `Chart.yaml`, `values.yaml`
  carrying the default deployment set, and `templates/sipxcluster.yaml`, which emits
  `.Values.cluster` verbatim into the SipxCluster spec. `helm lint` and `helm template` pass.
- Contributed by a downstream deployment of this platform, which had
  built it downstream before the boundary was corrected: the chart belongs to this story, and a
  consuming deployment carries only its own values.
- **The API remains alpha** (`sipx.dev/v1alpha1`), but its current contract is pinned by completed
  story `KO-1`; alpha status is not evidence that no contract exists.
- Still missing, and the bulk of this story: the operator Deployment, the CRDs, RBAC, the managed
  PostgreSQL and rtpengine dependencies, and a NOTES.txt. There is no image to run and no CRD to
  validate against, so the rendered resource is currently unserved.
- The default set's shape and the reasoning behind it (self-contained, echo trunk, probe on,
  `hostNetwork` placement arithmetic) is exercised by that deployment's devspace profiles.

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Probe: [ET-6](ET-6-run-continuous-probes-against-the-reference-deployment.md). Reference topology it scales down from: [DP-2](DP-2-author-the-3-zone-reference-topology.md).
