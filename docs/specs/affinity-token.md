# Spec: Affinity token

**Status:** normative · **Crate:** _future — created by CX-2_ · **Stories:** AF-1, AF-4, AF-5 ·
**Design:** [cluster-affinity](../designs/cluster-affinity.md)

## 1. Normative references

- RFC 3261 §12.1.1 / §12.1.2 — route-set construction from Record-Route (UAS: in order; UAC: in
  reverse order). Fixes which side presents which entry of the pair (§7).
- RFC 3261 §12.2.1.2 / §12.2.2 — the route set is **not recomputed** by target refresh requests.
  Load-bearing for the expiry decision (§7, M6).
- RFC 3261 §16.6 step 4 — Record-Route insertion; §18.1.1 — message size vs path MTU (the
  1300-byte guidance behind the size budget, §5); §19.1.4 — URI parameter comparison; §25.1 —
  `pname`/`pvalue` ABNF the parameter must satisfy.
- RFC 6141 — re-INVITE handling; confirms mid-dialog Record-Route does not alter the route set.
- RFC 5658 — multi-entry Record-Route practice for proxies that must distinguish sides
  (informative background for the two-entry pair, §7).
- RFC 8439 — ChaCha20-Poly1305 AEAD (encrypted mode, §4).
- RFC 2104 (§5 truncation) and FIPS 180-4 — HMAC-SHA-256 (authenticated-only mode, §4).
- RFC 4648 §5 — base64url encoding (unpadded, §5).
- RFC 4086 — randomness requirements for the nonce source.
- sipx kernel contracts consumed: the lossless message model (Record-Route/Route values and their
  URI parameters re-serialize verbatim), URI parameter access on route headers.
- Our specs consumed by / consuming this one: [proxy-behavior](proxy-behavior.md) §5 (P2/P3: the
  verdict input, `403` on failure), §7 row F4 (mint point and byte budget), §11 (what tokens do
  *not* cover: transaction affinity); [hook-framework](hook-framework.md) §5 class (b) and gate
  G5 — module facts ride inside this token under the sub-budget this spec fixes (§3, §5).

**Out of scope:** how the proxy consumes the verdict (proxy-behavior.md), flow references and the
connection table (AF-2), the owner RPC (AF-3), the key configuration schema and reload mechanics
(DP-1 — this spec fixes the *required attributes* only), Path minting semantics (M3, see §7 M7),
routing-policy content behind `policy version`, and the **inner framing of module facts** — the
token carries the module-facts region as opaque bytes; its per-fact structure is owned by
[hook-framework](hook-framework.md) §5.

**Upstream considerations** (AGENTS.md rule 6): considered for upstream: no — the token is
cluster-specific routing state (its fields name platform concepts: tenants, shards, edges, media
nodes, module facts) and carriage uses the kernel's existing URI-parameter surface as-is; the
typed `Path` header the M3 work needs is already a ledger row in [upstream.md](../upstream.md).

## 2. Sans-IO contract

Mint and verify are pure functions. Time enters as an input, randomness as an injected source, and
keys as data; neither function reads a clock, touches a socket, or consults a store.

```rust
struct Claims { tenant: u32, home_shard: u16, edge: u16, direction: Direction,
                media_node: u16, policy_version: u32, expiry: u32,
                module_facts: Bytes /* 0..=64 B, opaque here; framing: hook-framework §5 */ }
enum Direction { Originating, Terminating }         // §3, §7
enum Verdict { Valid(Claims), Invalid(Reason) }     // Reason is telemetry-only; never on the wire

fn mint(claims: &Claims, key: &MintKey, nonce: [u8; 12]) -> Token;
fn verify(bytes: &[u8], keys: &KeySet, now: u32, expect: &Expect) -> Verdict;
```

`Expect` carries the ingress context the proxy already has: an optional pinned tenant (a listener
or interconnect bound to one tenant) and, when a second consecutive platform Route was popped, the
partner token for the pair check (§8, S9). Verification is stateless by design (§9).
`module_facts` is returned verbatim in the verdict — mint and verify never interpret it.

## 3. Byte layout (version 1)

All multi-byte integers are big-endian. The token is a fixed-offset record with one
variable-length region at the end of the body — parsing is offset arithmetic, no varints, no
allocation.

