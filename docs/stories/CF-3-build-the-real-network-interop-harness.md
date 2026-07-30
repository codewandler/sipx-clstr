---
id: CF-3
title: Build the real-network interop harness
pillar: Platform
status: backlog
priority: 2
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
- [ ] At least one SIPp scenario exercises the node with a SIP implementation that does not reuse the
      sipx parser, serializer, transaction, or transport crates: REGISTER, INVITE, provisional and
      final responses, dialog ACK, and BYE all cross real sockets and are asserted on the wire.
- [ ] The independent scenario retains the remote Contact and Record-Route set learned from the
      dialog and constructs ACK/BYE from them. An AoR-shaped shortcut is not accepted as evidence for
      in-dialog routing; PX-13 and ET-7 own the node/probe fixes this proof exercises.
- [ ] The existing `scripts/e2e-call.sh` run remains as a same-kernel, separate-process integration
      test and is labelled that way in CI output and documentation. It is not counted as the
      independent-parser interop leg.
- [ ] CI reports the same-kernel integration and independent interop legs separately, so one cannot
      mask or substitute for the other. A missing independent tool exits as an unready/failing interop
      job, not a green skip.
- [ ] A failing-first capture from `86e6b10` demonstrates why the separation matters: the current
      same-tag `sipx` CLI run cannot satisfy the independent-implementation criterion even when its
      call and audio assertions pass.

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
- Extended from the validated adversarial review of `86e6b10` (`v0.12.0`), synthesis finding
  **V-17**. CI builds its `sipx` CLI from the same tag whose libraries the node pins. That remains
  valuable process/socket evidence, but it cannot reveal defects shared by both parser/serializer or
  transaction implementations.
- Considered for upstream: **split.** SIPp is a named interop target and this repository owns the
  deployed-node scenarios, CI orchestration, captures, and comparison report. Any generic SIP testkit
  fixture or protocol correction the scenarios expose belongs in sipx first and must be consumed from
  there rather than copied locally.
