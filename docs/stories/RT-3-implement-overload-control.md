---
id: RT-3
title: Implement overload control
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing]
note: RFC 7339 / RFC 7415
---

# Implement overload control

## Goal
Implement SIP overload control: honor rate feedback from downstream and emit it upstream when this cluster sheds load — not just `503`.

## Acceptance
- [ ] RFC 7339 negotiation and RFC 7415 rate-based control pass their vectors.
- [ ] The simulated-collapse scenario stays stable: goodput holds while offered load exceeds capacity.
- [ ] Emission ties into the transport layer's existing backpressure path.

## Progress
- (not started)

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md).
