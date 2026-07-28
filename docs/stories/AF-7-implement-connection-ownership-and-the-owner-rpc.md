---
id: AF-7
title: Implement connection ownership and the owner RPC
pillar: Cluster
status: backlog
priority:
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, transport]
note: blocked by AF-2, AF-3 — implements both
---

# Implement connection ownership and the owner RPC

## Goal
Implement what AF-2 specifies and AF-3 designs: the per-edge connection table, flow_ref minting
and resolution, and the connection-owner RPC that delivers requests to the edge owning a
client's connection — the epic's only cross-node signalling hop, built.

## Acceptance
- [ ] The connection table tracks AF-2's schema; flow_refs mint, verify, and invalidate on
generation bump (reconnect), each with a test.
- [ ] The owner RPC delivers per AF-3's semantics — at-most-once, bounded queueing — and its
failure taxonomy (owner unreachable ≠ flow dead ≠ flow rejected) surfaces as distinct outcomes
in a harness scenario.
- [ ] A cross-node delivery to a connection-bound client succeeds in the multi-node harness, and
the owner's loss produces the specified temporarily-unavailable outcome, not a hang.

## Progress
- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md). This story exists so the epic's
done-criteria have an implementer; roadmap M2's "flow_ref and the connection-owner RPC" lands
here.
