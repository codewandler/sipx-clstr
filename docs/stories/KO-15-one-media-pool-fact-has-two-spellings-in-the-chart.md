---
id: KO-15
title: One media-pool fact has two spellings in the chart
pillar: Cluster
status: done
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

- [x] The duplication is resolved in **one** direction, and the direction is recorded: either
      `deployment.rtpengine.enabled` is derived from `cluster.mediaPool[].mode` (the schema is the
      single source, matching `KO-1`'s single-definition rule) or it is removed and the workload
      condition reads the mode directly. Not both, and not a validation rule that merely refuses
      disagreement — a refusal still leaves two places to write the same fact.
      → **Removed.** `deployment.rtpengine.enabled` is gone from `deploy/helm/values.yaml:129`; the
      condition is `deploy/helm/templates/_helpers.tpl`'s `sipx-clstr.mediaPool.managed`, which reads
      `cluster.mediaPool[].mode` directly. Direction recorded in
      `docs/specs/sipx-cluster-crd.md` §5 (the paragraph under the chart-local table) and §11.
- [x] **Failing-first**: a check or a rendered-template assertion that fails while the two keys can
      disagree — set `deployment.rtpengine.enabled: false` with `cluster.mediaPool[0].mode: managed`
      and show that nothing today objects, or that the render silently follows one of them.
      → Both halves shown: at the merge base, `enabled: false` beside `mode: managed` passes
      `check-crd-drift.py` with exit 0; the new fifth axis fails on the unmodified chart naming
      exactly `deployment.rtpengine.enabled`. See Progress.
- [x] Whatever `KO-1`'s CRD-drift checker (`scripts/check-crd-drift.py`) can hold of this is held by
      it, rather than by a second checker. If it cannot, say why in one line.
      → Held by it, as a fifth axis; no second checker. What it cannot hold in one line: it reads
      names, so it cannot judge that a *reason* written in the declaration table is true — it can
      only make the declaration compulsory.
- [x] `deploy/helm/check-values.sh` and the default deployment set still pass, and the chart still
      renders exactly one `SipxCluster` (`KO-2`'s first acceptance item is unaffected).
- [x] `scripts/gate.sh` green.

## Progress

- **Direction: the key is removed, the mode is the only switch.** The alternative — deriving
  `deployment.rtpengine.enabled` from the mode — would have kept a values key an overlay can still
  write, so a `--set deployment.rtpengine.enabled=false` would silently beat the derivation or be
  silently ignored; either way there would still be two places to write the fact, which is what the
  Acceptance rules out. Removing it leaves `cluster.mediaPool[].mode` as the single source, which is
  also the direction sipx-cluster-crd K1/K4 already fix (the document decides, the chart reads) and
  the one `cluster.probe` has had since `KO-14` for the same reason.
- **What is left under `deployment.rtpengine`** — `image`, `replicas`, `hostNetwork` — is the managed
  workload's *shape*, and the configuration document carries none of it: `mediaPool[]` has no image
  field, and in `mode: managed` the operator owns the node list (`KO-7`), so the replica count has
  nothing to disagree with. Removing the whole block would have left `KO-2` with no image to run.
- **The condition** is `deploy/helm/templates/_helpers.tpl`'s `sipx-clstr.mediaPool.managed`: "true"
  when any declared pool is `managed`, empty otherwise. `KO-2`'s rtpengine workload guards on it.
  Nothing renders it yet — verified out-of-tree against a copy of the chart with a probe template:
  default → `true`, `--set 'cluster.mediaPool[0].mode=external'` → empty, no `mediaPool` → empty.
- **Held by `KO-1`'s checker, as a fifth axis** (`scripts/check-crd-drift.py`), not a second script:
  *the `deployment:` half is closed and declared too*. Every path the chart writes under
  `deployment:` is either an operator-half row of sipx-cluster-crd §5 or a row of §5's new
  chart-local table, checked in both directions **one level below each key** — which is what makes a
  switch re-added *inside* an already-declared block as red as a whole new block. Rule M7 in §4; the
  self-test gained four assertions, one of which replays this story's own shape.
  Its limit, in one line: it reads names, so it forces a reason to be written for every chart-local
  key and cannot judge whether that reason is true.
- **Failing-first, at `git merge-base main HEAD` = `2cb22dd`:**
  - the base checker, on the base chart with `deployment.rtpengine.enabled: false` beside
    `cluster.mediaPool[0].mode: managed`, exits **0** — "the mapping 1:1 with the chart". Nothing
    objected to the two keys contradicting each other.
  - the new axis, run against the **unmodified** `deploy/helm/values.yaml`, exits 1 with exactly one
    problem: "deploy/helm/values.yaml writes `deployment.rtpengine.enabled` and sipx-cluster-crd.md
    declares no row for it — a `deployment:` key with no `SipxCluster` field is either a chart-local
    one §5 declares with a reason or a defect (M7)".
- **Gate green**, and `deploy/helm/check-values.sh` still loads the rendered document for all six
  identities; `helm template` still renders exactly one `SipxCluster`.
- **Not fixed here** (noted for `KO-2`): `deployment.affinity` is chart-local while `nodeSelector`
  and `tolerations` reach the resource, so the third member of that triple stops at the chart. It is
  now declared with that fact written down rather than unremarked; adding a fifth operator field is a
  change to sipx-cluster-crd K3 and §4's three lists at once, which is not this story's.

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
