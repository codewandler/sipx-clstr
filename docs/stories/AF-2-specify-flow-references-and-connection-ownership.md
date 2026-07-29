---
id: AF-2
title: Specify flow references and connection ownership
pillar: Cluster
status: in-progress
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, transport]
note: 
---

# Specify flow references and connection ownership

## Goal
Specify the connection table and the `flow_ref = signed(node_id, connection_id, generation)` reference stored in location bindings — a client's connection has exactly one owner.

## Acceptance
- [x] The connection-table schema is specified: transport, remote address, authenticated identity, TLS info, flow generation, last activity.
- [x] flow_ref format and generation-bump rules (reconnect invalidates old references) are specified with vectors.
- [x] Binding integration is defined with RG-1 so a lookup yields the owner to RPC.

## Progress
- 2026-07-29: Written as **§11–§14 of [affinity-token](../specs/affinity-token.md)** rather than a
new file: the flow reference shares the token's key set, algorithms, 14-byte header and AEAD call
site, and splitting them would have duplicated §4 and §6 into a second document that drifts. The
spec's title, story list, scope note and upstream section were updated to cover both families.
- 2026-07-29: **Layout (§11.2).** `version 0x81 | key id | nonce[12] | tenant u32 | node u16 |
incarnation u32 | connection u32 | generation u32 | transport u8 | tag`. Fixed length: **49 B**
encrypted (`chacha20-poly1305`, the default) or **45 B** authenticated-only (`hmac-sha256-96`);
canonical text form base64url-unpadded, 66/60 chars. Two additions to the design's
`signed(node_id, connection_id, generation)` sketch, both load-bearing: **`incarnation`** (the
owner's boot second) stops a restarted node from re-issuing an identity a live reference already
names — without it the generation counter only protects within one run of one process (vector
FR-9) — and **`transport`** lets a caller refuse a `sips` target on a plaintext flow
(RFC 3261 §26.2.2) without asking the owner.
- 2026-07-29: **No expiry field, deliberately (FM7).** A token needs one because a dialog exists
nowhere in the cluster; a reference names a row that either exists or does not, so the object's
death is a tighter bound than any clock and needs no clock. Consequence: `verify_flow` takes **no
`now`** — the only verifier in this epic that does not — and time enters only as the key set's
validity windows (narrowed on reload/timer) and the connection table's idle timer. Second
consequence, recorded as an amendment to §6 K4: a retiring key must stay verify-valid until
`t_switch + max(L, E_max) + S`, because a reference leaves circulation when its binding refreshes,
not when a clock says so.
- 2026-07-29: **Domain separation (§11.3) is cryptographic, not conventional.** Both families are
minted under one key set, so byte 0 (`0x01` token / `0x81` reference) sits inside the AEAD's AAD
and the HMAC input: rewriting it to change a record's family invalidates the tag (FR-20), and the
reverse rejection was already normative in §8 S1 (FR-18/FR-19). One shared header shape, one
prologue, one key lookup.
- 2026-07-29: **Generation-bump rules (§12.2).** Slot + generation + incarnation, with four MUSTs
that make the invariant mechanical: node ids unique across the node set (CT1 — the only way to
break the invariant from outside this spec, so AF-6/DP-1 must reject duplicates at validation
time); incarnation strictly increasing across restarts, taken as an input, not read from a clock
(CT2); generation bumped on every slot allocation (CT3) and **never wrapped** — a slot at
`0xFFFFFFFF` is retired instead (CT4). Freed slots return to the free list immediately: safety
comes from the bump, not from a quarantine.
- 2026-07-29: **Binding integration (§13.3) and the outcome taxonomy (§13.2).** `lookup` →
`verify_flow` → owner is one store read and two pure functions, with the owner falling out of the
reference itself — no directory, no membership query (AGENTS.md rule 5 holds by construction).
Resolution `RS1…RS5` is a pure function of the owner's table. The taxonomy AF-3 will carry is
fixed here with its cost to the request: `FlowDead` and `OwnerUnreachable` remove the target
before a branch exists (→ `480` when the set empties, `430 Flow Failed` in M3), `FlowRejected`
is a branch failure with `503` semantics — an owner that is up and saying "not now" is a server
condition, not an unavailable user.
- 2026-07-29: **Vectors FR-1 … FR-21** (§14), byte-exact under §10's test key set — deliberately
the same keys, since one key set mints both families and a second one would not exercise that.
Five round-trip, seven resolution (FR-7 is the reconnect/generation-bump row; FR-9 the restart
row), eight negative, one integration. Derived with an independent script that first reproduces
AF-1's AT-1/AT-2/AT-3/AT-5/AT-6 byte-for-byte and RFC 8439 §2.8.2, then every encrypted vector is
re-opened to its plaintext body; every hex string and text form in the spec was re-checked against
that derivation after writing.
- 2026-07-29: **Flagged, not decided — the reconnect/idempotency gap (BI5).** A UA that reconnects
and *retransmits* (same Call-ID, same CSeq, new reference) is a Noop under location-service §5.3
B4 as written, leaving the binding pointing at a dead connection until the next refresh. Safety is
untouched — the stale reference resolves to nothing (RS3), never to the new connection — so this
is reachability latency, not mis-delivery. It changes §5.3's comparison and is therefore RG-8's
call; the spec records the recommendation (compare **flow identity**, not bytes).
- 2026-07-29: Zero Rust changed. Doc gate green: `cargo fmt --all --check`,
`scripts/check-provenance.sh`, `scripts/check-vectors.py --check`, `scripts/check-docs.sh`. Board,
CHANGELOG and roadmap untouched per task scope.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
