---
id: BS-3
title: Implement a three-party conference focus through external relay control
pillar: Services
status: backlog
priority: 18
design: docs/designs/services-b2bua.md
epic: services-b2bua
areas: [services, conference, media]
note: blocked by BS-2 and a MediaRelay conference contract
---

# Implement a three-party conference focus through external relay control

## Goal

Extend the optional session service to own several SIP legs while the external relay performs all
media mixing.

## Acceptance

- [ ] `MediaRelay` gains only the portable conference operations required by `SS-10`/`SS-11`, with
      byte-level adapter vectors before driver code.
- [ ] Join, leave, DTMF policy, participant failure and whole-conference teardown pass deterministically.
- [ ] A real three-party conference carries asserted audio to every participant and removes relay
      state after the last leg ends.
- [ ] Killing the owner produces exactly `SS-12` and the published HA statement; a new conference can
      start on a healthy owner.
- [ ] The signalling/service process never decodes, mixes or forwards RTP.

## Progress

- Not started.
