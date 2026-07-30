---
id: RG-17
title: Make authoritative location-store reads fallible instead of inventing absence
pillar: Registrar
status: ready
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

- [ ] `LocationStore::read` has a fallible result that distinguishes absence from database and decode
      failure; every backend, adapter, conformance fixture and caller handles the distinction.
- [ ] REGISTER read failure becomes `Rejection::Unavailable` and a wire `503`. Query, wildcard
      removal, absent-contact removal and mutation all take the same failure path; none relies on a
      later CAS to discover a read error.
- [ ] Location lookup preserves store failure separately from an empty target set. The proxy receives
      an explicit failure input/outcome and originates `503 Service Unavailable` rather than answering
      `480` as though the AoR were authoritatively empty. This locally originated 503 is not a branch
      response and is not rewritten by proxy-behavior R8.
- [ ] Commit failure and exhausted CAS retries retain their existing bounded `503` behavior.
- [ ] **Failing-first fault tests:** injected database-read and stored-JSON-decode failures make a
      REGISTER query, a no-op deregistration, and a call lookup fail explicitly. Each returns false
      absence on `86e6b10`.
- [ ] The identical location-store conformance suite runs against in-memory and PostgreSQL backends;
      live PostgreSQL fault coverage runs when `SIPX_CLSTR_TEST_DATABASE_URL` is set.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-08**](../reviews/00-validated-synthesis.md#v-08--postgresql-readdecode-errors-become-successful-absence).
- The current comment at `crates/sipx-clstr-node/src/postgres_store.rs:227-240` proves only the
  mutation case: a revision-zero commit can fence an existing row. `Outcome::Noop` returns before
  commit, so it has no such fence.
- **Upstream boundary:** no; fallible authoritative state and its REGISTER/lookup policy are platform
  location-service orchestration, not SIP kernel behavior.
