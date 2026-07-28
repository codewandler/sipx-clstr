---
id: AF-4
title: Implement the token mint/verify library
pillar: Cluster
status: backlog
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: blocked by AF-1
---

# Implement the token mint/verify library

## Goal
Implement AF-1 as a pure library: mint, encode, parse, verify — with key rotation.

## Acceptance
- [ ] AF-1's byte-level vectors pass exactly.
- [ ] Tampered tags, expired tokens, unknown key ids and replayed nonces are rejected, each with a test.
- [ ] Rotation works with overlapping key validity; old-key verification ends at the specified boundary.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
