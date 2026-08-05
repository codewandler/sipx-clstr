---
id: KO-16
title: Make the Helm skeleton advertise only what it actually installs
pillar: Cluster
status: in-progress
priority: 2
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy, docs]
note: V-19 — metadata promises a full install; the chart emits one unserved custom resource
---

# Make the Helm skeleton advertise only what it actually installs

## Goal

Make Helm's own package metadata tell the truth about the current skeleton: it renders one
`SipxCluster` resource, but installs no CRD, controller, RBAC, node, database, media relay, or probe.
Keep KO-2 blocked on those executable pieces rather than letting a successful render impersonate a
working deployment.

## Acceptance

- [x] `deploy/helm/Chart.yaml` describes a non-operational schema/rendering preview. It does not say
      `helm install` installs an operator, CRDs, RBAC, or a self-contained local cluster until those
      templates and images exist.
- [x] The chart's version/appVersion and comments no longer present the old `0.1.0` package or
      pre-KO-1 “provisional API” state as current fact. KO-1 is recorded as complete; the remaining
      absence is implementation, not an undecided resource contract.
- [x] `helm show chart` and `helm template` are the acceptance surfaces. A failing-first check asserts
      the rendered object inventory is exactly one `SipxCluster` and that package metadata explicitly
      labels it unserved/non-operational; it fails on `86e6b10` because the description promises a
      complete installation.
- [x] The chart emits an unmistakable install-time note explaining that no CRD/controller is shipped
      and linking the named blockers. It must not make a dry-run or rendered manifest look like a
      ready SIP deployment.
- [x] KO-2 remains `blocked` until, at minimum, a served CRD, operator image and reconcile loop, RBAC,
      node/database workload rendering, Services, health checks, and the passing local probe exist.
      KO-16 changes the honesty of the skeleton; it does not satisfy any of those blockers.
- [x] The default values/rendering checks remain green for all six roles, exactly one custom resource
      is rendered, and `scripts/gate.sh` is green.

## Progress

- **Implemented (KO-16, this worktree).** The check is `deploy/helm/check-advertised.sh` — helm-only,
  so it sits beside `check-values.sh` outside the gate for the reason `scripts/gate.sh` records. It
  holds four surfaces: the `helm show chart` description must label the chart
  non-operational/unserved and must not read “Installs the …” (V-19's verb phrase, matched as the
  claim rather than the vocabulary, since the honest text uses the same nouns to say they are
  absent); `version`/`appVersion` must equal Cargo.toml `[workspace.package] version` — the only
  version this repo cuts, given the chart is unpublished (KO-12) and no image backs the default tag
  (KO-17); the `helm template` inventory must be exactly `["SipxCluster"]` and the rendered stream
  must carry the `UNSERVED` marker (plain `#` lines in `templates/sipxcluster.yaml`, deliberately
  outside the `{{/* */}}` block so helm emits them and a saved manifest still says what applying it
  will not do); and `templates/NOTES.txt` must exist naming no-CRD/no-controller, KO-3, ET-4, and a
  GitHub link — read from source because helm never templates notes. Proved red at the merge base
  `134e7e1` (first failure line: “the description promises a complete installation”), and
  `deploy/helm/Chart.yaml` is byte-identical between `86e6b10` and that base, so the story's cited
  sha fails the same way. `check-values.sh` re-run after the change: all six roles load, `helm lint`
  clean. KO-2 untouched and still `blocked`. A resuming agent should know: the version assertion
  means every release cut now also bumps `Chart.yaml` or `check-advertised.sh` goes red — that is
  the designed direction (stale claim → red check), not an accident.

## Notes

- Source: validated synthesis **V-19**. `deploy/helm/Chart.yaml:3-9` claims the chart installs an
  operator, CRDs, RBAC, and a working local environment, while its only template is
  `deploy/helm/templates/sipxcluster.yaml`. The values and public deploy page already admit that
  nothing reconciles the resource.
- Dependencies: none. This should land while KO-2 is blocked; truthful metadata does not require the
  missing controller. It must not unblock KO-2.
- Considered for upstream: **no.** Helm package metadata, custom-resource delivery, and the operator's
  blocker contract are Kubernetes deployment orchestration local to this platform.