| Offset | Len | Field | Type | Meaning |
|---|---|---|---|---|
| 0 | 1 | version | u8 | Layout version, `0x01`. Anything else: reject (§8, S1) |
| 1 | 1 | key id | u8 | Selects key **and algorithm** from configuration (§6) |
| 2 | 12 | nonce | [u8; 12] | Mint-unique; the AEAD nonce in encrypted mode. Not a replay defense (§9) |
| 14 | 4 | tenant | u32 | Logical tenant id; `0` reserved (none/system) |
| 18 | 2 | home shard | u16 | Rendezvous shard owning the connection-bound side's registration state; `0` = none |
| 20 | 2 | edge affinity | u16 | Logical id of the minting edge — the connection-owner hint for requests toward a connection-bound side (delivery semantics: AF-2/AF-3); `0` = none |
| 22 | 1 | direction | u8 | `0x01` ORIG, `0x02` TERM — the dialog side that presents this token (§7); all other values: reject |
| 23 | 2 | media node | u16 | Logical id of the media relay (e.g. an rtpengine instance) holding the dialog's media session; `0` = no media session |
| 25 | 4 | policy version | u32 | Tenant policy/config version at mint; lets an edge detect stale cached policy without a lookup |
| 29 | 4 | expiry | u32 | Absolute UNIX seconds; `u32` is valid until 2106 |
| 33 | 1 | facts len | u8 | Length `F` of the module-facts region, `0 ≤ F ≤ 64` (the sub-budget, see below) |
| 34 | F | module facts | bytes | Opaque to this layer: contributed by extension modules at hook phase H9 into the F4 mint draft, returned verbatim in the verdict (hook-framework §5 class b). Inner framing: hook-framework |
| 34 + F | 16 or 12 | tag | — | AEAD tag (16 B, encrypted mode) or truncated HMAC (12 B, authenticated-only mode) |

Bytes 0–13 are the **header** (always cleartext — the parser needs version and key id before any
cryptography, and the AEAD needs the nonce). Bytes 14 through 33 + F are the **body** (cleartext
in authenticated-only mode, ciphertext in encrypted mode; ChaCha20 is length-preserving, so
offsets are identical in both modes).

**[sipx-clstr] The module-fact sub-budget is 64 bytes** (`F ≤ 64`), the ceiling
[hook-framework](hook-framework.md) G5 enforces at startup over Σ `TokenFact.max_bytes` of a
selected module set. This spec is the budget authority; 64 B is adopted because it fits the F4
budget with headroom (§5 shows the arithmetic at F = 0 and F = 64). The `facts len` byte is the
region's own framing: it MUST equal the body length minus 20 (§8, S5), so a token is
self-describing and truncation cannot masquerade as a shorter facts region.

**Totals (the arithmetic):**

- header: 1 + 1 + 12 = **14 B**
- body: (4 + 2 + 2 + 1 + 2 + 4 + 4) + 1 + F = 19 + 1 + F = **20 + F B**, F ∈ [0, 64]
- token, encrypted mode: 14 + 20 + F + 16 = **50 + F B** → 50 B (F = 0) … 114 B (F = 64)
- token, authenticated-only mode: 14 + 20 + F + 12 = **46 + F B** → 46 B (F = 0) … 110 B (F = 64)

**Width rationale.** Key id: at most two keys are verify-valid at once under the rotation rule
(§6), so 256 wrapping ids cover decades of rotations. Tenant `u32`: logical ids assigned by
configuration; 4 bytes outlast any realistic tenant count and keep the id opaque (never a name).
Shard/edge/media `u16`: cluster-internal logical ids; 65 535 of each is far beyond the design
scale, and per the design rule tokens carry **no hostnames or raw node identifiers** — a logical
id is meaningless without the cluster's own configuration. Direction gets a full byte: one bit
would pack into a flags field, but a fixed-offset byte keeps the parse trivial and leaves 254
values to reject as corruption. Nonce 12 B: matches the RFC 8439 nonce exactly, so the mint nonce
*is* the AEAD nonce with no derivation step, and 96 random bits keep the birthday bound harmless
under the per-key mint ceiling (§7, M4). Expiry in whole seconds: second granularity is ample
against a default 24 h lifetime and a 30 s skew allowance. Facts len `u8`: one byte frames the
region; values above 64 are invalid by rule, not by width — the width leaves room if a future
version renegotiates the sub-budget.

## 4. Cryptography and confidentiality

Two algorithms exist in version 1. The algorithm is a property of the **key** (§6), not of the
token: key id lookup yields `(secret, algorithm)`, and the algorithm fixes the tag length and
therefore the valid total-length range (§3).

