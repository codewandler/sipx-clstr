---
id: KO-16
title: Make the Helm skeleton advertise only what it actually installs
pillar: Cluster
status: ready
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

- [ ] `deploy/helm/Chart.yaml` describes a non-operational schema/rendering preview. It does not say
      `helm install` installs an operator, CRDs, RBAC, or a self-contained local cluster until those
      templates and images exist.
- [ ] The chart's version/appVersion and comments no longer present the old `0.1.0` package or
      pre-KO-1 “provisional API” state as current fact. KO-1 is recorded as complete; the remaining
      absence is implementation, not an undecided resource contract.
- [ ] `helm show chart` and `helm template` are the acceptance surfaces. A failing-first check asserts
      the rendered object inventory is exactly one `SipxCluster` and that package metadata explicitly
      labels it unserved/non-operational; it fails on `86e6b10` because the description promises a
      complete installation.
- [ ] The chart emits an unmistakable install-time note explaining that no CRD/controller is shipped
      and linking the named blockers. It must not make a dry-run or rendered manifest look like a
      ready SIP deployment.
- [ ] KO-2 remains `blocked` until, at minimum, a served CRD, operator image and reconcile loop, RBAC,
      node/database workload rendering, Services, health checks, and the passing local probe exist.
      KO-16 changes the honesty of the skeleton; it does not satisfy any of those blockers.
- [ ] The default values/rendering checks remain green for all six roles, exactly one custom resource
      is rendered, and `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Source: validated synthesis **V-19**. `deploy/helm/Chart.yaml:3-9` claims the chart installs an
  operator, CRDs, RBAC, and a working local environment, while its only template is
  `deploy/helm/templates/sipxcluster.yaml`. The values and public deploy page already admit that
  nothing reconciles the resource.
- Dependencies: none. This should land while KO-2 is blocked; truthful metadata does not require the
  missing controller. It must not unblock KO-2.
- Considered for upstream: **no.** Helm package metadata, custom-resource delivery, and the operator's
  blocker contract are Kubernetes deployment orchestration local to this platform.
