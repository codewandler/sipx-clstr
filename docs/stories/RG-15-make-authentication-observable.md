---
id: RG-15
title: Make authentication observable, and make its replay window O(1)
pillar: Signalling
status: ready
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

- [ ] Every authentication outcome is logged with its reason. `ChallengeResponse.because` is documented
      "Why, for logs and tests" and `driver.rs` discards it; the register path emits no `tracing` event
      at all. [registrar-auth](../specs/registrar-auth.md) §3 A1 justifies the open-tenant principal by
      "`RG-3`'s audit trail must be able to say *unauthenticated*", and nothing writes that anywhere.
- [ ] Credentials, nonce secrets and password material never reach a log line. This crate already has
      the pattern — `StoreChoice::describe()` returns a backend name and never its DSN, precisely so a
      log line cannot leak a resolved secret "into the one artefact most likely to be copied into an
      issue". Apply the same discipline here rather than inventing a second one.
- [ ] The replay window is O(1) on the hot path. `seen` is a `VecDeque` scanned front-to-back while
      the nonce being checked is always the newest entry at the back, so every authenticated REGISTER
      walks all 4096 entries — measured at roughly 11 µs empty against 23 µs full, under the node-wide
      authenticator lock. A map keyed by nonce makes it a lookup.
- [ ] **Failing-first**: a test asserts an authentication refusal is observable — a `401` with a
      recorded reason — and a benchmark or timed assertion pins the window's cost as independent of
      how full it is. Both fail today.
- [ ] The poison bypass in the auth path is reconsidered. `driver.rs` takes the authenticator lock
      with `unwrap_or_else(PoisonError::into_inner)`, which deliberately keeps deciding authentication
      on state a panic left mid-update. That is the right default for most locks in this crate and the
      wrong one for a replay window; decide and record which.
- [ ] `cargo test -p sipx-clstr-registrar -p sipx-clstr-node` green.

## Progress

- (running log)

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
