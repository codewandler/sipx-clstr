---
id: ET-5
title: Publish probe results as metrics and alerts
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe, deploy]
note: blocked by ET-2; joins the DP-3 metric set
---

# Publish probe results as metrics and alerts

## Goal
Turn verdicts into the outside-view half of the observability story: success ratio and step latency per edge, transport and zone, with alerts that localize a fault instead of announcing one.

## Acceptance
- [ ] Metrics exist for probe outcome (pass/fail/inconclusive) and per-step latency, labelled by probe name, edge address, transport and zone, and are documented alongside the DP-3 invariant metrics.
- [ ] `inconclusive` runs are counted separately and never contribute to the failure ratio that alerts fire on.
- [ ] Shed probes (RT-3 overload control) are recorded as shed, distinguishable from both success and failure.
- [ ] Alerting rules fire on consecutive failures against a single target and are documented with their thresholds and the reasoning behind them; a single lost packet does not page.
- [ ] The last verdict is exported in a form the operator's `SipxCluster` status can consume.

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Metric set: [DP-3](DP-3-implement-observability-that-proves-the-invariants.md).
- These signals are also the invariant gate for autoscaling ([KO-6](KO-6-implement-autoscaling-with-drain-aware-scale-in.md)).
