---
id: ME-3
title: Implement media-node selection and reselection
pillar: Media
status: ready
priority: 3
design: docs/designs/media-control.md
epic: media-control
areas: [media, affinity]
note: unblocked — ME-1, AF-1 and AF-4 are all done, so the node id has a token to ride in; M2 #14
---

# Implement media-node selection and reselection

## Goal
Select a media node once per call by rendezvous hashing over `tenant || Call-ID || initial From-tag`, carry it in the affinity token, and re-anchor honestly on node failure.

## Acceptance
- [ ] Selection is deterministic and balanced (distribution test); the node id round-trips through the token so any edge addresses the same relay.
- [ ] Node failure triggers re-anchoring on the next offer/answer — and the test asserts exactly that semantics, no silent continuity claim.

## Progress
- (not started)

## Notes
- Design: [media-control](../designs/media-control.md).
