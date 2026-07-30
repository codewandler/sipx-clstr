---
id: KO-15
title: One media-pool fact has two spellings in the chart
pillar: Cluster
status: ready
priority: 2
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [deploy, config]
note: deployment.rtpengine.enabled and cluster.mediaPool[].mode duplicate one fact; harmless only while nothing consumes it
---

# One media-pool fact has two spellings in the chart

## Goal

`deploy/helm/values.yaml` says whether this deployment runs its own rtpengine **twice**, in two
places that can disagree:

- `deployment.rtpengine.enabled`, a chart-level boolean, and
- `cluster.mediaPool[].mode: managed | external`, which is the config schema's own answer, rendered
  verbatim into the custom resource.

Two spellings of one fact is a defect whether or not they currently agree, because nothing makes them
agree — an operator who flips one is entitled to expect the platform to follow, and only one of the
two is read by anything.

## Acceptance

- [ ] The duplication is resolved in **one** direction, and the direction is recorded: either
      `deployment.rtpengine.enabled` is derived from `cluster.mediaPool[].mode` (the schema is the
      single source, matching `KO-1`'s single-definition rule) or it is removed and the workload
      condition reads the mode directly. Not both, and not a validation rule that merely refuses
      disagreement — a refusal still leaves two places to write the same fact.
- [ ] **Failing-first**: a check or a rendered-template assertion that fails while the two keys can
      disagree — set `deployment.rtpengine.enabled: false` with `cluster.mediaPool[0].mode: managed`
      and show that nothing today objects, or that the render silently follows one of them.
- [ ] Whatever `KO-1`'s CRD-drift checker (`scripts/check-crd-drift.py`) can hold of this is held by
      it, rather than by a second checker. If it cannot, say why in one line.
- [ ] `deploy/helm/check-values.sh` and the default deployment set still pass, and the chart still
      renders exactly one `SipxCluster` (`KO-2`'s first acceptance item is unaffected).
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

## Notes

- **Found by `KO-1`** while pinning the `SipxCluster` resource to the config schema, and reported as
  wanting its own story rather than a note: "it is the same shape as the `DP-8` flag drift — one fact
  with two spellings that can disagree — and it is currently harmless only because nothing consumes
  the key yet. Once `KO-2` writes the rtpengine workload, it stops being harmless."
- The `DP-8` precedent is the reason this is `priority: 2` rather than a cleanup. When `DP-8` replaced
  three provisional flags, roughly thirty documented commands went on naming flags the binary had
  stopped accepting, through a release, with the whole gate green — because nothing held the two
  surfaces to each other. This is that shape before it has cost anything.
- Sequencing: this wants to land **before** [`KO-2`](KO-2-ship-the-helm-chart-for-a-local-k3s-environment.md)
  writes the rtpengine workload, and it is cheap now precisely because no consumer exists. It does not
  block `KO-3`.
- Considered for upstream: no. A Helm values contract and its rendering are deployment orchestration;
  there is nothing protocol-generic here.
