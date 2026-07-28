---
id: ME-5
title: Implement the media-anchoring module
pillar: Media
status: backlog
priority:
design: docs/designs/media-control.md
epic: media-control
areas: [media, extensions]
note: blocked by ME-4, ME-2, EX-3
---

# Implement the media-anchoring module

## Goal
Implement the extension module ME-4 designs: intercept offers and answers at the declared hook
phases, drive `MediaRelay`, and apply the per-call-class ICE stance — the code that actually
anchors media.

## Acceptance
- [ ] Offers and answers on anchored calls are rewritten via `MediaRelay` at exactly the phases
ME-4 chose; the module's manifest declares them.
- [ ] The ICE stance is enforced: on anchored calls the relay participates in or strips ICE —
a harness scenario shows an ICE-capable endpoint cannot negotiate around the relay.
- [ ] Media-direct calls bypass the module entirely — ME-4's stated requirement, asserted here
in a harness scenario.
- [ ] Re-INVITE and UPDATE reach the same media node via the token's node id (with ME-3).

## Progress
- (not started)

## Notes
- Design: [media-control](../designs/media-control.md). This story exists so the module ME-4
designs has an implementer; the epic's M2 exit (relayed media surviving re-INVITE) lands here.
