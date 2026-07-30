---
id: AF-4
title: Implement the token mint/verify library
pillar: Cluster
status: in-progress
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: blocked by AF-1
---

# Implement the token mint/verify library

## Goal
Implement AF-1 as a pure library: mint, encode, parse, verify — with key rotation.

## Acceptance
- [x] AF-1's byte-level vectors pass exactly.
- [x] Tampered tags, expired tokens and unknown key ids are rejected, each with a test; verification is stateless — legitimate re-presentation of the same token on every mid-dialog request verifies every time, and no replay store exists (per AF-1's replay semantics).
- [x] Rotation works with overlapping key validity; old-key verification ends at the specified boundary.

## Progress

- **`crates/sipx-clstr-affinity`** is the library: `mint`/`mint_with`/`verify`, the `KeySet` with
  its verify windows, and the strict unpadded-base64url codec for the `aft` parameter value. Sans-IO
  throughout — `now` and the nonce are arguments, and the `chacha20poly1305` dependency is built
  `default-features = false` so its `getrandom` feature cannot reach an OS entropy source from a
  crate that must be replayable. No allocation on the mint path; the token is a fixed 114-byte
  inline buffer.
- **All eighteen §10 vectors pass byte-exactly**, `AT-1` … `AT-18`, in
  `tests/vectors_affinity_token.rs`. `AT-17` and `AT-18` are records `mint` refuses to produce, so
  the suite seals them itself from the plaintexts §10 prints — an independent construction that has
  to agree with the fixture bytes.
- **§6 rotation and §9 replay** are `tests/rotation_and_replay.rs`: K1/K3/K4 walked in order,
  `retirement_deadline` implementing `t_switch + max(L, E_max) + S`, the old key verifying at the
  boundary second and rejecting one second later, and eight threads verifying one token
  concurrently through a shared `&KeySet` — which is what "no replay store" looks like when it is
  checked rather than asserted.
- **`affinity-token` §10 gained a round-trip vector table.** `AT-1` … `AT-6` were prose with byte
  blocks while `AT-7` … `AT-18` were rows, so `check-vectors.py` could read half of §10 and had no
  claim to hold the other half to. The table restates no byte.
- **Blocked on one fenced file.** The eighteen `AT-*` rows are still marked deferred in
  `docs/reference/vector-scope.toml`, so `scripts/check-vectors.py --check` fails with eighteen
  *stale deferral* errors plus an out-of-date `docs/reference/conformance.md`. Deleting the
  `[[deferred]]` blocks for `AT-1` … `AT-18` and re-running `scripts/check-vectors.py` (no
  `--check`) to regenerate the report is the whole remaining change; every other gate step is
  green. The `FR-*` rows stay deferred to `AF-7`, correctly — this story implements §2–§10 only.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
- Considered for upstream: **no** — mint/verify over this cluster's routing state is orchestration.
  The fields name platform concepts (tenants, shards, edges, media nodes, module facts) and
  carriage reuses the kernel's URI-parameter surface unchanged, so there is nothing protocol-generic
  to lift. Same answer and same reasoning as the spec's own §1 note; nothing joins
  [upstream.md](../upstream.md).
- One new third-party dependency: `chacha20poly1305` (RustCrypto, the family `hmac` and `sha2`
  already come from). RFC 8439 is not optional here — it is the default algorithm and six §10
  vectors are AEAD — and hand-rolling it would contradict the workspace's own stated position on
  bespoke constructions in security positions.
