---
id: CF-4
title: Add fault injection to the simulation
pillar: Platform
status: backlog
priority: 
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: blocked by CF-5; UPSTREAM: testkit split filed as sipx X-14 — see docs/upstream.md
---

# Add fault injection to the simulation

## Goal
Make failure a first-class input: node kill, partition, timer skew and packet loss/duplication/reordering as scripted schedules.

## Acceptance
- [ ] Fault schedules compose with any scenario; failing seeds reproduce exactly.
- [ ] A simulation-vs-real comparison run validates the failure model against real sockets and documents the fidelity gaps.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
