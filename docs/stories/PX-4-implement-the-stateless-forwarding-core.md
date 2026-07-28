---
id: PX-4
title: Implement the stateless forwarding core
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: M2 — implementation deferred until the token path gives it a consumer; blocked by PX-1, PX-3; UPSTREAM — see docs/upstream.md
---

# Implement the stateless forwarding core

## Goal
Implement the §16.11 subset: validate, preprocess routing information, rewrite, forward, and pass responses through — as a pure engine plus driver glue.

## Acceptance
- [ ] PX-1's stateless-mode vectors pass under the deterministic harness.
- [ ] Byte-exact passthrough of all untouched message material is asserted (the kernel's lossless guarantee, preserved through forwarding).

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
