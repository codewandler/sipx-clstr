---
id: CF-4
title: Add fault injection to the simulation
pillar: Platform
status: done
priority:
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: the schedule half; the sim-vs-real fidelity row moved to CF-3, which owns the sockets
---

# Add fault injection to the simulation

## Goal
Make failure a first-class input: node kill, partition, timer skew and packet loss/duplication/reordering as scripted schedules.

## Acceptance
- [x] Fault schedules compose with any scenario; failing seeds reproduce exactly. — `sipx-clstr-sim::fault`; `tests/faults.rs` proves composition (merging equals queuing separately) and reproduction (same seed → identical trace, different seed → different trace).
- [ ] A simulation-vs-real comparison run validates the failure model against real sockets and documents the fidelity gaps. — **moved to `CF-3`**, which is the story that builds the real-socket layer. See Progress.

## Progress
**The schedule half is done (2026-07-29); the fidelity half moved to `CF-3`.**

- `fault.rs` carries `Fault` — `KillNode`, `Partition`, `Heal`, `SetLinkPolicy`, `TimerSkew` — and
  `Schedule`, both plain values. Composition is concatenation, so two scenarios' weather merges
  without either knowing about the other, and `Sim::schedule` can be called repeatedly with the
  same result as merging first (asserted, not assumed).
- **No third mechanism.** Every fault is an override of what `net.rs` already models: a partition
  is `partitioned` on the crossing links, a kill is that on all of a node's links. Nothing reaches
  past the link layer to invent a second way for a message to go missing, because two mechanisms
  that can both drop a packet eventually disagree about whether one was dropped.
- **Faults are queue entries.** A scheduled fault is an event with a deadline like any other, so
  it interleaves with deliveries and timers under the same insertion-sequence tie-break. Applying
  faults from outside the queue would have made their ordering depend on when the scenario author
  called a method — the one thing in the harness whose order was not a function of (scenario, seed).
- **Kill is not isolation.** `KillNode` drops the node's timer generations as well as cutting its
  links, so nothing fires afterwards. A node that keeps timing out while unreachable is a real and
  *different* failure, and it is `Partition` over all of that node's links.
- **A fault at `START` lands after `Started`.** Nodes are started before the queue is drained, so
  a `TimerSkew` at time zero does not affect the timer a node arms in reaction to `Started`, only
  what it arms afterwards. Pinned by `timer_skew_changes_what_a_node_waits_for`, which asserts
  five pings rather than four for exactly this reason.
- **The existing suite is untouched**: 53 unit tests and every scenario file pass at the same
  seeds, and `a_scenario_with_no_schedule_is_unchanged` asserts the zero case directly, because
  adding fault machinery must not perturb a scenario that never uses it.
- The trace gained one event, `Fault(String)`, rendered as `** FAULT …`, and `viz.rs` a matching
  `FrameKind::Fault` — its totality test makes that mandatory rather than optional.

**Why the second criterion moved rather than being marked done or left hanging:** validating the
failure model against real sockets needs real sockets, and the story that builds them is `CF-3`
(SIPp, the sipx CLI, rtpengine, a containered node). Writing a comparison run here would mean
building half of `CF-3` inside `CF-4` and calling it a fidelity check. `CF-3`'s acceptance now
carries the row.

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md).
- The load-source ticks and LB-stickiness faults the design also sketches (`RemapFlow`,
  `stickiness_miss`) are **not** here: both need the LB actor, which arrives with the multi-node
  work in M2. `Fault` is a closed enum precisely so adding them is a compile error at every match
  rather than a silently unhandled variant.
