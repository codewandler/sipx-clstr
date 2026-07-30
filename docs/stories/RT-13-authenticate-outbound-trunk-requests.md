---
id: RT-13
title: Authenticate outbound requests for a configured trunk
pillar: Signalling
status: backlog
priority: 14
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, auth, security]
note: blocked by RT-2, RT-12 and CX-9; policy is local, digest calculation is the released kernel's
---

# Authenticate outbound requests for a configured trunk

## Goal

Answer a trunk's 401/407 challenge from configured credentials without teaching the proxy core a
second digest implementation.

## Acceptance

- [ ] Per-trunk configuration selects a credential reference and permitted digest algorithms; secret
      material is resolved in the driver and redacted from every output.
- [ ] A challenge retries the owning branch with correct method, Request-URI, CSeq and transaction
      behavior through the released kernel digest surface.
- [ ] Stale, repeated, unsupported or invalid challenges have bounded retry and explicit branch
      outcomes; no authentication loop is possible.
- [ ] Unconfigured trunks pass the challenge upstream according to policy rather than borrowing
      another trunk's credentials.
- [ ] Failing-first `OB-7` authenticates one trunk and proves a wrong secret and algorithm downgrade
      are refused.

## Progress

- Not started.
