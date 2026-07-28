---
id: RG-2
title: Implement server-side digest authentication
pillar: Signalling
status: blocked
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: M1 #9 · BLOCKED on sipx S-16 — digest primitives are kernel logic
---

# Implement server-side digest authentication

## Goal
Implement the server side of digest: challenge emission, nonce minting with a replay window, and credential verification for REGISTER and proxy authentication — generic primitives upstreamed, credential store and policy here.

## Acceptance
- [ ] Challenge/verify vectors pass for the MD5 and SHA-256 families (RFC 8760), including `stale` handling and nonce-count replay rejection.
- [ ] The nonce store's replay window is bounded and tested under retransmission.
- [ ] The primitive/policy split against sipx is recorded in the upstream ledger and honored.

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
