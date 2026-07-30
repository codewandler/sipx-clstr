---
id: AF-4
title: Implement the token mint/verify library
pillar: Cluster
status: in-progress
priority: 1
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: AF-1 is done — unblocked; M2 critical path, next after PX-13
---

# Implement the token mint/verify library

## Goal
Implement AF-1 as a pure library: mint, encode, parse, verify — with key rotation.

## Acceptance
- [ ] AF-1's byte-level vectors pass exactly.
- [ ] Tampered tags, expired tokens and unknown key ids are rejected, each with a test; verification is stateless — legitimate re-presentation of the same token on every mid-dialog request verifies every time, and no replay store exists (per AF-1's replay semantics).
- [ ] Rotation works with overlapping key validity; old-key verification ends at the specified boundary.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
