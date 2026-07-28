---
id: EX-6
title: Design an async external routing hook
pillar: Platform
status: backlog
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [hooks, routing]
note: blocks a downstream deployment's parity milestone; a blocking HTTP call on the INVITE path today
---

# Design an async external routing hook

## Goal
Give the hook framework a phase in which an external service can influence routing — selecting egress and rewriting identity — without blocking the transaction.

## Acceptance
- [ ] A hook phase on the outbound path may consult an external service asynchronously.
- [ ] Timeout, and the behaviour on client error, server error and timeout, are declared per hook, not coded.
- [ ] The transaction's timers and capacity are not coupled to the external service's latency.
- [ ] Failure semantics are specified: which outcomes fail the call, which proceed with a declared default.
- [ ] Harness scenarios cover slow, failing and flapping external services.

## Progress
- (not started)

## Notes
- One deployment performs a synchronous HTTP lookup on every outbound INVITE to select the carrier pool and caller-ID; a 4xx fails the call and a 5xx is tolerated.
- For some pools this lookup is the *only* selection mechanism, so it cannot simply be removed.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-2**). The evidence sits in that deployment's own reference material.
