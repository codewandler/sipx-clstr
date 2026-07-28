---
id: CF-1
title: Design the deterministic cluster harness
pillar: Platform
status: ready
priority: 4
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: UPSTREAM — sipx-testkit split; see docs/upstream.md
---

# Design the deterministic cluster harness

## Goal
Design the seeded, virtual-time, multi-node simulation every cluster behavior must reproduce in before it touches a socket — and decide what generalizes upstream into sipx-testkit.

## Acceptance
- [ ] The design covers the virtual clock, seeded randomness, in-memory transports with loss/duplication/reordering/latency/partition, multi-node topology, scripted fault schedules, and reproduction from a failing seed.
- [ ] The sipx-testkit upstream split is decided per component (clock trait, loopback transport) and recorded in [upstream.md](../upstream.md).
- [ ] The scenario format (code vs declarative) is decided with rationale.
- [ ] The M2 exit assertions — node kill, partition, zero cross-node dialog lookups — are expressible as scenarios in the chosen format.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
