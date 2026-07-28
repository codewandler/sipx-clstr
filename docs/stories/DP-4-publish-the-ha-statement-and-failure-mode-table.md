---
id: DP-4
title: Publish the HA statement and failure-mode table
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy]
note: 
---

# Publish the HA statement and failure-mode table

## Goal
Publish the honest availability contract: service HA guaranteed, call-survival explicitly out of scope for v1, with a per-failure-mode table.

## Acceptance
- [ ] Each failure mode has a row: what breaks, what the endpoint observes, what recovers automatically, in what bound.
- [ ] Every row is backed by a harness scenario demonstrating exactly the documented behavior — the table and the tests cannot drift.

## Progress
- (not started)

## Notes
- Design: [deployment](../designs/deployment.md).
