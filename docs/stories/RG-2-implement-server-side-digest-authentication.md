---
id: RG-2
title: Implement server-side digest authentication
pillar: Signalling
status: in-progress
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: M1 #9 · unblocked by sipx v0.4.0 — the spec and the decision core are in; the store wiring is not
---

# Implement server-side digest authentication

## Goal
Implement the server side of digest: challenge emission, nonce minting with a replay window, and credential verification for REGISTER and proxy authentication — generic primitives upstreamed, credential store and policy here.

## Acceptance
- [x] Challenge/verify vectors pass for the MD5 and SHA-256 families (RFC 8760), including `stale` handling and nonce-count replay rejection. — `RA-A-1…4` (algorithms and the downgrade refusal), `RA-D-8` (`stale`), `RA-R-2`/`RA-R-4` (replay, and a count that goes backwards).
- [x] The nonce store's replay window is bounded and tested under retransmission. — `RA-R-1` (a retransmission is not a replay) and `RA-R-5` (the window does not grow with traffic).
- [x] The primitive/policy split against sipx is recorded in the upstream ledger and honored. — [upstream](../upstream.md) rows `S-16`/`X-20` are `landed in 0.4.0`; the split is normative in [registrar-auth](../specs/registrar-auth.md) §2, and `tests/sans_io.rs` enforces the half that could regress silently.
- [ ] Authentication runs **before** REGISTER processing and the principal it yields reaches the binding: `auth::Decision` is consumed on the path into `RegisterCommand`, and a binding written under an open tenant carries `principal: None` as a recorded fact.
- [ ] A harness scenario proves the retransmission case end to end — a REGISTER replayed with an unchanged nonce-count still authenticates — which is M1's fifth exit criterion and the reason verification had to be reachable from the sans-IO side at all.

## Progress
**Unblocked 2026-07-29 by sipx `v0.4.0`, and the decision core is in.** The release carries both
kernel pieces this story waited on, so the remedy the previous entry insisted on — a tag, not a
`[patch]` — is the one that happened.

- **The pins moved.** `sipx-sip`, `sipx-transport` and `sipx-sdp` go `v0.2.1` → `v0.4.0`, and
  `sipx-ua` joins them at `default-features = false`. That flag is the whole of `X-20`'s value and
  it was omitted on the first pass while the comment above it claimed otherwise, which put
  `sipx-transport` and `tokio` back in the registrar's graph. `tests/sans_io.rs` now walks
  `Cargo.lock` rather than reading a manifest, because the rule became transitive the moment a
  kernel crate entered this crate — and a transitive rule is one manifest text cannot check.
- **[registrar-auth](../specs/registrar-auth.md) is accepted**, prefix `RA`, four families:
  the decision order (§3), algorithm selection under RFC 8760 (§4), replay versus retransmission
  (§3, §6), and the tenant boundary (§5). `scripts/check-vectors.py` knows the prefix.
- **`registrar::auth` implements the policy half**: `CredentialStore` (a trait, because credentials
  are a deployment's business — the same reason `S-16`'s `verify` takes a password rather than
  looking one up), `InMemoryCredentials`, `TenantAuth`, `Decision`. Sans-IO like its neighbours:
  `now` is an argument and the nonce secret is supplied rather than drawn, so a harness scenario
  replays byte for byte from its seed.
- **22 RA vectors pass**, and they answer challenges with the kernel's *client* responder rather
  than with fixtures written here — so the two halves of digest are proved to agree with each
  other, not each with a local guess. `RA-D-6` is the one worth naming: an unknown user is
  indistinguishable from a wrong password, via a placeholder credential that keeps §3 A4's "same
  path" literal instead of aspirational.

**What remains** is the wiring, which is the last two acceptance boxes: `Decision` is not yet
consumed on the path into `RegisterCommand`, so nothing currently sets the `principal` that
`binding.rs:125` and `command.rs:68` already carry, and there is no harness scenario for the
retransmission case. That scenario is M1's fifth exit criterion, and it is why verification had to
be reachable from the sans-IO side at all — if the crypto lived in the node, the deterministic
harness could only observe the criterion, never assert it.

**Kept from the blocked period, because the reasoning still binds:** writing digest here was
refused (it contradicts the design's primitive/policy split and the `AGENTS.md` upstream-first
rule — two implementations of one algorithm eventually disagree about who is authenticated, and
the one that disagrees quietly is a security bug), and so was `[patch]`ing to a local checkout
(unreproducible builds, and it hides the dependency from the ledger that exists to track it).

## Notes
- Design: [registrar-location](../designs/registrar-location.md). Ledger:
  [upstream](../upstream.md) — the `S-16` and `X-20` rows.
- The nonce-store scope question the design leaves open (per-edge versus shared) is settled by
  `S-16`'s construction: a nonce is verifiable from the key and the realm alone, so any edge holding
  the key recognises any other edge's nonce. Only the replay window is per-node, and a nonce-count
  replayed at a *different* edge is the case that window does not catch — which is a real limit to
  record here rather than a hole to discover in M2.
