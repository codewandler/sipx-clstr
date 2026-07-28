---
id: CF-5
title: Implement the deterministic cluster harness
pillar: Platform
status: backlog
priority:
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: blocked by CF-1 — early M1; PX-4/PX-7/RG-3 vector runs depend on it
---

# Implement the deterministic cluster harness

## Goal
Build what CF-1 designs: the seeded, virtual-time, multi-node simulation — virtual clock,
in-memory transports, scripted topologies — that every M1 vector story runs under. CF-1 decides;
this story ships the runtime.

## Acceptance
- [ ] A scenario runs N platform nodes and M simulated endpoints on a virtual clock; timers fire
only when time is advanced, and two runs with the same seed produce identical traces.
- [ ] In-memory transports support the loss/duplication/reordering/latency knobs CF-1 specifies
(partition and kill schedules arrive with CF-4).
- [ ] A trivial end-to-end scenario (one node, REGISTER then INVITE against stub logic) passes
in CI as the harness's own smoke test.
- [ ] Components upstreamed per CF-1's sipx-testkit split are consumed from sipx, not forked.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md). This story exists because
the epic's done-criteria said "implemented early in M1" while no story implemented it.
