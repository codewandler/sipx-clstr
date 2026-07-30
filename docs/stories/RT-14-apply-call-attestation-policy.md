---
id: RT-14
title: Apply ingress verification and egress call-attestation policy
pillar: Signalling
status: backlog
priority: 15
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, identity, security]
note: blocked by RT-2 and released sipx S-20; cryptography upstream, trust/routing policy here
---

# Apply ingress verification and egress call-attestation policy

## Goal

Bind generic signing and verification to explicit tenant/trunk trust policy and routing outcomes.

## Acceptance

- [ ] The trunk spec defines verification, failure, attestation-level and re-sign policy with no
      security-weakening default.
- [ ] Certificate/key retrieval is asynchronous driver I/O with bounded caches; signature parsing and
      cryptography reuse the released kernel.
- [ ] Invalid inbound identity follows the configured RFC response, and an unsigned path cannot be
      reported as verified.
- [ ] Diversion or dialog termination that changes the calling identity triggers the declared
      re-attestation rule before egress.
- [ ] Metrics distinguish absent, verified, failed and signed without logging tokens or private keys.
- [ ] Failing-first `OB-7` covers accepted, invalid and re-signed calls.

## Progress

- Not started.
