---
id: AF-6
title: Design config-first membership and key distribution
pillar: Cluster
status: backlog
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, deploy]
note: feeds DP-1 — this story owns the membership/key schema sections
---

# Design config-first membership and key distribution

## Goal
Keep v1 free of consensus: the node set, shard map and token keys come from validated, reloadable configuration.

## Acceptance
- [ ] This story owns the membership/key schema sections; DP-1 integrates them unchanged into 
the full config schema (AF-6 first, DP-1 second — the circular reference is resolved this way).
- [ ] Reload without restart is specified and tested; the key-rotation runbook (overlap window, cutover) is documented.
- [ ] The design records what a future dynamic membership service would replace, so nothing here paints it out.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
