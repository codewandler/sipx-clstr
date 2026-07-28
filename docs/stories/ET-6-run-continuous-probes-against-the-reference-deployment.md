---
id: ET-6
title: Run continuous probes against the reference deployment
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe, deploy]
note: blocked by ET-3, ET-4, DP-2
---

# Run continuous probes against the reference deployment

## Goal
Prove the probe on a real deployment: continuous test calls through the 3-zone reference topology (and the local k3s environment), detecting deliberately broken listeners within a bounded time.

## Acceptance
- [ ] The `e2e-tester` role runs in the reference topology dialling every edge address and transport on a schedule, with results visible in the metric stack.
- [ ] A killed listener, a stopped edge and an expired/invalid TLS certificate each produce a failing verdict naming the right target within the documented detection bound; the platform's internal metrics alone are shown not to catch at least one of them.
- [ ] The probe passes continuously against a healthy deployment over a soak window without false failures, and the run is recorded.
- [ ] The same probe configuration works against the local k3s environment shipped by [KO-2](KO-2-ship-the-helm-chart-for-a-local-k3s-environment.md).

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Topology: [DP-2](DP-2-author-the-3-zone-reference-topology.md). Real-network harness: [CF-3](CF-3-build-the-real-network-interop-harness.md).
