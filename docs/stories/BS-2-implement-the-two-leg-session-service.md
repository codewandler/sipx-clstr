---
id: BS-2
title: Implement the optional two-leg session service
pillar: Services
status: backlog
priority: 17
design: docs/designs/services-b2bua.md
epic: services-b2bua
areas: [services, call, media, affinity]
note: blocked by BS-1, CX-9, AF-7 and ME-5
---

# Implement the optional two-leg session service

## Goal

Run the `SS-1` … `SS-9` state machine as a separately enabled service using released kernel call
coupling and platform routing/relay orchestration.

## Acceptance

- [ ] The sans-IO engine passes `SS-1` … `SS-9` under virtual time with exact effects.
- [ ] The driver owns sockets, timers, routes, credentials and relay I/O; the engine owns no I/O.
- [ ] Service-disabled builds and deployments retain the proxy path and dependency set unchanged.
- [ ] A real two-leg call proves early media, bidirectional audio, DTMF, re-INVITE and teardown.
- [ ] No media packet enters the service process and no global dialog lookup enters the hot path.

## Progress

- Not started.
