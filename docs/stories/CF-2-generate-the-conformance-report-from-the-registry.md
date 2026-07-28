---
id: CF-2
title: Generate the conformance report from the registry
pillar: Platform
status: backlog
priority: 
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: blocked by EX-2
---

# Generate the conformance report from the registry

## Goal
Generate the per-requirement conformance report — like the story board, never hand-maintained.

## Acceptance
- [ ] Each requirement reports one status: implemented / not applicable / profile-disabled / partial / known deviation / interop workaround — with its proving tests linked.
- [ ] The four coverage kinds (syntax, behavioral, role, interop) are reported separately.
- [ ] Generation is idempotent and CI-checked; a drifted report fails the gate.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
