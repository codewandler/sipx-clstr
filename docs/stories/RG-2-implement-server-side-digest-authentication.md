---
id: RG-2
title: Implement server-side digest authentication
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: M1 #9 · the seam, the driver wiring and the harness scenario are in; RG-8 carries what it found
---

# Implement server-side digest authentication

## Goal
Implement the server side of digest: challenge emission, nonce minting with a replay window, and credential verification for REGISTER and proxy authentication — generic primitives upstreamed, credential store and policy here.

## Acceptance
- [x] Challenge/verify vectors pass for the MD5 and SHA-256 families (RFC 8760), including `stale` handling and nonce-count replay rejection. — `RA-A-1…4` (algorithms and the downgrade refusal), `RA-D-8` (`stale`), `RA-R-2`/`RA-R-4` (replay, and a count that goes backwards).
- [x] The nonce store's replay window is bounded and tested under retransmission. — `RA-R-1` (a retransmission is not a replay) and `RA-R-5` (the window does not grow with traffic).
- [x] The primitive/policy split against sipx is recorded in the upstream ledger and honored. — [upstream](../upstream.md) rows `S-16`/`X-20` are `landed in 0.4.0`; the split is normative in [registrar-auth](../specs/registrar-auth.md) §2, and `tests/sans_io.rs` enforces the half that could regress silently.
- [x] Authentication runs **before** REGISTER processing and the principal it yields reaches the binding: `auth::Decision` is consumed on the path into `RegisterCommand`, and a binding written under an open tenant carries `principal: None` as a recorded fact. — `parse::admit` is the only door; `EdgeContext` no longer has a `principal` field to smuggle one through. Proved by `parse`'s seam tests and by `register_auth`'s `a_challenged_register_authenticates_and_binds_under_its_principal` / `an_open_tenant_binds_with_no_principal_recorded`.
- [x] A harness scenario proves the retransmission case end to end — a REGISTER replayed with an unchanged nonce-count still authenticates — which is M1's fifth exit criterion and the reason verification had to be reachable from the sans-IO side at all. — `crates/sipx-clstr-sim/tests/register_auth.rs`, `ra_r_1_a_retransmitted_register_authenticates_again`, with `ra_r_2_…` as the twin that keeps it from passing vacuously. **It also found something** — see below.

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

**The wiring landed 2026-07-29, and the scenario with it.** All five boxes hold; the gate is green.

- **`parse::admit` is the seam.** It runs `TenantAuth::decide` and, only on `Proceed`, builds the
  command with the principal that decision produced — returning `Admission::{Command, Challenge,
  Reject}`. Three outcomes rather than a `Result`, because a challenge is not a failure: it is the
  first half of a round trip the client is expected to finish.
- **`EdgeContext::principal` is gone.** An identity is not something an edge *knows*, it is what a
  decision *produced*, and a settable field would have let a driver assert one nobody proved — the
  single mistake in this area no downstream test could catch. `register_command` remains, and is now
  explicitly the open-tenant path: it yields `principal: None`.
- **`Timestamp::as_secs`** is the clock seam — the location service counts nanoseconds, digest counts
  seconds. Truncating, so a nonce expires a moment late rather than a moment early and nobody is
  told `stale` for a nonce that was still good.
- **The node driver authenticates too**, via `NodeConfig::auth` (`AuthConfig`: realm, nonce secret,
  credentials), open by default. One `Mutex<TenantAuth>` for the process, because the replay window
  is the thing it holds and a per-request authenticator is a window that never says no. Scope note:
  this is beyond the box's letter, but a registrar that could never be *configured* to challenge
  would make M1's exit a claim about the harness rather than about the platform.
- **The harness scenario is `crates/sipx-clstr-sim/tests/register_auth.rs`**, eight cases: the
  challenge/answer round trip, the principal reaching the stored binding, RA-R-1's retransmission,
  RA-R-2's forged twin, RA-D-4's foreign realm refused 403, a wrong password binding nothing, an open tenant recording `None`, and a
  byte-for-byte replay sweep under jitter. The phone's half is `sipx_ua::auth::respond` — the
  kernel's own client — so what it proves is that the two halves agree.

## What the scenario found, and did not fix

**A retransmitted REGISTER authenticates and is then refused `500` by the location service.** The
`RG-2` half is correct: same nonce, same nonce-count, same digest, admitted twice. What answers it is
[location-service](../specs/location-service.md) §5.3 B5, because B4's idempotency test —
`process::already_holds` — reads "same granted expiry base" as the same **absolute deadline**. That
is true only for a retry arriving at the very nanosecond of the original, which no retransmission
ever does, so every one of them falls through to B5.

This is [`RG-3`](RG-3-implement-register-processing-on-the-in-memory-store.md)'s recorded open
question, pinned there by `a_re_presentation_at_a_later_instant_is_not_a_retry_and_is_refused` and
deferred to `AF-*`/`RG-5` as a *cluster* concern — a re-presentation at another node stamping its own
`now`. **It is not only that.** One node, one phone, no cluster, and the plainest event on a UDP
network reaches it. Left unchanged here rather than improvised over: reversing a decision that is on
the record belongs to the story that owns it, and it is a **spec** decision — §5.3 is normative, so
the reading changes there before the code does.

**Tracked as [`RG-8`](RG-8-settle-b4-idempotency-so-a-retransmission-is-a-retry.md)**, priority 1,
`ready`. `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule` pins the current
answer meanwhile, so the defect is something a build can fail on rather than a paragraph nobody
reads.

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
