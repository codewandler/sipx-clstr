---
id: DP-3
title: Implement observability that proves the invariants
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy]
note: 
---

# Implement observability that proves the invariants

## Goal
Ship metrics that prove the architecture, not decorate it: an invariant metric that moves is a bug.

## Acceptance
- [ ] The invariant metrics exist: cross-node dialog-lookup counter (must read zero), token verification failures by cause, flow-RPC delivery outcomes, per-trunk breaker state, per-shard registration write latency.
- [ ] Structured SIP event logs and traces correlate a call across nodes.
- [ ] Alerting is defined on the invariant metrics.

## Progress
- (not started)

## Notes
- Design: [deployment](../designs/deployment.md).
