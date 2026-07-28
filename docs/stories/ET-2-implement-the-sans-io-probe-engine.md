---
id: ET-2
title: Implement the sans-IO probe engine
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe, harness]
note: blocked by ET-1, CF-1
---

# Implement the sans-IO probe engine

## Goal
Implement the scheduler and probe state machine as pure logic over the sipx UA layers, so probe runs — including their failure modes — are seeded, reproducible tests before they ever touch a socket.

## Acceptance
- [ ] The engine consumes fired timers and injected randomness; no clock, socket or sleep appears in the logic crate.
- [ ] Failing-first harness scenarios from ET-1's vectors: a clean pass; no 200 within the step timeout; 200 but the correlation marker is missing; the register step fails; the echo never rings. Each yields the specified verdict, deterministically, from a fixed seed.
- [ ] Scheduling honours interval, jitter and the configured rate bound; the target matrix is walked so each verdict names its edge address, transport and zone.
- [ ] `inconclusive` is produced — not `fail` — when the fault is on the probe side, and a harness scenario proves it.
- [ ] The run record type is shared with ET-4's API and ET-5's metrics; there is one record shape, not three.

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Runs inside the harness from [CF-1](CF-1-design-the-deterministic-cluster-harness.md).
- UA and dialog behavior comes from the sipx kernel unmodified; anything missing there is an upstream story ([upstream.md](../upstream.md)).
