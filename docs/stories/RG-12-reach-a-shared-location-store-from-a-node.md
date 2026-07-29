---
id: RG-12
title: Reach a shared location store from a running node
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location, deploy]
note: the store is selectable and refuses to fall back; reading a document at startup is DP-10
---

# Reach a shared location store from a running node

## Goal

Let a node be pointed at the PostgreSQL location service that `RG-4` already built and tested, so
that two nodes can see the same registrations. Without this there is no multi-node registration:
every node is an island holding bindings that die with it.

## Acceptance

- [x] The location store backend is selected by configuration, not by a cargo feature at the call
      site. `driver::run` no longer hardcodes `InMemoryStore::new()`.
- [ ] The DSN arrives as a reference (`dsnRef`) resolved from the environment, never as a literal
      in the document (cluster-config V9). **Not done here** — `StoreChoice::Postgres` takes the
      *resolved* DSN and its doc comment says why (resolution is IO, so V9 puts it in the driver),
      but nothing resolves a `dsnRef` yet because nothing reads a document yet. `DP-10`.
- [x] A node configured for the in-memory store behaves exactly as it does today — this story adds
      a choice, it does not change the default's behaviour.
- [x] **Failing-first**: a test registers an address-of-record against one node's store handle and
      resolves it through a second handle to the same database, proving the binding crossed the
      process boundary. It fails against `InMemoryStore`.
- [x] The compare-and-swap contract still holds across two writers — a concurrent binding update
      from two handles cannot interleave into a set neither client asked for.
- [x] Startup fails loudly if the store is configured but unreachable. A
      registrar that silently falls back to memory is worse than one that refuses to boot.
- [x] `cargo test -p sipx-clstr-node --all-features` green, and the Postgres tests run when
      `SIPX_CLSTR_TEST_DATABASE_URL` is set.

## Progress

- **Done for the mechanism; the startup wiring is `DP-10`.** `NodeConfig` now carries a
  `StoreChoice`, and `driver::run` opens what it names instead of `InMemoryStore::new()`.
- **The refactor this needed.** `driver.rs` threaded `&InMemoryStore` through ten signatures, so the
  store could not be chosen at all. Those are now `&(dyn LocationStore + Send + Sync)` — the trait
  was already dyn-compatible, and the `Send + Sync` bounds are what keep the per-request
  `tokio::spawn` future `Send`.
- **A configured store that cannot be reached stops the node.** `NodeError::LocationStoreUnreachable`,
  connected eagerly at startup rather than lazily on the first REGISTER. A registrar that fell back
  to memory would come up healthy, answer `200` to everything, and serve bindings no peer can see —
  and nothing would say so. Also refused: asking for `postgres` in a binary built without the feature.
- **Failing-first, both halves demonstrated in one test.** The red half is executed, not asserted in
  prose: two in-process stores are opened, one is written, the other is read, and the test requires
  the second to be *empty* — if that ever passes, the green half has stopped discriminating. The
  green half then writes through one `PostgreSQL` handle and reads it back through a second,
  independently opened one, and checks that a stale-revision write from the second is refused rather
  than merged.
- Run against a real database, not skipped: a `postgres:16-alpine` container on `127.0.0.1:55432`.
  The pre-existing `RG-4` suite (7 tests) passes against the same instance, so the harness itself is
  known good.
- Gate green with the Postgres job enabled: 37 lib tests, the `RG-4` suite, these 3, MSRV 1.94,
  provenance, vectors, docs.
- Considered for upstream: no. Which location service a node uses is this platform's concern; the
  kernel has no location service.
- **What a reader should not conclude.** This does not make a cluster. It makes the *store* shared.
  Two nodes still need to be told to use it, which needs a document at startup (`DP-10`), and the
  cross-node call proof is `DP-9`.

## Notes

- **Blocked by `DP-8`**: the configuration surface has to exist before a store can be selected by
  it. Do not add a `--location-store` flag as a bridge — `main.rs` records that the schema replaces
  the flags rather than extending them.
- `RG-4` built and tested the backend; this story is purely about reachability from a running node.
- Authentication has the same shape of gap — digest is implemented and `AuthConfig` is never set by
  `main.rs`, so the shipped binary is an open registrar. Out of scope here, but the two are likely
  to be wired by the same mechanism, and the published docs currently promise neither.
