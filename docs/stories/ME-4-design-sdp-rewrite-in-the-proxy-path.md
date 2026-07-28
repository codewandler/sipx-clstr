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
Design media anchoring as an extension module: which hook phases see offers and answers, the latching stance, and the ICE stance — anchored calls cannot leave ICE untouched (see the design); pass-through is only for media-direct calls.

## Acceptance
- [ ] The hook phases for offer/answer interception are chosen against EX-1's spec.
- [ ] The ICE stance is decided per call class: anchored calls have the relay participate in 
ICE or strip it; untouched pass-through only for media-direct calls. Latching decided with 
rationale.
- [ ] Media-direct calls bypass the module entirely — stated here as a requirement; ME-5's 
harness scenario asserts it.

## Progress
- (not started)

## Notes
- Design: [media-control](../designs/media-control.md).
