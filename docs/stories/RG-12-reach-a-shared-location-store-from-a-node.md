---
id: RG-12
title: Reach a shared location store from a running node
pillar: Signalling
status: backlog
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location, deploy]
note: blocked by DP-8 — RG-4 built the Postgres store and driver.rs still hardcodes InMemoryStore
---

# Reach a shared location store from a running node

## Goal

Let a node be pointed at the PostgreSQL location service that `RG-4` already built and tested, so
that two nodes can see the same registrations. Without this there is no multi-node registration:
every node is an island holding bindings that die with it.

## Acceptance

- [ ] The location store backend is selected by configuration, not by a cargo feature at the call
      site. `driver::run` no longer hardcodes `InMemoryStore::new()`.
- [ ] The DSN arrives as a reference (`dsnRef`) resolved from the environment, never as a literal
      in the document (cluster-config V9).
- [ ] A node configured for the in-memory store behaves exactly as it does today — this story adds
      a choice, it does not change the default's behaviour.
- [ ] **Failing-first**: a test registers an address-of-record against one node's store handle and
      resolves it through a second handle to the same database, proving the binding crossed the
      process boundary. It fails against `InMemoryStore`.
- [ ] The compare-and-swap contract still holds across two writers — a concurrent binding update
      from two handles cannot interleave into a set neither client asked for.
- [ ] Startup fails loudly and with exit `2` if the store is configured but unreachable. A
      registrar that silently falls back to memory is worse than one that refuses to boot.
- [ ] `cargo test -p sipx-clstr-node --all-features` green, and the Postgres tests run when
      `SIPX_CLSTR_TEST_DATABASE_URL` is set.

## Progress

- (running log)

## Notes

- **Blocked by `DP-8`**: the configuration surface has to exist before a store can be selected by
  it. Do not add a `--location-store` flag as a bridge — `main.rs` records that the schema replaces
  the flags rather than extending them.
- `RG-4` built and tested the backend; this story is purely about reachability from a running node.
- Authentication has the same shape of gap — digest is implemented and `AuthConfig` is never set by
  `main.rs`, so the shipped binary is an open registrar. Out of scope here, but the two are likely
  to be wired by the same mechanism, and the published docs currently promise neither.
