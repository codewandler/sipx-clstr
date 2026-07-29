---
id: DX-10
title: Write the deployment and scaling operate pages
pillar: Foundation
status: ready
priority: 6
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, deploy, k8s]
note: HPA on CPU is explicitly rejected — scaling signals are SIP-shaped, and that is the story
---

# Write the deployment and scaling operate pages

## Goal

Give `website/docs/operate/deploy.md` and `website/docs/operate/scaling.md` their content: how
this is meant to be deployed once the operator exists, and the autoscaling model — which is the
top of the ladder this site is built to climb.

## Acceptance

- [ ] Both pages open with the `:::caution Preview` admonition and say what runs today: a
      container and a single-node k3d profile, nothing else.
- [ ] `deploy.md` describes the reference topology (roles by config, one region, three zones as
      failure domains), the operator and chart arrangement, and states that the chart currently
      renders a custom resource **nothing serves**.
- [ ] It notes that `edge` and `registrar` are host-networked because UDP 5060 must see the real
      source address, and that replica counts above node count are therefore a placement error
      rather than a capacity decision.
- [ ] `scaling.md` explains why **HPA on CPU or memory is explicitly rejected** — it does not see
      the constraints that bind: connections, shard ownership.
- [ ] It gives the SIP-shaped signals scaling is driven by (registrations per shard, calls per
      second, in-flight transactions, media sessions, shed rate) and the guardrails that hold
      scaling — shedding load, a non-zero invariant counter, a failing probe, a zone floor —
      naming those as correctness signals rather than capacity signals.
- [ ] It states that every scale-in goes through the same drain path as a rolling update, and that
      scale-in is sequenced **after** the drain machinery for that reason.
- [ ] Both link their designs by absolute GitHub URL.

## Progress

- (running log)

## Notes

- Designs: `docs/designs/deployment.md`, `docs/designs/k8s-deployment-operator.md`. Absolute
  GitHub URLs only.
- Autoscaling is phase 2 of the operator epic (`KO-5`, `KO-6`), after drain (`KO-4`).
- Do not document `deploy/helm/values.yaml` as the configuration schema — see `DX-5` and `KO-14`.
