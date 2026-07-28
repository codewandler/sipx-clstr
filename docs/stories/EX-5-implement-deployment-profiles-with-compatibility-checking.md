---
id: EX-5
title: Implement deployment profiles with compatibility checking
pillar: Platform
status: backlog
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions, deploy]
note: 
---

# Implement deployment profiles with compatibility checking

## Goal
Implement named, compatibility-checked module sets — CoreProxy, ModernRegistrar, CarrierInterconnect, WebSocketUA — as the deployable unit of "which SIP we speak".

## Acceptance
- [ ] A profile validates at startup against the registry: dependencies present, conflicts absent, trust-domain-bound mechanisms only where asserted.
- [ ] A deliberately conflicting profile is rejected in a test with an actionable error.

## Progress
- (not started)

## Notes
- Design: [extension-framework](../designs/extension-framework.md).
