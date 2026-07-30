---
id: CX-5
title: File the nonce-uniqueness defect upstream and make nonce uniqueness normative
pillar: Platform
status: in-progress
priority: 2
design:
epic:
areas: [upstream, registrar, security]
note: DELIBERATELY OPEN — RA-R-8 is deferred to this story; closing it orphans the row. Was: the nonce is a function of the clock alone, so honest users collide in the replay window
---

# File the nonce-uniqueness defect upstream and make nonce uniqueness normative

## Goal

Get the kernel's nonce to carry per-challenge entropy, and add the rule and the vector row that stop
the defect coming back. Today two clients challenged in the same second receive the byte-identical
nonce and share one replay counter, so a correct credential is refused as a replay.

## Acceptance

- [ ] The defect is filed against [sipx](https://github.com/codewandler/sipx) with a reproduction,
      and recorded in [upstream.md](../upstream.md). `sipx-ua`'s `challenge::mint` builds
      `<issued-at in hex>.<HMAC of it>` — a pure function of the second, the realm and the secret,
      with nothing per-challenge. It is kernel surface by
      [AGENTS.md](../../AGENTS.md) #6: nonce minting is an auth primitive, not platform orchestration.
- [x] A normative rule lands in [registrar-auth](../specs/registrar-auth.md) §6 requiring a nonce to
      be unique per challenge, not merely unforgeable and expiry-checkable. §6 today says only that
      "a nonce is verifiable from the secret and the realm alone" and analyses cross-*edge* collision,
      which is the same property one axis away — it misses two clients on one edge.
- [x] An `RA-R` row covers it: two clients answer the same nonce at the same `nc`, and the second is
      **not** refused as a replay. §8's table has no such row, which is exactly why all 24 `RA` rows
      pass while the defect is live.
- [ ] **Failing-first**: a harness scenario in `sipx-clstr-sim` challenges two users within one
      simulated second and requires both to authenticate. It fails today.
- [ ] `crates/sipx-clstr-registrar/tests/vectors_register_auth.rs`'s
      `assert_eq!(value, same_nonce, "the fixture needs one nonce, not two")` is revisited. It pins
      nonce *equality* across two separately-constructed authenticators at one timestamp, which only
      holds because the nonce has no entropy — so the fixture will break when the defect is fixed, and
      it should break in a way that reads as intended rather than as a regression.
- [ ] `auth.rs`'s claim of "a **fresh** nonce every time, including on a refusal" is made true or
      corrected. It calls `challenge_at(stale, now)`, which is identical for the whole second, so a
      refusal re-offers the nonce it just refused.

## Progress

**Pass 1 — the specification half. The story stays open; the kernel half is not done.**

- **Verified before writing anything normative.** `sipx-ua::challenge::Authenticator::mint` at the
  pinned `v0.7.0` is `<issued-at in hex>.<HMAC-SHA-256(issued-at ":" realm)>`
  (`sipx-ua/src/challenge.rs:287,294`), `now` is whole seconds (`challenge.rs:440`), and the replay
  window is a `VecDeque` keyed on the nonce string holding one count and one response per nonce
  (`challenge.rs:194,388`). `record` returns `Err(Replay)` for an equal count with a different
  digest, so the second user's correct credential is refused. Traced through the code rather than
  executed — no Rust changes were in this story's write set, so no `cargo` ran here.
- **The bump does not fix it.** `crates/sipx-ua/src/challenge.rs` is byte-identical between `v0.7.0`
  and `v0.10.0` (`git diff --stat v0.7.0 v0.10.0 --` on that path is empty), so `CX-4` moves the pin
  without moving this. The ledger row says so, because "the next release will have it" is exactly the
  belief [upstream.md](../upstream.md)'s own rules warn about.
- **Reachable today, and quietly.** Since `FC-3`, a document declaring `tenant[].auth` produces a
  challenging registrar with one `TenantAuth` — one authenticator, one window — per tenant per node
  (`sipx-clstr-node/src/driver.rs:166,729`). Two of that tenant's users challenged inside one second
  and both answering at the same edge is enough. Observable consequence: the second user gets §3 A6's
  `401` with **no** `stale` for a password that is right, so the client stops and asks a human. No
  attacker, no error logged, and the edge believes it refused a replay. Untouched: open tenants, and
  two answers that land on two edges (separate windows).
- **What landed here.** `registrar-auth` §6 is now §6.1 (`N1` — a nonce is unique per challenge,
  normative, with RFC 7616 §3.3 and §5.12 and RFC 3261 §22.4 item 4) and §6.2 (the old secret /
  multi-edge text, unchanged); §3 now says what "fresh" means in A2, A6 and A7; §7.3's rejected
  "bind the nonce to the request" row now says explicitly that it does not bear on `N1`; §8 carries
  `RA-R-8`.
- **`RA-R-8` is deferred, not passing** — [vector-scope.toml](../reference/vector-scope.toml), reason
  and story recorded, `docs/reference/conformance.md` regenerated. The row is written as the
  requirement, so `python3 scripts/check-vectors.py --check` demanded it the moment it appeared
  (`RA-R-8: in the spec, covered by no test, and not deferred`). That is this story's failing-first:
  covering it needs a kernel that mints entropy and a harness scenario in `sipx-clstr-sim`.
- **Considered for upstream: yes — filed as the `A nonce with per-challenge entropy` row in
  [upstream.md](../upstream.md)**, status `open` (not yet filed as a sipx story), carrying the mint's
  construction with `path:line`, the shared-counter mechanism, the RFC citations, a kernel-only
  reproduction (one `Authenticator`, one `challenge_at(false, t)`, two users' `Presented` each correct
  at `nc=1` → `Authenticated` then `Rejected(Replay)`), and what it blocks here. The row is the only
  thing this pass could file: writing a story into the sipx repository was outside this pass's write
  set. **Next agent: file it there and link both directions, per `CX-1`'s pattern.**
- **Not done, and why:** the sipx story itself; the `sipx-clstr-sim` scenario; the
  `assert_eq!(value, same_nonce, …)` fixture in
  `crates/sipx-clstr-registrar/tests/vectors_register_auth.rs:215`; and `auth.rs`'s "**fresh** nonce
  every time" comment (`crates/sipx-clstr-registrar/src/auth.rs:256`). The last three are `crates/**`
  and were fenced off this pass. The spec now records what that comment should say (§3, after A7), so
  correcting it is a comment change against a written rule rather than a judgement call.

## Notes

- **The mechanism.** The replay window is keyed on the nonce *string* and tracks one nonce-count per
  nonce. Alice authenticates at `nc=1` and proceeds; Bob — a different user, correct password,
  legitimately at `nc=1`, challenged in the same second and therefore holding the same nonce bytes —
  is refused as a replay. Per [registrar-auth](../specs/registrar-auth.md) §3 A6 that is a `401`
  *without* `stale=true`, which the spec designs so a client "will stop and ask a human". So a correct
  credential is refused and the user is sent to change a password that was fine — the failure mode
  §3 A7 exists to prevent, reached through a different door.
- **Latent today, immediate on `FC-3`.** No shipped node can enable authentication, so nothing is
  broken in the field. The moment `tenant[].auth` is wired, any node above roughly one REGISTER per
  second collides routinely, and Bob has to climb to Alice's nonce-count to get in. Read this story
  before believing a green auth test at scale.
- **Why the harness did not catch it.** All 24 `RA` vector rows pass, including RA-R-7. The rows are
  correct; the property is not among them. This is the shape `CF-8` is about — a spec's table is only
  as good as its coverage of the properties the spec means, and "unique per challenge" was never
  written down as one.
- The crypto around it is sound and should not be disturbed: constant-time comparison on both the
  response digest and the nonce MAC, keyed HMAC-SHA-256 rather than `H(secret‖msg)`, the realm bound
  into the MAC so one secret across two realms cannot cross-validate, and the digest verified before
  the clock so a wrong password on an expired nonce is `Mismatch` rather than `Stale`. The defect is
  entropy, not construction.
- Fixing it upstream will change what a nonce looks like, so check whether anything here parses a
  nonce's shape rather than treating it as opaque before the bump lands. `CX-4` is the pending kernel
  upgrade and is the natural carrier.

## Integration

**Integrated, and deliberately left open.** The spec rule and the upstream ledger row are in; the
coverage is not, and `RA-R-8`'s deferral names *this story*. Closing it would leave a deferral pointing
at work nobody will do — the precise failure `check-vectors.py`'s own header warns about — so the status
stays open until the row is covered.

Verified independently rather than taken from the report:

- **The defect is real.** `sipx-ua/src/challenge.rs`'s `mint` is `<issued-at hex>.<HMAC(issued-at)>` — a
  pure function of the clock, the realm and the secret. Two challenges in the same second at one edge
  produce the identical nonce.
- **`CX-4` will not fix it, and the story's own note saying it would was wrong.** `challenge.rs` is
  **byte-identical** across `v0.7.0` → `v0.10.0` → kernel `main`: `git diff` produces no output for that
  path in either range. The bump moves the pin, not the mint. `CX-4`'s note has been corrected.
- **The gate demands the new row.** Row count moved 492 → 493 and deferrals 358 → 359, so it is counted
  and passes only because it is deferred.

What remains: the sipx-side story (the implementor could not write into the kernel checkout), the
harness scenario, and the two code comments in `auth.rs` and `vectors_register_auth.rs` that claim a
"fresh nonce" the mechanism does not yet deliver. The spec now says what those comments should say, so
they become edits against a written rule rather than judgement calls.
