---
id: KO-17
title: Publish versioned multi-architecture node images
pillar: Cluster
status: backlog
priority: 19
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, release, ci]
note: blocked by DP-15 and runnable KO-2; publish the image CX-10 proves
---

# Publish versioned multi-architecture node images

## Goal

Make the tested Linux x86_64/arm64 node image installable by immutable registry reference.

## Acceptance

- [ ] One release workflow builds both architectures from the locked release source and publishes a
      manifest list plus per-platform digests, checksums, provenance and SBOM.
- [ ] The exact digest tested by `CX-10` is promoted; publication does not rebuild it.
- [ ] Images run non-root, expose only documented listeners and report source/kernel versions.
- [ ] `KO-12`'s chart references the immutable release image and its appVersion comes from the same
      release metadata.
- [ ] A clean registry pull starts the image and passes the bounded node smoke test.

## Progress

- Not started.