| Algorithm id (config) | Construction | Tag | Body |
|---|---|---|---|
| `chacha20-poly1305` | RFC 8439 AEAD; key 32 B; nonce = token nonce; AAD = header (bytes 0–13); plaintext = body | 16 B | encrypted |
| `hmac-sha256-96` | tag = HMAC-SHA-256(secret, header ‖ body) truncated to its first 12 bytes; key 32 B | 12 B | cleartext |

**[sipx-clstr] rules:**

- Every token is authenticated; there is no unauthenticated mode. Rationale: Record-Route
  contents echo back from untrusted endpoints — an unauthenticated token is an open redirect into
  the cluster (design, alternatives considered).
- **Encrypted mode is the default.** Deployments MUST use `chacha20-poly1305` keys unless they
  explicitly accept exposing every body field to endpoints and on-path elements;
  `hmac-sha256-96` is that explicit opt-out (e.g. a single-tenant lab).
- Tag comparison in `hmac-sha256-96` MUST be constant-time. AEAD open is atomic per RFC 8439.
- No decision may branch on unauthenticated body bytes: the tag is checked (or the AEAD opened)
  before the facts framing, expiry, direction, or any scope field is read (§8 order).

**Truncation bound (why 96 bits is enough).** The tag can only be tested against the cluster's
online verifier — the key never leaves the cluster, so there is no offline oracle, and recovering
the key from tags is a 2^256 search. Forgery success after `q` submitted guesses is ≤ q·2⁻⁹⁶: an
attacker sustaining 10⁶ forged mid-dialog requests per second for a year makes q ≈ 3.2·10¹³ < 2⁴⁵
attempts, for success probability ≤ 2⁻⁵¹ — each attempt costing a full SIP request answered `403`.
RFC 2104 §5 sanctions truncation to ≥ 80 bits and ≥ half the output; 96 is comfortably above both.
The Poly1305 tag is fixed at 16 B by RFC 8439; its forgery bound is stronger still.

**Confidential fields.** In encrypted mode the entire body is ciphertext:

| Field | Confidential? | Why |
|---|---|---|
| tenant | yes | Business-sensitive: the token is visible to both endpoints and every on-path element; the far side of an interconnect must not learn the near side's tenant identity |
| home shard, edge affinity, media node | yes | Internal topology — cleartext would let an attacker aim load at the specific shard, edge, or media node carrying a victim's call |
| policy version | yes | Reveals per-tenant configuration churn cadence |
| module facts | yes | Contributed by extension modules for internal routing decisions; contents are module-defined and must be assumed sensitive — the region rides inside the ciphertext wholesale |
| direction, expiry, facts len | incidental | Not secrets, but nothing outside the cluster needs them and the parse does not need them before key lookup — so they ride inside the ciphertext |
| version, key id, nonce | no (cleartext by necessity) | Needed to parse and select the key before any cryptography. They leak only rotation cadence and randomness. The token's *total length* remains observable and quantizes with F — a deployment for which the module-fact set itself is sensitive should note that ciphertext length is not hidden |

## 5. URI carriage and the size budget

The token travels as a URI parameter on the platform's own Record-Route URIs (and therefore on
the Route values endpoints derive from them; Path in M3 uses the same parameter, §7 M7).

**[sipx-clstr] rules:**

- Parameter name: **`aft`**. Registered lowercase; matched case-insensitively per RFC 3261
  §19.1.4. It does not collide with any RFC-defined URI parameter.
- Encoding: **base64url, unpadded** (RFC 4648 §5). The alphabet (`A–Z a–z 0–9 - _`) falls
  entirely inside RFC 3261 §25.1 `unreserved`, so the value needs **no percent-escaping**, and
  omitting padding removes `=`, which would need escaping. The value is case-sensitive; decoding
  MUST reject padding characters and any byte outside the alphabet.
- Exactly one `aft` parameter per platform URI. A Route value that resolves to the platform and
  carries zero or multiple `aft` parameters fails verification (§8) — there is no tokenless
  platform Route on a mid-dialog request.
- The URI host is a cluster-wide service identity (any edge recognizes and pops it —
  proxy-behavior §5), never an individual node name; the node-level facts ride encrypted inside
  the token, keeping the design rule that public tokens carry no internal identifiers.

**Worked example** (test key and fixtures of §10; token AT-1, empty facts region):

```
;aft=AQGgoaKjpKWmp6ipqqsMq3hYTeXCqKEP-hT8-tSR9LWTv4lIVor6ECKn1SaVRa_euZs

Record-Route: <sip:edge.cluster.example;lr;aft=AQGgoaKjpKWmp6ipqqsMq3hYTeXCqKEP-hT8-tSR9LWTv4lIVor6ECKn1SaVRa_euZs>
```

