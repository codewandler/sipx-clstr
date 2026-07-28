---
id: RG-2
title: Implement server-side digest authentication
pillar: Signalling
status: blocked
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: M1 #9 · BLOCKED on a sipx release carrying S-16 + X-20 — both are written, neither is tagged
---

# Implement server-side digest authentication

## Goal
Implement the server side of digest: challenge emission, nonce minting with a replay window, and credential verification for REGISTER and proxy authentication — generic primitives upstreamed, credential store and policy here.

## Acceptance
- [ ] Challenge/verify vectors pass for the MD5 and SHA-256 families (RFC 8760), including `stale` handling and nonce-count replay rejection.
- [ ] The nonce store's replay window is bounded and tested under retransmission.
- [ ] The primitive/policy split against sipx is recorded in the upstream ledger and honored.

## Progress
**Not started, and the blocker has moved but not lifted.** Both kernel pieces now exist; neither is
in a tag this workspace can pin.

- **`S-16` is implemented upstream.** `sipx-ua::challenge` provides `Authenticator` (self-describing
  nonces — `<issued-at>.<HMAC over it and the realm>`, so there is no table of issued nonces),
  `Presented::from_request`, `Verdict`, `stale`, and a bounded replay window. Its acceptance names
  the same retransmission-versus-replay distinction M1's exit criterion does. It is on the kernel's
  `main`, after the `v0.3.0` tag.
- **`X-20` is implemented upstream**, because `S-16` alone was not enough: `sipx-ua` pulled `tokio`
  and `sipx-transport` unconditionally, so taking the authenticator meant linking an async runtime
  into a sans-IO crate. `sipx-ua = { default-features = false }` now yields `auth`, `challenge`,
  `outbound` and `registrar` with no runtime in the resolved graph.

**What is actually needed to unblock:** a sipx release — the S-16 and X-20 commits pushed to
`origin` and carried by a tag — after which this workspace bumps its `sipx-*` pins past `v0.2.1`
and adds `sipx-ua` with `default-features = false` to `sipx-clstr-registrar`.

**Not worked around, deliberately.** Two shortcuts were available and both were refused:

- *Write the digest verification here.* It would have closed the story today and contradicted the
  design's own split ("primitives upstreamed, credential store and policy here") plus the
  `AGENTS.md` upstream-first rule. Two implementations of one algorithm eventually disagree about
  who is authenticated, and the one that disagrees quietly is a security bug.
- *`[patch]` the dependency at a local checkout.* That makes the build unreproducible and hides the
  dependency from the ledger that exists to track it.

**What remains for this story once the pin moves** is the policy half, which is genuinely this
repo's: the credential store behind `S-16`'s password argument, which tenants require
authentication and in which realm, the authenticated principal recorded on the binding, and the
harness scenario proving a retransmitted REGISTER with an unchanged nonce-count still authenticates.
That last one is why verification has to be reachable from the sans-IO side at all — if the crypto
lived in the node, the deterministic harness could not assert M1's exit criterion, only observe it.

## Notes
- Design: [registrar-location](../designs/registrar-location.md). Ledger:
  [upstream](../upstream.md) — the `S-16` and `X-20` rows.
- The nonce-store scope question the design leaves open (per-edge versus shared) is settled by
  `S-16`'s construction: a nonce is verifiable from the key and the realm alone, so any edge holding
  the key recognises any other edge's nonce. Only the replay window is per-node, and a nonce-count
  replayed at a *different* edge is the case that window does not catch — which is a real limit to
  record here rather than a hole to discover in M2.
