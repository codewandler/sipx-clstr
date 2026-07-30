---
id: AF-4
title: Implement the token mint/verify library
pillar: Cluster
status: done
priority: 1
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: all 18 AT vectors proved; sans-IO verified by dependency graph, and CX-5's nonce class closed
---

# Implement the token mint/verify library

## Goal
Implement AF-1 as a pure library: mint, encode, parse, verify — with key rotation.

## Acceptance
- [x] AF-1's byte-level vectors pass exactly.
- [x] Tampered tags, expired tokens and unknown key ids are rejected, each with a test; verification is stateless — legitimate re-presentation of the same token on every mid-dialog request verifies every time, and no replay store exists (per AF-1's replay semantics).
- [x] Rotation works with overlapping key validity; old-key verification ends at the specified boundary.

## Progress
- **Done.** `crates/sipx-clstr-affinity` mints and verifies; all eighteen `affinity-token` §10 vectors
  pass, and they land as **proved**, not shape-only.
- **The vectors were recomputed independently, twice, by two parties who did not meet.** The
  implementor recomputed `AT-1`…`AT-18` in Python against `cryptography`'s ChaCha20-Poly1305 *before
  writing any Rust*; the reviewer then recomputed nine of them again from **§3's field table** rather
  than from §10's printed hex, using OpenSSL rather than RustCrypto, and reproduced the plaintexts,
  the tokens and the base64url parameters byte-identically. That only happens if the header
  composition, field order, endianness and AAD all agree. Spec and implementation were not bent
  toward each other.
- **Sans-IO holds by dependency graph rather than by assertion.** `getrandom` appears under this crate
  for none of its three versions; the tree is `aead`, `chacha20`, `cipher`, `poly1305`,
  `universal-hash`, `zeroize` and nothing entropy-shaped. `default-features = false` is load-bearing:
  the default set pulls `getrandom`, which would put an OS entropy source inside a crate the
  deterministic harness must be able to replay.
- **`CX-5`'s defect class was checked, not merely avoided.** The nonce is 96 *injected* bits, never
  derived from a clock or a realm; `AT-1` and `AT-2` share claims and differ in every byte after
  offset 1; §8 `S9` refuses a pair whose two entries share a nonce. No analogous weakness was found in
  the spec to report.
- **The AAD is the whole 14-byte header, including version and key id**, so a token cannot be
  relabelled to another key or replayed as a flow reference. Both AEAD failure modes collapse to one
  `Reason::Tag` — no padding-oracle shape, because there is no padding and no distinguishable branch.
- **`Reason` cannot leak by accident:** it has no `Display` and no `Error` impl, so it is
  unformattable into a response with `{}`, and the telemetry-only contract is stated on the enum and
  on `Verdict::Invalid` where a consumer meets it.
- **Statelessness is structural.** No `Cell`/`RefCell`/`Mutex`/`OnceLock`/`static mut`/`unsafe`
  anywhere in the crate; `verify` takes `&KeySet`, so a replay ledger would need `&mut` or interior
  mutability and would not compile. Proved by 2048 re-presentations and 8 threads through one key set.
- **Robustness beyond the diff's own tests**, run at review: 200 000 random buffers, 260 000 structured
  mutants at every facts length 0–64, 200 000 random decoder strings and the `u32` corners — in
  **debug**, with overflow checks on. No panic, and no random input ever returned `Valid`.
- **Integrator work, done at merge:** the eighteen `AT-*` deferrals removed from the fenced
  `vector-scope.toml`, `sipx-clstr-affinity` added to `[workspace.dependencies]` so `AF-5` can depend
  on it by convention, the crate named in `AGENTS.md`'s layout table, and §2's signature sketch
  brought into line with what shipped.
- **Known and deliberate, for `AF-5`:** non-canonical base64url is *accepted* (§5 names exactly two
  rejections and both are implemented), so two distinct `aft` parameter strings can carry one token.
  Harmless here because every decision runs on authenticated token bytes — a hazard only if `AF-5`
  ever compares parameter strings for equality. And the 2³² mint ceiling is procedure, not a runtime
  refusal: a stateless `mint` cannot count.

- (not started)

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