**Budget arithmetic (end-to-end, at both ends of the facts region).** Unpadded base64url of `n`
bytes is `4·⌊n/3⌋` chars plus 0/2/3 for `n mod 3` = 0/1/2; the parameter adds `;aft=` (5 B):

| Layout | Raw token | base64url | Parameter | ≤ 200 B (F4)? |
|---|---|---|---|---|
| Encrypted, F = 0 | 50 B | 50 = 3·16 + 2 → 67 chars | **72 B** | yes — headroom 128 B |
| Encrypted, F = 64 (ceiling) | 114 B | 114 = 3·38 → 152 chars | **157 B** | yes — headroom 43 B |
| Authenticated-only, F = 0 | 46 B | 46 = 3·15 + 1 → 62 chars | **67 B** | yes — headroom 133 B |
| Authenticated-only, F = 64 | 110 B | 110 = 3·36 + 2 → 147 chars | **152 B** | yes — headroom 48 B |

The provisional ≤ 200 B budget of proxy-behavior §7 F4 is **verified**: worst case **157 B**,
with the full 64 B module-fact sub-budget on board. F4's ceiling SHOULD stay at 200 B — the spare
43 B is version-1 headroom for a future layout, not an invitation to grow this one. One wording
flag for the F4 re-review: the normative budget is the token **parameter**; vector PB-F-1's
shorthand "Record-Route + token ≤ 200 B" reads as the whole header line, which at the facts
ceiling exceeds 200 B for any realistic host (202 B in the example below) — the parameter
reading is the one this spec verifies.

**Against UDP MTU (RFC 3261 §18.1.1).** A dialog-forming request gains the two-entry pair (§7).
With the example host above, each `Record-Route` line including CRLF is 117 B at F = 0 (234 B per
request, token parameters 2 × 72 = 144 B of it) and 202 B at the F = 64 ceiling (404 B per
request). A typical dialog-forming INVITE of 800–1100 B stays under the 1300 B threshold at which
§18.1.1 forces a congestion-controlled transport with the empty region; profiles that spend the
full sub-budget on UDP-heavy edges are spending real MTU headroom, and hook-framework G5 is the
gate that makes that spend explicit per deployment. The token's share is bounded here so the
proxy spec can budget the rest (host part, other headers) independently.

## 6. Key model and rotation

**Config-first (design decision):** keys arrive exclusively via deployment configuration (schema
owned by DP-1), reloadable without restart. There is no key-exchange protocol, no discovery, and
no key material in any message. Required attributes per key entry:

| Attribute | Meaning |
|---|---|
| `id` | u8, the wire key id (§3) |
| `algorithm` | `chacha20-poly1305` or `hmac-sha256-96` (§4) |
| `secret` | exactly 32 bytes |
| `verify_from`, `verify_until` | the validity window within which this key verifies tokens |
| `mint` | boolean; the key new tokens are minted under |

**[sipx-clstr] rules (the config loader MUST enforce):**

- Exactly one `mint: true` key at any config version, and its verify window covers
  `now + L + S` (lifetime and skew, §7/§8).
- No two entries share an `id` while both verify windows are open — ids may wrap over the years,
  never overlap.
- A key id absent from the current key set is an **unknown key**: verification fails (§8, S2).

**Rotation is distribute-then-activate:**

| # | Step |
|---|---|
| K1 | Add key B with `mint: false`, `verify_from ≤ now` to the configuration; reload every node |
| K2 | Confirm every node holds B (a deployment concern — DP observability, not this spec) |
| K3 | Flip `mint` from A to B in one config change; reload |
| K4 | Keep A verify-valid until `t_switch + L + S` (last possible A-mint plus maximum token lifetime plus skew), then remove A |

Rationale for the order: between K1 and K3 every node can verify B-tokens before any node mints
one, so a reload wave can never produce a token some healthy edge rejects. Rationale for K4's
bound: every token minted under A expires by `t_switch + L`, so when A retires, no *live* token
references it.

**Tokens under a retired key** are hard-rejected (unknown key id → `403`, §8). Followed
procedure means only already-expired tokens are affected. Retiring early is permitted for exactly
one reason — key compromise — and its stated cost is that mid-dialog requests of dialogs minted
under the retired key are answered `403`: killing those dialogs is chosen over routing on tokens
an attacker may now forge. Blast radius of a compromise is otherwise bounded by `L` and rotation.

