---
id: PX-2
title: Design the proxy transaction driver
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, transport]
note: decided: lives here, not upstream
---

# Design the proxy transaction driver

## Goal
Design the driver that fans one server transaction out to N client transactions directly over sipx-sip's `TransactionLayer`, reusing sipx-transport's pool and resolution machinery where crate boundaries allow.

## Acceptance
- [ ] The design covers per-branch destinations and failure handling, CANCEL wiring via `TransactionKey::for_cancelled_invite`, ownership (no locks on the signalling path), and backpressure.
- [ ] The crate boundary against sipx-transport is decided: what imports, what is extracted, what is reimplemented — with rationale in the design doc.
- [ ] The driver contract is expressible sans-IO so PX-1 vectors drive it in the harness.

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md). Blocked by PX-1.
