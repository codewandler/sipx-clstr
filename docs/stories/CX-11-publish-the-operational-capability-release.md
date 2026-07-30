---
id: CX-11
title: Publish the M4 operational capability release
pillar: Platform
status: backlog
priority: 22
design: docs/specs/operational-capability-baseline.md
epic:
areas: [release, docs, deploy]
note: blocked by CX-10, KO-12, KO-17 and green release gates
---

# Publish the M4 operational capability release

## Goal

Publish the exact artifacts that passed M4 with an honest capability and HA statement.

## Acceptance

- [ ] The tested node-image digest, chart, documentation and source tag are published without a
      rebuild between proof and release.
- [ ] A clean three-zone install resolves only immutable references and passes the smoke subset of
      `OB-1`, `OB-8`, `OB-10` and `OB-12`.
- [ ] Release notes link the generated proof, upstream pin, SBOM/checksums and failure-mode table.
- [ ] Public docs name the explicit M4 exclusions and make no call-survival promise beyond `SS-12`.
- [ ] `CX-8` closes only after the published references are independently re-fetched and verified.

## Progress

- Not started.