## 7. Mint rules

**[sipx-clstr] rules:**

| # | Rule |
|---|---|
| M1 | Tokens are minted where proxy-behavior §7 F4 record-routes: dialog-forming requests the platform stays in the path of. One entry per side — the **pair** — one fresh token per entry. Registrations (Path) follow in M3, see M7 |
| M2 | **Direction.** ORIG names the dialog side that sent the dialog-forming request; TERM the side it was forwarded to. Each token's direction field names the side that will *present* it: the entry facing the originating side carries ORIG, the entry facing the terminating side carries TERM. The mint pushes the ORIG entry first and the TERM entry on top of it, so route-set learning (RFC 3261 §12.1.1 in order for the UAS, §12.1.2 reversed for the UAC) hands each endpoint its own side's entry as the first of the pair |
| M3 | The pair's claims are identical except direction and nonce: same tenant, home shard, edge affinity, media node, policy version, expiry, **and byte-identical module-facts region**. Verification enforces this (§8, S9) |
| M4 | Nonces come from the injected randomness source (RFC 4086), fresh per token — the two entries of a pair MUST NOT share one. A `(key id, nonce)` pair MUST never repeat: nonce reuse under an AEAD key is catastrophic (RFC 8439 §4). A key MUST be rotated before 2³² mints; random 96-bit nonces then collide with probability ≤ (2³²)²∕2⁹⁷ = 2⁻³³ |
| M5 | `expiry = now + L`, with `L` the configured token lifetime. Default **L = 86 400 s (24 h)**; configurable, floor 600 s. The rotation overlap (§6 K4) scales with `L` — a deployment raising `L` accepts slower key retirement |
| M6 | **No mid-dialog refresh — expiry outlives the dialog.** RFC 3261 §12.2.1.2/§12.2.2 forbid recomputing the route set on target refresh, and RFC 6141 confirms it: a token minted into a re-INVITE's Record-Route can never reach the peer's route set, so an effective in-dialog refresh mechanism does not exist in the protocol. Decision: the platform does not Record-Route target refresh requests (proxy-behavior F4 already scopes Record-Route to dialog-forming requests); a target refresh is an ordinary mid-dialog request — token verified, forwarded, nothing re-minted. A dialog outliving `L` fails **explicitly**: its next in-dialog request is answered `403` (§8) — never mis-routed. Deployments whose dialogs plausibly exceed 24 h raise `L`; that is the only mechanism, and specifying a "refresh" the peer cannot adopt would be false comfort |
| M7 | **Path (M3).** Registration flows mint the same layout into Path URIs with the same `aft` parameter and budget; the direction value for Path tokens is fixed by the M3 stories when the upstream typed Path header lands ([upstream.md](../upstream.md)). Version 1 reserves no additional fields for it |
| M8 | **Module facts.** The facts region is handed to `mint` as opaque bytes, `0 ≤ F ≤ 64`, assembled from `ContributeTokenFact` effects at hook phase H9 into the F4 mint draft (hook-framework §3/§5); Σ of declared `max_bytes` over a profile is gated at startup (G5), so a conforming deployment cannot present an over-budget region to `mint`. `mint` MUST reject F > 64 regardless — the layout ceiling is not the hook framework's to relax |

## 8. Verification

The normative algorithm, in order — the first failing step wins:

| # | Step | On failure |
|---|---|---|
| S1 | Structure: length ≥ 46 (the smallest valid token, §3) and `version == 0x01` | Invalid (parse) |
| S2 | Key lookup: key id present in the key set with `verify_from ≤ now ≤ verify_until` | Invalid (unknown key) |
| S3 | Length in the key's algorithm range: 50–114 (`chacha20-poly1305`) or 46–110 (`hmac-sha256-96`) — the tag length is fixed by the algorithm, the rest is body | Invalid (parse) |
| S4 | Authenticate: AEAD open (AAD = header, nonce from header) or constant-time compare of the truncated HMAC (§4). Only now is the body readable | Invalid (tag) |
| S5 | Facts framing: the `facts len` byte equals body length − 20 (which the S3 range already caps at 64) | Invalid (framing) |
| S6 | Expiry: reject iff `now > expiry + S`, with skew allowance `S` = 30 s default, configurable. `now` is an input (§2) — the sans-IO clock rule | Invalid (expired) |
| S7 | Field validity: `direction ∈ {0x01, 0x02}` | Invalid (field) |
| S8 | Context scope: if the ingress pins a tenant (`Expect`), it MUST equal the token's tenant | Invalid (scope) |
| S9 | Pair consistency: when the proxy popped two consecutive platform Routes (the normal mid-dialog shape given M2), the partner token MUST pass S1–S7 under the same rules, carry equal claims apart from direction — including a byte-identical module-facts region — have the **complementary** direction, and a distinct nonce. The **first-popped** token governs (its direction names the presenting side) | Invalid (pair) |

