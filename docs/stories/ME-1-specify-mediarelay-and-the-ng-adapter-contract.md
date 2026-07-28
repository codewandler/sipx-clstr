---
id: ME-1
title: Specify MediaRelay and the NG adapter contract
pillar: Media
status: backlog
priority: 
design: docs/designs/media-control.md
epic: media-control
areas: [media]
note: 
---

# Specify MediaRelay and the NG adapter contract

## Goal
Specify the platform's only view of media — the `MediaRelay` trait — and the rtpengine NG integration contract behind it.

## Acceptance
- [ ] Trait semantics for offer/answer/update/delete/query are specified, including `NullMediaRelay`'s pass-through behavior.
- [ ] The NG mapping is pinned: command set, bencode framing, cookie correlation, timeout/retransmission budget, error taxonomy (node down vs command rejected), health signals.
- [ ] A tested rtpengine baseline version is named per the AGENTS.md integration carve-out.

## Progress
- (not started)

## Notes
- Design: [media-control](../designs/media-control.md).
