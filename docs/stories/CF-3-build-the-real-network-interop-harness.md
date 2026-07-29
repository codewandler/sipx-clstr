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
- [ ] A simulation-vs-real comparison run validates `CF-4`'s failure model against real sockets and documents the fidelity gaps — inherited from `CF-4`, which built the model but had no sockets to check it against.

## Progress
- (not started)
- **Inherited a row from `CF-4` on 2026-07-29.** `CF-4` shipped the fault model — kill, partition,
  heal, link policy, timer skew, all seeded and reproducible — and could not validate it against
  real sockets, because this story is what builds them. The comparison run belongs where the
  sockets are; doing it inside `CF-4` would have meant building half of this story there and
  calling the result a fidelity check.

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
- The model to compare against is `sipx-clstr-sim::fault`. The gaps worth documenting are the ones
  the design already suspects: reordering emerges from latency jitter here rather than being a
  knob, a `Stream` link fails by breaking rather than by stalling, and virtual time means a
  retransmission costs nothing — real sockets will disagree about all three.
