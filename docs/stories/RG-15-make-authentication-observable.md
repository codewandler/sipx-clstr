---
id: RG-15
title: Make authentication observable, and make its replay window O(1)
pillar: Signalling
status: in-progress
priority: 3
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, security, observability]
note: the reason for every 401 and 403 is computed and discarded; nothing logs an auth outcome at all
---

# Make authentication observable, and make its replay window O(1)

## Goal

Give the registrar an authentication audit trail, and stop the replay window scanning itself from the
wrong end. The decision already computes *why* it refused; the driver drops it, and there is no log
line on a `401`, a `403` or a success anywhere in the register path.

## Acceptance

- [x] Every authentication outcome is logged with its reason. `ChallengeResponse.because` is documented
      "Why, for logs and tests" and `driver.rs` discards it; the register path emits no `tracing` event
      at all. [registrar-auth](../specs/registrar-auth.md) §3 A1 justifies the open-tenant principal by
      "`RG-3`'s audit trail must be able to say *unauthenticated*", and nothing writes that anywhere.
      → `driver::record_authentication`, one record per §3 outcome; the rules are `registrar-auth` §9.
- [x] Credentials, nonce secrets and password material never reach a log line. This crate already has
      the pattern — `StoreChoice::describe()` returns a backend name and never its DSN, precisely so a
      log line cannot leak a resolved secret "into the one artefact most likely to be copied into an
      issue". Apply the same discipline here rather than inventing a second one.
      → `ChallengeResponse::describe() -> &'static str`; the return type is the guarantee. `RA-L-1`
      asserts the presented password, nonce, `cnonce`, digest and username reach no line.
