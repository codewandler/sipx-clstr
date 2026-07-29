---
id: PX-4
title: Implement the stateless forwarding core
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: M2 — implementation deferred until the token path gives it a consumer; S-15 landed in v0.4.0, PX-1 is done, so PX-3 is the only thing left in front of it
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
