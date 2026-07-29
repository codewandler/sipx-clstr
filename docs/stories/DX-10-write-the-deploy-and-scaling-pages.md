---
id: DX-10
title: Write the deployment and scaling operate pages
pillar: Foundation
status: done
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

- [x] Both pages open with the `:::caution Preview` admonition and say what runs today: a
      container and a single-node k3d profile, nothing else.
- [x] `deploy.md` describes the reference topology (roles by config, one region, three zones as
      failure domains), the operator and chart arrangement, and states that the chart currently
      renders a custom resource **nothing serves**.
- [x] It notes that `edge` and `registrar` are host-networked because UDP 5060 must see the real
      source address, and that replica counts above node count are therefore a placement error
      rather than a capacity decision.
- [x] `scaling.md` explains why **HPA on CPU or memory is explicitly rejected** — it does not see
      the constraints that bind: connections, shard ownership.
- [x] It gives the SIP-shaped signals scaling is driven by (registrations per shard, calls per
      second, in-flight transactions, media sessions, shed rate) and the guardrails that hold
      scaling — shedding load, a non-zero invariant counter, a failing probe, a zone floor —
      naming those as correctness signals rather than capacity signals.
- [x] It states that every scale-in goes through the same drain path as a rolling update, and that
      scale-in is sequenced **after** the drain machinery for that reason.
- [x] Both link their designs by absolute GitHub URL.

## Progress

- Both pages written; the placeholder bodies are gone and the frontmatter is unchanged.
- `deploy.md` — "What runs today" status table, the three-zone topology with a mermaid diagram and
  the exposure table, roles as configuration from the closed set, host networking and the replica
  count it bounds, the operator/chart arrangement with the four lifecycle events a template cannot
  express, and a section saying outright that no operator image and no CRD exist. It also warns
  against reading `deploy/helm/values.yaml` as the configuration schema (`KO-14`).
- `scaling.md` — CPU rejected on three grounds (it does not see connections or shard ownership; a
  registration storm, a UDP flood and media load are not alike in CPU terms; under shedding the
  signal moves the wrong way), the per-role signal table, the four guardrails presented as
  correctness signals that *disable* the decision rather than feed it, scale-out versus scale-in
  per role, and the sequencing argument: a scale-in is the rolling-update drain path, so it cannot
  precede `KO-4`.
- Zones are framed as failure domains rather than shards, and the zone floor is justified as the
  thing that keeps them that.
- Gate: `python3 scripts/check-docs.py` → `docs: clean (166 markdown files checked)`;
  `npm run build` in `website/` → `[SUCCESS] Generated static files in "build"`. The mermaid block
  lands in the client bundle exactly as `clustering/how-it-works.md`'s does.
- Considered for upstream: no. These pages describe this platform's own deployment shape — roles,
  zones, shards, the operator and its custom resource — and the kernel has no opinion about any of
  them.

## Notes

- Designs: `docs/designs/deployment.md`, `docs/designs/k8s-deployment-operator.md`. Absolute
  GitHub URLs only.
- Autoscaling is phase 2 of the operator epic (`KO-5`, `KO-6`), after drain (`KO-4`).
- Do not document `deploy/helm/values.yaml` as the configuration schema — see `DX-5` and `KO-14`.
