---
id: AF-5
title: Round-trip tokens through Record-Route and Route
pillar: Cluster
status: backlog
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, proxy]
note: blocked by AF-4, PX-5; UPSTREAM: Path header, sipx T-14 — see docs/upstream.md
---

# Round-trip tokens through Record-Route and Route

## Goal
Carry the token through the proxy: minted into Record-Route on dialog-forming requests, verified from Route on mid-dialog requests — at any edge, with zero lookups.

## Acceptance
- [ ] A dialog-forming request through edge A yields a Record-Route token; the mid-dialog request arriving at edge B routes on it with the cross-node lookup counter at zero (harness scenario).
- [ ] The Path variant lands once the upstream typed Path header exists.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md). Blocked by AF-4, PX-5.
