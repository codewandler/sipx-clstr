---
id: KO-7
title: Operate the rtpengine media pool in managed and external modes
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, media, deploy]
note: managed pool makes the local demo self-contained; external pool for production
---

# Operate the rtpengine media pool in managed and external modes

## Goal
Let one `SipxCluster` express its media pool either way: **managed** — the operator deploys and operates rtpengine so the Helm + local-k3s demo is self-contained — or **external** — the pool is declared and validated but never touched, as production deployments on dedicated hosts require.

## Acceptance
- [ ] The CR expresses the mode per pool; switching a pool from managed to external (and back) changes nothing else in the config, and one cluster can hold both kinds at once.
- [ ] Managed mode: rtpengine pods run with host networking and the declared RTP port range, the NG control endpoint is reachable only on the private network (asserted by a NetworkPolicy test), and readiness is gated on NG actually answering — not on the container starting.
- [ ] Pool membership is published into node config so media-node selection by rendezvous hash sees exactly the nodes that exist; adding or removing a managed node converges without restarting signalling nodes.
- [ ] External mode: the operator validates endpoint reachability, advertised port ranges and feature compatibility, publishes membership, reports it in status — and creates, restarts or scales nothing. A test asserts the operator issues no writes against an external pool.
- [ ] Draining a managed media node waits for anchored sessions to reach zero within a configured grace window; the decided policy after the window expires (hold, or force with a recorded event) is implemented and documented.
- [ ] The local k3s demo passes a call with media through the managed pool end to end, asserted by two sipx CLI phones and a passing `e2e-tester` probe.
- [ ] No signalling process links media in either mode — the `MediaRelay` NG contract is the only interface, and this is checked, not assumed.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Relay contract: [ME-1](ME-1-specify-mediarelay-and-the-ng-adapter-contract.md); node selection: [ME-3](ME-3-implement-media-node-selection-and-reselection.md).
- rtpengine is an integration target here, never a behavioral precedent.
