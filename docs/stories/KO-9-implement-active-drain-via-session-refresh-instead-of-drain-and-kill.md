---
id: KO-9
title: Implement active drain via session refresh instead of drain-and-kill
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy, affinity, media]
note: blocked by KO-4; mechanisms land with M3 session timers and RFC 5626 flows
---

# Implement active drain via session refresh instead of drain-and-kill

## Goal
Make draining a node *active*: instead of waiting out a bounded window and then terminating
(killing whatever is left), the drain migrates work off the node using the protocol's own
refresh machinery — so a drain ends because the node is empty, not because a timer expired.

A proxy cannot originate in-dialog requests, so "re-INVITE drain" means exploiting refreshes
that already pass through: session-timer re-INVITEs (RFC 4028, M3) give every live call a
periodic transit through a healthy edge — the anchoring module re-anchors media off a draining
relay and reissues Record-Route tokens off a draining shard/edge at exactly that moment.
Signalling flows drain by re-homing: a draining edge stops accepting, then closes RFC 5626
flows gracefully so clients flow-recover to another edge immediately instead of at
registration expiry.

## Acceptance
- [ ] A draining edge closes owned flows in bounded batches; clients re-register elsewhere via
flow recovery, and the location bindings' flow_refs move — observed in a harness scenario, no
registration lost.
- [ ] During a drain, session-timer refreshes passing through re-anchor media away from a
draining rtpengine and mint fresh tokens that exclude the draining node; the call continues —
asserted end-to-end in the harness.
- [ ] The operator's drain stage (KO-4) uses activity, not only time: it terminates when owned
flows and in-flight transactions reach zero, with the bounded window as the fallback, and
reports which of the two ended the drain.
- [ ] Calls whose endpoints support no session timers are the documented residue: they ride out
the fallback window and their fate matches the DP-4 failure table — no silent kill without a
table row.

## Progress
- (not started)

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md) (drain machinery,
KO-4) with the mechanisms owned by M3 (RFC 4028 session timers, RFC 5626 flows), ME-3/ME-5
(re-anchoring) and AF-1 (token reissue on refresh).
- Upstream-first check: refresh/flow mechanics are protocol work already scoped to sipx-clstr's
M3 modules or the kernel; this story only orchestrates them from the operator — stays here.