A single platform Route with a valid token is processed on that token alone — heterogeneous route
sets (foreign proxies between the entries, strict-routing predecessors per proxy-behavior §5 P1)
make a mandatory-pair rule wrong, and Path (M7) presents single entries by nature.

**[sipx-clstr] failure behavior (normative):**

- Every failure collapses to one verdict: `Invalid`. On a mid-dialog request the proxy answers
  **`403 Forbidden`** — proxy-behavior §5 P3 and vectors PB-P-4/PB-P-5 are the consuming rules.
  There is **no fallback routing, no guessing, no degraded mode**: an unverifiable token means
  the request is not routable here, by definition (design decision).
- `Reason` never reaches the wire — one status code for every failure. Distinguishing tampered
  from expired from unknown-key on the wire would hand an attacker a debugging oracle; telemetry
  gets the reason, the network does not.
- Missing token: a first Route resolving to the platform without exactly one `aft` parameter
  (§5) is a verification failure like any other — `403`.
- Timing: S4's comparison is constant-time; steps S1–S3 may return earlier, which reveals only
  structural facts the sender already knows, never key-dependent state.

## 9. Replay semantics

Normative, and deliberately so — this section is the design's settled answer, not an omission:

- **Verification is stateless.** There is no nonce ledger, no seen-token store, and an
  implementation MUST NOT add one. Re-presenting the same token on **every mid-dialog request is
  the mechanism**, not an attack: the token is the dialog's routing state, and the whole point is
  that any edge verifies it with zero shared state. A replay store would reintroduce exactly the
  hot-path global state this token exists to remove. The nonce provides mint uniqueness and
  unlinkability (§3, M4) — it is **not** a replay defense.
- **What possession grants — routing continuation only.** A verified token routes an in-scope
  mid-dialog request; it authenticates nothing about the sender. Request validation, including
  the authentication hook (proxy-behavior §4, V7), still runs per profile; and the claims are
  logical ids meaningful only inside the cluster's configuration — a token addresses no internal
  node directly. Module facts return verbatim into hook phases as read-only context
  (hook-framework §5) and grant nothing by themselves.
- **Cross-context abuse is bounded by expiry and scope.** A token captured from dialog A and
  presented on a fabricated request verifies — by design. What the attacker gets is bounded by
  the claims: that tenant, that direction, that home shard and media node, until `expiry + S`.
  This is the accepted trade recorded in the design: the bound is the scope fields plus `L`,
  not per-message freshness.
- **Considered and rejected — binding the token to dialog identifiers** (e.g. a Call-ID hash):
  the presenting endpoint chooses the dialog identifiers on its own requests, so the binding
  constrains no attacker who can also choose them; it would add request-parse coupling to
  `verify` (breaking the pure `bytes → verdict` shape of §2) for no bound gained. Rejected.
- **Exposure is assumed.** The token rides in Route/Record-Route, visible to both endpoints and
  every on-path element; that is why confidential fields are encrypted (§4) and why possession
  must confer nothing beyond scoped routing.

## 10. Test vectors

Fixtures are fixed and documented; every vector below is deterministic and byte-exact. The test
keys MUST NOT appear in any deployment configuration.

**Test key set:**

