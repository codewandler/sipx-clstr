---
id: ET-3
title: Implement the echo answering endpoint
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe]
note: blocked by ET-1; signalling echo only, no in-process RTP
---

# Implement the echo answering endpoint

## Goal
Provide the other end of the probe call: an endpoint that registers in the test tenant, answers probe INVITEs, and reflects the correlation marker — so a probe run traverses the real path end to end.

## Acceptance
- [ ] The echo endpoint registers as an ordinary AoR in the test tenant and answers INVITEs with 200 OK, reflecting the probe's correlation marker.
- [ ] A failing-first harness test: probe → edge → location lookup → echo → 200 with marker → BYE, asserted end to end on a seed.
- [ ] No proxy role links a UAS: the echo runs as its own role/mode per ET-1's decision, and a build check or module-graph assertion proves the separation.
- [ ] No RTP is forwarded in-process; the media assertion path is left as a documented extension point over `MediaRelay`.
- [ ] Malformed or unauthenticated calls to the echo are rejected the way any UAS would reject them — the test tenant is not a bypass.

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Media echo, when it lands, goes through [media-control](../designs/media-control.md).
