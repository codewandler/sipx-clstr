---
id: RT-15
title: Populate diversion history and a reason for every routing hop
pillar: Signalling
status: backlog
priority: 16
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, history, privacy]
note: blocked by RT-9 and released sipx S-21; syntax upstream, routing facts here
---

# Populate diversion history and a reason for every routing hop

## Goal

Make routing and session-service diversion visible as structured history without leaking more than
the selected privacy policy permits.

## Acceptance

- [ ] Each retarget operation appends the RFC 7044 entry and RFC 3326 reason derived from the actual
      route decision; forwarding without retarget does not invent one.
- [ ] Indexing, ordering, escaping and history-size bounds use the released kernel types and pass
      byte-level vectors.
- [ ] Privacy policy removes or anonymizes protected entries before an untrusted egress while
      preserving a reviewable local audit result.
- [ ] The session service and proxy routing modules call the same policy surface.
- [ ] Failing-first `OB-7` proves two diversions and their reasons arrive in order.

## Progress

- Not started.