- [ ] The replay window is O(1) on the hot path. `seen` is a `VecDeque` scanned front-to-back while
      the nonce being checked is always the newest entry at the back, so every authenticated REGISTER
      walks all 4096 entries — measured at roughly 11 µs empty against 23 µs full, under the node-wide
      authenticator lock. A map keyed by nonce makes it a lookup.
      → **Not done here, and it may not be**: `seen` is a private field of `sipx-ua`'s `Authenticator`,
      the window is an auth primitive (AGENTS.md #6), and `registrar-auth` §2 already names "the replay
      window's data structure" as explicitly not this platform's. Filed in
      [upstream.md](../upstream.md) with the mechanism, the `path:line`s, a reproduction and a fresh
      measurement (7.5 µs at one entry against 19.2 µs at 4096, timing `verify_at` alone) — which
      corroborates the story's numbers. The `O(n)` under the same lock that **is** ours is done:
      `InMemoryCredentials` is keyed rather than scanned.
- [ ] **Failing-first**: a test asserts an authentication refusal is observable — a `401` with a
      recorded reason — and a benchmark or timed assertion pins the window's cost as independent of
      how full it is. Both fail today.
      → First clause done: `crates/sipx-clstr-node/tests/auth_observable.rs`, red at the merge base
      with three REGISTERs producing zero records. Second clause **not** done as a committed
      assertion: an assertion that the *kernel's* window is `O(1)` cannot pass against the pinned tag
      and this repository may not change it, so the measurement is evidence in the ledger row instead.
      Its local analogue is committed —
      `auth::tests::the_credential_lookup_does_not_scan_the_tenant`, red at 1 against 4096.
- [x] The poison bypass in the auth path is reconsidered. `driver.rs` takes the authenticator lock
      with `unwrap_or_else(PoisonError::into_inner)`, which deliberately keeps deciding authentication
      on state a panic left mid-update. That is the right default for most locks in this crate and the
      wrong one for a replay window; decide and record which.
      → **Kept, and now recorded and observed.** See the Progress note; `driver.rs:765`.
- [x] `cargo test -p sipx-clstr-registrar -p sipx-clstr-node` green. → and the whole gate with it.

## Progress

**Pass 1 — everything except the kernel's replay window, which is filed rather than fixed.**

- **The audit trail is `registrar-auth` §9**, new: L1 (one record per §3 outcome, with its reason),
  L2 (no credential material in a record, enforced by a `&'static str` return type rather than by
  discipline), L3 (a success names §5's principal; an A1 proceed says *unauthenticated* out loud).
  Vectors `RA-L-1`, `RA-L-2` and `RA-L-3` in §8, all three proved by
  `crates/sipx-clstr-node/tests/auth_observable.rs`. `("RA", "L")` joins `check-vectors.py`'s
  families and `docs/reference/conformance.md` is regenerated.
- **The record is emitted from the driver, not from the decision.** The registrar is sans-IO
  (AGENTS.md #2) and a decision function that logs performs an effect the harness cannot replay from
  a seed, so the registrar produces the fact and `driver::record_authentication` emits it. What the
  registrar gained is `ChallengeResponse::describe()`, which is the whole of L2's mechanism: the
  reason is a `&'static str`, so a nonce, a `cnonce`, a response digest, a presented username or a
  password cannot ride into a line however carelessly a driver writes it.
- **A2 is `info` and says "challenged"; A6, A7 and A3 are `warn` and say "refused".** Every phone's
  ordinary first REGISTER takes A2, and recording it as trouble would bury the real thing. A
  `Rejection` that is not `Forbidden` is *not* recorded as an authentication outcome at all — `admit`
  produces `Forbidden` from A3 alone, and everything else it can reject with is a message that
  authenticated fine and then failed to become a command.
- **The poison bypass stays, and the argument is now written down rather than assumed.** Propagating
  the poison would make one panic stop every REGISTER for the tenant for the life of the process, and
  a registrar that stops answering refreshes turns a transient fault into every phone on the tenant
  going unreachable. What the bypass can cost is bounded on the safe side: the realm, the secret and
  the algorithm are immutable for the authenticator's life, so the only state a panic can tear is the
  replay window, and a torn entry is one whose count advanced without its digest — which **refuses a
  correct credential**, never accepts a wrong one. Fail-closed. What was genuinely wrong was that it
  was silent, so a poisoned lock is now an `error!` before it is stepped over.
- **`InMemoryCredentials` is keyed, not scanned.** It was a `Vec` walked with `find` — `O(users)` per
  REGISTER under the node-wide authenticator lock, which is a ceiling on the node rather than on one
  request. Two nested maps, because a `HashMap<(String, String), _>` cannot be probed with
  `(&str, &str)` without allocating a key per lookup on the hot path. First-wins on a duplicated
  `(tenant, username)` is preserved deliberately and pinned by a test. So this closes the story note's
  "fix it here or name it in `FC-3` — not neither" by fixing it here.
- **`auth.rs`'s A4 comment is trimmed.** It claimed a decision "identical in content *and* in the work
  done" for a present versus an absent user, which the early-returning scan never made good on. It now
  claims what is true — the digest runs either way, which is the part that dominates — and says
  explicitly that it is not a constant-time claim.
- **Considered for upstream: split.** The replay window's data structure is the kernel's and is filed
  in [upstream.md](../upstream.md) as a new `open` row; the audit trail is deployment policy —
  *which* refusals a platform records and what it will not print — and stays here, alongside the rest
  of `registrar-auth`'s policy half.
- **Left for elsewhere, deliberately.** `RA-L` covers the outcomes this test can reach without a
  correct digest; the "authentication succeeded" record with §5's principal is emitted and reviewed
  but not vector-pinned, because asserting it needs a scenario that authenticates, which is
  `sipx-clstr-sim`'s shape rather than this test's.

## Notes

- **Why this is worth doing before `FC-3` rather than after.** The code all exists and is reachable
  from tests, so none of it waits on authentication being wireable. And the combination it guards
  against is live the moment `FC-3` lands: no rate limiting, a 300-second nonce lifetime, and no log
  line on a refusal makes brute force against a tenant both undetectable and unbounded.
- **The lock itself is fine and should stay.** It is a correctness requirement — `record` mutates the
  shared replay window, which is what §6 makes the replay defence — and it spans synchronous work only,
  never an `await`. Measured ceiling is roughly 42 k REGISTER/s node-wide, which is not a registrar's
  binding constraint. The defect is the data structure under it, not the lock.
- One more O(n) under that lock: `InMemoryCredentials::password` is a linear scan with `String`
  compares, so it is O(users) per REGISTER. Harmless while credentials are always empty; a throughput
  ceiling the moment `FC-3` populates them. Fix it here or name it in `FC-3` — not neither.
- `auth.rs`'s comment claiming a decision is "identical in content *and* in the work done" for a
  present versus absent user is not literal, because that scan returns early on a hit. Negligible
  beside the SHA-256 work, but the comment overstates and should be trimmed rather than left as a
  claim the code does not make good on.
- Considered for upstream: the replay window lives in `sipx-ua`'s challenge machinery, so the data
  structure change is likely kernel surface — check before implementing, and see `CX-5`, which is
  already filing a defect against the same code.
