---
id: CF-3
title: Build the real-network interop harness
pillar: Platform
status: backlog
priority: 
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: SIPp + sipx CLI + rtpengine
---

# Build the real-network interop harness

## Goal
Build the real-socket layer on top of simulation: SIPp scenarios and the sipx CLI phone against a containered deployment.

## Acceptance
- [ ] SIPp suites and CLI-driven register/call scenarios run in CI against a containered node; rtpengine joins for media-path tests.
- [ ] Harness traps and gaps are documented and filed as stories, mirroring the kernel's interop discipline.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
