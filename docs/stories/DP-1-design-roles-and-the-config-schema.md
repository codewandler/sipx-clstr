---
id: DP-1
title: Design roles and the config schema
pillar: Cluster
status: ready
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [deploy]
note: 
---

# Design roles and the config schema

## Goal
Design the typed, versioned configuration: role selection (edge / registrar / inbound-proxy / outbound-proxy), membership, shard map, token keys, trunks — with a defined reloadable subset.

## Acceptance
- [ ] The schema validates at startup with precise errors; one binary boots into any role combination.
- [ ] The reloadable subset (trunks, keys, shard map) is defined with drain-then-switch semantics for the shard map.
- [ ] AF-6's membership/key sections are integrated, not duplicated.

## Progress
- (not started)

## Notes
- Design: [deployment](../designs/deployment.md).
- The role set also carries `e2e-tester` ([ET-1](ET-1-specify-the-e2e-tester-role-and-probe-contract.md)) — a probe role, never on the call path.
- This schema is the single source for the operator's `SipxCluster` spec ([KO-1](KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md)); a second dialect is a defect.
