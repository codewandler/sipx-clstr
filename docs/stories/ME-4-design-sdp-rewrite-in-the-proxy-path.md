---
id: ME-4
title: Design SDP rewrite in the proxy path
pillar: Media
status: backlog
priority: 
design: docs/designs/media-control.md
epic: media-control
areas: [media, extensions]
note: 
---

# Design SDP rewrite in the proxy path

## Goal
Design media anchoring as an extension module: which hook phases see offers and answers, the latching stance, and how ICE material passes through untouched.

## Acceptance
- [ ] The hook phases for offer/answer interception are chosen against EX-1's spec.
- [ ] The latching and ICE pass-through stance is decided with rationale.
- [ ] Media-direct calls bypass the module entirely — asserted in a harness scenario.

## Progress
- (not started)

## Notes
- Design: [media-control](../designs/media-control.md).
