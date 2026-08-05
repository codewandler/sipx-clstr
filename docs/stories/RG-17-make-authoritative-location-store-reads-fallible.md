---
id: RG-17
title: Make authoritative location-store reads fallible instead of inventing absence
pillar: Registrar
status: in-progress
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location, postgres, node]
note: V-08 · a failed or undecodable PostgreSQL read becomes empty revision zero, so a query or no-op removal can return a false 200
---

# Make authoritative location-store reads fallible instead of inventing absence

## Goal

Represent an unavailable or undecodable authoritative location-store read as failure, so REGISTER
and proxy lookup never turn unknown durable state into a successful empty answer.

## Acceptance

- [x] `LocationStore::read` has a fallible result that distinguishes absence from database and decode
      failure; every backend, adapter, conformance fixture and caller handles the distinction.
- [x] REGISTER read failure becomes `Rejection::Unavailable` and a wire `503`. Query, wildcard
      removal, absent-contact removal and mutation all take the same failure path; none relies on a
      later CAS to discover a read error.
- [x] Location lookup preserves store failure separately from an empty target set. The proxy receives
      an explicit failure input/outcome and originates `503 Service Unavailable` rather than answering
      `480` as though the AoR were authoritatively empty. This locally originated 503 is not a branch
      response and is not rewritten by proxy-behavior R8.
- [x] Commit failure and exhausted CAS retries retain their existing bounded `503` behavior.
- [x] **Failing-first fault tests:** injected database-read and stored-JSON-decode failures make a
      REGISTER query, a no-op deregistration, and a call lookup fail explicitly. Each returns false
      absence on `86e6b10`.
- [x] The identical location-store conformance suite runs against in-memory and PostgreSQL backends;
      live PostgreSQL fault coverage runs when `SIPX_CLSTR_TEST_DATABASE_URL` is set.
- [x] `scripts/gate.sh` is green.

## Progress

Landed, spec first: location-service §6.1 **K7** (a store that cannot be read is not an empty store)
and §7 **L8** (a lookup failure is not L5's empty set), with §5.1 S10 amended to name the read and §2's
trait sketch made fallible; proxy-behavior §2 gains `TargetsUnavailable` and §7 the locally originated
`503` that R8 does not rewrite. Four vector rows registered and **proved**: `LS-K-7`, `LS-K-8`,
`LS-L-9`, `PB-F-11` (`docs/reference/conformance.md` regenerated; 173/602 proved, no new deferrals).

The code: `LocationStore::read` and `::lookup` return `Result<_, ReadFailure>`, whose two variants —
`Unavailable` and `Undecodable` — are `PostgresStore`'s `StoreError::{Database, Decode}` kept apart all
the way out. `apply` refuses at the read with `Rejection::Unavailable`, **before** `process` runs, so a
query, an absent-contact removal, a wildcard removal and a mutation take one path. `Applied.revision`
became `Option<Revision>`: on a failed read nothing was learned, and reporting `Revision::INITIAL`
there would be the same invention the story closes.

Deliberately *not* changed: `commit`. Its `CasConflict`-on-error behaviour and S10's exhausted-retry
`503` are untouched, and `vectors_register.rs::s10_exhausted_cas_retries_answer_503_rather_than_looping`
still proves them.

`driver.rs` is one hunk — the `ProxyEffect::ResolveTargets` arm, which now feeds
`ProxyInput::TargetsUnavailable` on `Err`. A Request-URI that will not canonicalize still resolves to
the empty set and `480`: that is an answer, not a failure.

`scripts/gate.sh` is green with `SIPX_CLSTR_TEST_DATABASE_URL` set, so its opt-in PostgreSQL step
ran too — `postgres_read_faults` 3/3 and `postgres_store` 9/9, the latter including
`run_read_failure_suite` against a live corrupted row and the `LS-L-9` lookup fault.

The merge base carried a gate failure of its own —
`crates/sipx-clstr-node/tests/devspace_dialable.rs` was unformatted and tripped three clippy lints,
from the `8c61cf4` rescue commit. It was left untouched here (out of fence) and fixed on `main` in
`151f2e2`, which this branch merged before its final gate run.

### For the integrator

- Ledgers are untouched by design (`CHANGELOG.md`, the board, `docs/roadmap.md`): this story is
  `in-progress` and needs `/track:done RG-17` on merge.

## Notes

- Validated synthesis finding [**V-08**](../reviews/00-validated-synthesis.md#v-08--postgresql-readdecode-errors-become-successful-absence).
- The current comment at `crates/sipx-clstr-node/src/postgres_store.rs:227-240` proves only the
  mutation case: a revision-zero commit can fence an existing row. `Outcome::Noop` returns before
  commit, so it has no such fence.
- **Upstream boundary:** no; fallible authoritative state and its REGISTER/lookup policy are platform
  location-service orchestration, not SIP kernel behavior.
