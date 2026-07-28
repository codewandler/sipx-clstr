---
id: DP-2
title: Author the 3-zone reference topology
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy]
note: 
---

# Author the 3-zone reference topology

## Goal
Author the reference deployment: three zones, edges, the HA PostgreSQL location store, routing instances, media nodes, DNS SRV discovery — and its Kubernetes expression that respects SIP.

## Acceptance
- [ ] Manifests express host networking or a source-preserving L4 dataplane for UDP, pinned long-lived flows, PodDisruptionBudgets with connection draining, and media nodes on dedicated hosts.
- [ ] The exposure table (UDP/TCP 5060, TLS 5061, explicit WS/WSS, media ranges, private management) is documented.
- [ ] Bare-metal/systemd remains a first-class documented path for media nodes.

## Progress
- (not started)

## Notes
- Design: [deployment](../designs/deployment.md).
- These manifests stay the readable reference of what the operator generates ([k8s-deployment-operator](../designs/k8s-deployment-operator.md)); packaging and reconciliation live there, topology and SIP constraints live here.