| id | algorithm | secret (hex) |
|---|---|---|
| `0x01` | `chacha20-poly1305` | `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| `0x02` | `hmac-sha256-96` | `404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f` |

**Fixed clock and claims:** mint time `T0 = 1785240000` (0x6A6899C0, 2026-07-28T12:00:00Z);
`L = 86 400` → `expiry = 1785326400` (0x6A69EB40); skew `S = 30`. Claims: tenant 7, home shard 3,
edge affinity 5, media node 9, policy version 41. Nonces:
`N1 = a0a1a2a3a4a5a6a7a8a9aaab`, `N2 = b0b1b2b3b4b5b6b7b8b9babb`, `N3 = c0c1c2c3c4c5c6c7c8c9cacb`,
`N4 = e0e1e2e3e4e5e6e7e8e9eaeb`, `N5 = f0f1f2f3f4f5f6f7f8f9fafb`, `N6 = d0d1d2d3d4d5d6d7d8d9dadb`,
`N7 = 909192939495969798999a9b`, `N8 = 808182838485868788898a8b`. Facts fixtures:
`FACTS8 = deadbeefcafef00d` (8 B), `FACTS64 = 000102…3f` (the 64 incrementing bytes).

Body plaintexts (per §3: claims ‖ facts len ‖ facts):

```
ORIG, F=0: 0000000700030005010009000000296a69eb4000   (20 B)
TERM, F=0: 0000000700030005020009000000296a69eb4000   (20 B)
ORIG, F=8: 0000000700030005010009000000296a69eb4008deadbeefcafef00d   (28 B)
```

### Round-trip vectors

**AT-1 — mint, encrypted mode, ORIG, empty facts** (key 0x01, nonce N1):

```
header     : 0101a0a1a2a3a4a5a6a7a8a9aaab
ciphertext : 0cab78584de5c2a8a10ffa14fcfad491f4b593bf
tag        : 8948568afa1022a7d5269545afdeb99b
token (50B): 0101a0a1a2a3a4a5a6a7a8a9aaab0cab78584de5c2a8a10ffa14fcfad491f4b593bf8948568afa1022a7d5269545afdeb99b
param (72B): ;aft=AQGgoaKjpKWmp6ipqqsMq3hYTeXCqKEP-hT8-tSR9LWTv4lIVor6ECKn1SaVRa_euZs
```

`verify` at `now = T0 + 60`, no pinned tenant → `Valid{tenant: 7, home_shard: 3, edge: 5,
direction: ORIG, media_node: 9, policy_version: 41, expiry: 1785326400, module_facts: []}`.

**AT-2 — mint, encrypted mode, TERM, empty facts** (the pair partner; key 0x01, nonce N2):

```
token (50B): 0101b0b1b2b3b4b5b6b7b8b9babb977d21affdeb898241769f86af4afa5a472b12a638519de0fd110e7abc7276fd595692d7
param (72B): ;aft=AQGwsbKztLW2t7i5uruXfSGv_euJgkF2n4avSvpaRysSpjhRneD9EQ56vHJ2_VlWktc
```

`verify` at `T0 + 60` → `Valid{…, direction: TERM, …}`. Pair (AT-1, AT-2) in either pop order
passes S9: equal claims, complementary directions, distinct nonces, identical (empty) facts.

**AT-3 — mint, authenticated-only mode, ORIG, empty facts** (key 0x02, nonce N3, body cleartext):

```
token (46B): 0102c0c1c2c3c4c5c6c7c8c9cacb0000000700030005010009000000296a69eb4000de92133ef3f0478943fa12cf
param (67B): ;aft=AQLAwcLDxMXGx8jJyssAAAAHAAMABQEACQAAAClqaetAAN6SEz7z8EeJQ_oSzw
```

`verify` at `T0 + 60` → `Valid` with AT-1's claims.

**AT-4 — parse round-trip.** Decode AT-1's parameter value (base64url, unpadded) → exactly the
50 token bytes above; split: header = bytes 0–13, tag = last 16, ciphertext between; AEAD open
with key 0x01, nonce = bytes 2–13, AAD = bytes 0–13 → plaintext body equals `ORIG, F=0`
byte-exact; facts len 0 = body length − 20 (S5). Re-encoding the 50 bytes reproduces the
parameter character-exact.

**AT-5 — mint with an 8-byte module-facts region** (key 0x01, nonce N4, facts = FACTS8):

```
token (58B): 0101e0e1e2e3e4e5e6e7e8e9eaeb0a28d0afb13885e699c2a026683367522a0ecff48b7c8ac253a2ed4452e800ba6d9eb3dce4871f5f0066e53b
param (83B): ;aft=AQHg4eLj5OXm5-jp6usKKNCvsTiF5pnCoCZoM2dSKg7P9It8isJTou1EUugAum2es9zkhx9fAGblOw
```

`verify` at `T0 + 60` → `Valid{…, module_facts: deadbeefcafef00d}` — the region returns
verbatim, uninterpreted.

**AT-6 — mint at the facts ceiling, F = 64** (key 0x01, nonce N5, facts = FACTS64) — the budget
vector: 114 raw bytes, 152 encoded chars, parameter 157 B ≤ 200 (§5):

```
token (114B): 0101f0f1f2f3f4f5f6f7f8f9fafbc04c3ec187abded25efe92fd872d11bb5c60e4d20282e14939807603959516badb1fbf91908d41944ffe6d515c19445099ffbc9c9529a209ef3c2f4698fddc959a720db8bf22c727899cb40ca72de5a4eb6261d830daf9ae55904a17c7c1ad1fdab57649
param (157B): ;aft=AQHw8fLz9PX29_j5-vvATD7Bh6ve0l7-kv2HLRG7XGDk0gKC4Uk5gHYDlZUWutsfv5GQjUGUT_5tUVwZRFCZ_7yclSmiCe88L0aY_dyVmnINuL8ixyeJnLQMpy3lpOtiYdgw2vmuVZBKF8fBrR_atXZJ
```

`verify` at `T0 + 60` → `Valid{…, module_facts: 000102…3f}`.

### Negative vectors

Unless noted, `verify` runs at `now = T0 + 60` against the full test key set, no pinned tenant.
Every rejection is `Invalid` and, on a mid-dialog request, `403` (PB-P-4/5).

| # | Given | Expect |
|---|---|---|
| AT-7 | AT-1 with the last tag byte XOR 0x01: `…afdeb99a` | Invalid at S4 (tag) |
| AT-8 | AT-1 with ciphertext byte 14 XOR 0x01: `…aaab0dab78…` | Invalid at S4 (tag — the AEAD authenticates the ciphertext) |
| AT-9 | AT-1 at `now = 1785326431` (= expiry + S + 1) | Invalid at S6 (expired). Boundary: at `now = 1785326430` (= expiry + S) it is still Valid |
| AT-10 | AT-1 against a key set holding only key 0x02 (key 0x01 retired) | Invalid at S2 (unknown key) — rejected before any cryptography |
| AT-11 | AT-1 with `Expect{tenant: 8}` (ingress pinned to tenant 8) | Invalid at S8 (scope) |
| AT-12 | Pair (AT-1, then the token below): partner minted with the ORIG body under nonce N2 — both individually valid, directions ORIG+ORIG | Invalid at S9 (pair: directions not complementary) |
| AT-13 | Token below: direction byte 0x03, valid tag (key 0x01, nonce N6, F = 0) | Invalid at S7 (field) — the tag verifies; the value is still rejected |
| AT-14 | AT-1 with byte 0 = 0x02 (version) | Invalid at S1 (parse) — before key lookup; the broken tag is never reached |
| AT-15 | AT-1 truncated to 49 bytes | Invalid at S3 (parse: below the 50-byte AEAD minimum) |
| AT-16 | Parameter value of AT-1 with `==` padding appended | Rejected at decode (§5: unpadded, alphabet-only) — S1 is never reached |
| AT-17 | Token below: facts len byte = 0x05 but an 8-byte region (body 28 B), valid tag (key 0x01, nonce N7) | Invalid at S5 (framing: 5 ≠ 28 − 20) |
| AT-18 | Token below: facts len byte = 0x41 (65) with 65 facts bytes — total 115 B, valid tag (key 0x01, nonce N8) | Invalid at S3 (parse: 115 > 114, the AEAD maximum — the 64-byte sub-budget is a length bound before it is ever a framing check) |

Fixture bytes for the minted negatives:

```
AT-12 (50B):  0101b0b1b2b3b4b5b6b7b8b9babb977d21affdeb898242769f86af4afa5a472b12a62f5bedcfc9ed0982905f32de15a8be94
AT-13 (50B):  0101d0d1d2d3d4d5d6d7d8d9dadbf44534e6a85ade8308fdf459d6d171297e6b63af3f00146ddc46d2501f50053fc4d9356e
AT-17 (58B):  0101909192939495969798999a9baf8a6d9c880c3a7fe3377248adca0ec5edf2c9437df9b9d059cb51001ccd9e00980879389d498751caf6e0db
AT-18 (115B): 0101808182838485868788898a8b7f4417c0d508dc301bb82ca3b66faabf4e75378ce3575789d320c778c1763476842492859cc4cb86dd56041dc380171ac2d3e66633195122164b34c2092df8b65b2588b694a32ad431ccbea9fb4118518a607eafe326c52773c0e51bd236b8953176862240
```

AF-4 derives its mint/verify tests from these vectors verbatim; AF-5 carries AT-1/AT-2 through
the proxy's Record-Route → Route path in the harness; hook-framework HF-7 exercises the G5 gate
that keeps profiles inside the 64-byte sub-budget AT-6 pins.
