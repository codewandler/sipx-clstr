---
id: BS-1
title: Accept the optional session-service specification
pillar: Services
status: ready
priority: 4
design: docs/designs/services-b2bua.md
epic: services-b2bua
areas: [services, call, media]
note: M4 spec-first story; no runtime implementation
---

# Accept the optional session-service specification

## Goal

Review and settle the state, ownership, media and failure contract before a dialog-terminating
service is implemented.

## Acceptance

- [ ] `session-service.md` names every state, input, effect and timer needed by `SS-1` … `SS-12`.
- [ ] CANCEL, early media, offer relay, glare, BYE, relay failure, conference membership and owner
      loss each have one normative outcome.
- [ ] The upstream boundary is checked against the qualifying kernel surface and every missing
      generic primitive has a ledger row/story.
- [ ] The HA statement agrees that M4 preserves service availability for new sessions, not an
      established session after owner loss.
- [ ] The design is accepted after cross-review with proxy, affinity and media-control specs; no code
      lands in this story.

## Progress

- Draft normative spec filed; review not started.
