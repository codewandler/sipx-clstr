---
id: AF-1
title: Specify the affinity token
pillar: Cluster
status: done
priority: 3
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: 
---

# Specify the affinity token

## Goal
Write `docs/specs/affinity-token.md`: the byte-level format and security rules of the signed token that lets any edge route a mid-dialog request with zero global lookups.

## Acceptance
- [x] Fields specified with byte layout: format version, key id, tenant, home shard, edge affinity, direction (which side of the dialog the Route position faces), media node, policy version, expiry, nonce, authentication tag — with mint → encode → parse → verify round-trip vectors.
- [x] Encoding into Record-Route/Route and Path URIs is specified with a size budget (target: well under 200 bytes as a URI parameter), justified against UDP MTU limits.
- [x] Key rotation via key id and overlapping validity; distribution is config-first.
- [x] Verification-failure behavior is normative: a mid-dialog request with an unverifiable token is hard-rejected; there is no fallback routing.
- [x] The encryption policy names which fields are confidential; every token is authenticated.
- [x] Replay semantics are normative: re-presenting the same token on every mid-dialog request 
is the mechanism, not an attack; verification is stateless, and cross-context abuse is bounded 
by expiry and the scope fields.

## Progress
- 2026-07-28: Wrote `docs/specs/affinity-token.md` (normative). Layout v1: header 14 B (version 1 + key id 1 + nonce 12), body 20 + F B (tenant u32, home shard u16, edge affinity u16, direction u8, media node u16, policy version u32, expiry u32, facts-len u8, module-facts region F ≤ 64 B), tag 16 B (ChaCha20-Poly1305 AEAD, default/encrypted) or 12 B (HMAC-SHA-256/96, cleartext opt-out). Verification is a 9-step stateless algorithm; every failure is a `403` hard reject per proxy-behavior §5 P3 / PB-P-4/5. Replay semantics normative (§9): no nonce store, re-presentation is the mechanism, cross-context abuse bounded by expiry + tenant/shard/direction scope. Key model: config-first, distribute-then-activate rotation, retired key = hard reject. Expiry decision: no mid-dialog refresh exists in the protocol (RFC 3261 §12.2 fixes the route set at dialog establishment, so re-minting into target-refresh Record-Route cannot reach the peer); therefore expiry outlives the dialog — default L = 24 h, configurable, explicit 403 failure mode beyond it.
- 2026-07-28: **FINAL BYTE BUDGET, for the proxy-behavior F4 / PX-1 re-review:** worst-case token URI parameter = **157 bytes** (raw 114 B at the 64-byte module-facts ceiling → 152 base64url chars + 5 for `;aft=`); empty-facts default = 72 B; authenticated-only mode = 67–152 B. The provisional ≤ 200 B budget in proxy-behavior §7 F4 and PX-1's open checkbox are **verified with 43 B worst-case headroom** — recommend keeping the 200 B ceiling as v2 headroom. One wording flag (not edited here): vector PB-F-1 says "Record-Route + token ≤ 200 B"; if read as the whole header line rather than the parameter, a full-facts token exceeds it (202 B line with the example host). The normative F4 text ("the token parameter") is what the spec verifies.
- 2026-07-28: **EX-1 coordination (hook-framework §5 class b / G5):** the requested module-facts region is reserved in the layout — variable-length, own length framing (facts-len byte, verified against body length at step S5), byte-identical across the Record-Route pair, opaque to mint/verify, confidential (inside the AEAD ciphertext). **The 64-byte sub-budget FITS and is adopted as the normative ceiling** (`F ≤ 64`); arithmetic shown at F = 0 (param 72 B) and F = 64 (param 157 B) in spec §5; ceiling vector AT-6 pins the max case with real bytes. EX-1's 64 B placeholder needs no change; budget authority is now affinity-token.md §3.
- 2026-07-28: Vectors AT-1 … AT-18 are deterministic and byte-exact under two documented test keys (AEAD id 0x01, MAC id 0x02), fixed clock T0 = 1785240000, computed with an RFC 8439 §2.8.2 self-check and decrypt round-trip verification. Negatives cover tampered tag/ciphertext, expired (with boundary), unknown/retired key id, wrong tenant scope, pair direction mismatch, invalid direction value, unknown version, truncation, padded encoding, and facts-framing mismatch/overflow. Path carriage uses the same parameter and budget; the Path direction value is deliberately deferred to M3 (upstream typed Path header, see docs/upstream.md).
- 2026-07-28: Board not regenerated and CHANGELOG untouched per task scope; story left in-progress pending integrator review of the F4 number.

## Notes
- 2026-07-28 — integrator review passed; cross-references reconciled (see CHANGELOG).
- Design: [cluster-affinity](../designs/cluster-affinity.md).
