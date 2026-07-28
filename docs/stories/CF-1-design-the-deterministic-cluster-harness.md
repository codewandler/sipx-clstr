---
id: CF-1
title: Design the deterministic cluster harness
pillar: Platform
status: done
priority: 4
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: UPSTREAM: sipx-testkit split filed as sipx X-14; see docs/upstream.md
---

# Design the deterministic cluster harness

## Goal
Design the seeded, virtual-time, multi-node simulation every cluster behavior must reproduce in before it touches a socket — and decide what generalizes upstream into sipx-testkit.

## Acceptance
- [x] The design covers the virtual clock, seeded randomness, in-memory transports with loss/duplication/reordering/latency/partition, multi-node topology, scripted fault schedules, and reproduction from a failing seed.
- [x] The sipx-testkit upstream split is decided per component (clock trait, loopback transport) and recorded in [upstream.md](../upstream.md).
- [x] The scenario format (code vs declarative) is decided with rationale.
- [x] The M2 exit assertions — node kill, partition, zero cross-node dialog lookups — are expressible as scenarios in the chosen format.

## Progress
- 2026-07-28: Concretized the design doc's Approach into decisions. Time: discrete-event
  scheduler owning a virtual clock (one EDF queue, insertion-order tiebreak); sans-IO components
  keep their fired-timer contract — `SetTimer` effects become queue entries at `now + after`,
  cancellation reuses the kernel timer queue's generation-counter discipline. Network: per-link
  `LinkPolicy` (loss, duplication, latency distribution — reordering emerges from jitter —
  partition), datagram vs stream link kinds, fault schedules as time-windowed policy overrides;
  the LB actor models 5-tuple stickiness and PB-A-1 is a `RemapFlow` fault. Scenario format
  decided: code-as-scenarios (Rust builder + typed trace assertions, each a `#[test]`), with
  schedules/policies as declarative values CF-4 can generate; sketch included. Seed model: one
  u64 master seed → labeled derived streams → byte-identical trace; failing seeds reported with
  event index and replayed via env override, pinned as regression tests. Load model for RT-3:
  open-loop rate sources + per-node service model (service-time distribution, bounded queue,
  503 shed), goodput-vs-offered collapse assertion. Three M2 exit scenarios sketched
  (`bye_survives_edge_kill`, `partition_spares_mid_dialog`, `foreign_edge_spray`). Upstream
  split recorded per component in [upstream.md](../upstream.md): timer-queue generalization and
  seeded loopback link upstream (one CX-1 story); virtual clock, simulated network, runner,
  load model, RNG discipline local. Registry alignment decided: extend the kernel's
  `registry.toml` schema to requirement grain as an independent instance, kernel rows inherited
  by reference — feeds EX-2/CF-6. Awaiting design acceptance (doc remains `proposed`) before
  the story closes.

## Notes
- 2026-07-28 — integrator review passed; cross-references reconciled (see CHANGELOG).
- Design: [conformance-harness](../designs/conformance-harness.md).
