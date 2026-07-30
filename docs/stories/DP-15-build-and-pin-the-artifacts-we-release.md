---
id: DP-15
title: Build release-profile images and pin every input used to prove or publish them
pillar: Cluster
status: ready
priority: 3
design: docs/designs/deployment.md
epic: deployment
areas: [build, ci, deploy, security]
note: V-20 — the documented image is dev-profile and release evidence follows mutable tags
---

# Build release-profile images and pin every input used to prove or publish them

## Goal

Make the container and the evidence attached to it one reproducible release artifact: optimized by
default, built from the lockfile, and tested/published only with immutable dependency, action, and
base-image identities.

## Acceptance

- [ ] The Dockerfile's ordinary/default target builds `sipx-clstr` with Cargo's release profile and
      `--locked`. A separately named development target or explicit `CARGO_PROFILE=dev` remains
      available for iteration; `docker build .` can no longer silently select it.
- [ ] CI builds the exact runtime image once, records its digest, runs the container smoke/e2e checks
      against that digest, and makes that same digest the publishable output. No later job rebuilds a
      look-alike artifact from mutable inputs.
- [ ] The sipx CLI e2e peer is checked out at the full commit recorded in `Cargo.lock`, not merely the
      tag in `Cargo.toml`. CI also verifies that the human-readable tag resolves to that commit and
      fails on disagreement.
- [ ] Every third-party GitHub Action is pinned to a reviewed full commit SHA with its release name in
      a comment. Builder/runtime/service images are pinned by digest while retaining readable tags in
      comments or variables. An automated update may propose changes, but no mutable major tag is an
      executed release input.
- [ ] The image emits an SBOM containing Rust and OS packages plus source revision, sipx lock commit,
      and build profile. Dependency and image vulnerability policy runs against the locked/published
      digest and has an explicit severity/exception rule rather than informational output only.
- [ ] **Failing-first checks:** a repository check fails on default `CARGO_PROFILE=dev`, action refs
      lacking a 40-character SHA, un-digested release image inputs, or an e2e checkout by tag. Each
      condition exists on `86e6b10` and is removed by this story.
- [ ] A release-image smoke test asserts `sipx-clstr --version`, non-root UID, expected entrypoint,
      and the absence of the debug binary path. Public Docker instructions build the same default
      artifact the check proves.
- [ ] `scripts/gate.sh` and the release workflow are green.

## Progress

- (not started)

## Notes

- Source: validated synthesis **V-20**. `Dockerfile:34-52` defaults to the debug/dev build;
  `.github/workflows/ci.yml:126-143` clones the e2e peer by tag even though `Cargo.lock` records a
  full commit; actions and base images also use mutable version tags.
- Dependencies: none. This hardening applies to the current standalone node image and does not wait
  for the operator or Helm chart.
- Considered for upstream: **no.** These are this repository's artifact, CI, and release provenance
  rules. sipx supplies an input commit; pinning and attesting how this product consumes it belongs
  here.
