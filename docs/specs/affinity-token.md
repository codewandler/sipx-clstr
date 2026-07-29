# Spec: Affinity token and flow references

**Status:** normative · **Crate:** _future — created by CX-2_ · **Stories:** AF-1, AF-2, AF-4,
AF-5, AF-7 · **Design:** [cluster-affinity](../designs/cluster-affinity.md)

Two record formats live here, because they share one key set, one cryptographic construction and
one parser prologue: the **affinity token** (§2–§10) carries a dialog's routing state in the
message so any edge can forward a mid-dialog request with no lookup, and the **flow reference**
(§11–§14) names the one resource that cannot ride in a message — a client's connection — together
with the node that owns it. §11.3 is the domain separation that keeps the two from ever being
mistaken for one another.

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
- **Flow references (§11–§14) additionally:** RFC 5626 §5.2 — generating flow tokens: an edge
  proxy that encodes a flow into a Path URI rather than keeping a table index MUST make the token
  unforgeable and untamperable (the RFC's own construction is an HMAC over the flow identifier).
  §11 is that construction, generalized so the token names the *owning node* as well as the flow,
  which is what a cluster needs and a single proxy does not. RFC 5626's `430 Flow Failed` is the
  M3 answer for the dead-flow case §13 answers `480` today. RFC 3261 §18.2.1 and RFC 3581 — the
  observed source address a connection row records. RFC 3261 §26.2.2 — the `sips` scheme's
  hop-by-hop TLS requirement, which is why the transport class is a field of the reference rather
  than a question for the owner. RFC 5923 — connection reuse: what may be sent on a connection is
  decided by its validated identity, not by its 5-tuple, so the row carries the identity. RFC 7118
  — SIP over WebSocket, the `WS`/`WSS` transport classes.
- sipx kernel contracts consumed: the lossless message model (Record-Route/Route values and their
  URI parameters re-serialize verbatim), URI parameter access on route headers; for §12, the
  transport layer's accepted-connection handle and its TLS session facts.
- Our specs consumed by / consuming this one: [proxy-behavior](proxy-behavior.md) §5 (P2/P3: the
  verdict input, `403` on failure), §7 row F4 (mint point and byte budget), §7 F7/F10 and its
  empty-target-set `480` (the consuming rules for §13), §11 (what tokens do *not* cover:
  transaction affinity); [hook-framework](hook-framework.md) §5 class (b) and gate
  G5 — module facts ride inside this token under the sub-budget this spec fixes (§3, §5);
  [location-service](location-service.md) §4 (the binding's `flow_ref` column), §7 L7 (`Target`
  carries it verbatim) and §5.3 B4 (the idempotency interaction §13.3 BI5 records).

**Out of scope:** how the proxy consumes the verdict (proxy-behavior.md), the owner RPC's
transport, node-to-node authentication and queueing (AF-3 — §13.2 fixes only the outcomes it must
distinguish), the key configuration schema and reload mechanics (DP-1 — this spec fixes the
*required attributes* only), Path minting semantics (M3, see §7 M7), RFC 5626 outbound semantics,
`430 Flow Failed` and UDP flows (M3, see §11.4 FM6), routing-policy content behind
`policy version`, and the **inner framing of module facts** — the token carries the module-facts
region as opaque bytes; its per-fact structure is owned by
[hook-framework](hook-framework.md) §5.

**Upstream considerations** (AGENTS.md rule 6):

- The token: considered for upstream — **no**. It is cluster-specific routing state (its fields
  name platform concepts: tenants, shards, edges, media nodes, module facts) and carriage uses the
  kernel's existing URI-parameter surface as-is; the typed `Path` header the M3 work needs is
  already a ledger row in [upstream.md](../upstream.md).
- The flow reference and the connection table: considered for upstream — **no**, cluster-specific.
  A flow reference names *this platform's* node set and *this node's* table slot, and RFC 5626
  §5.2 leaves the construction of a flow token to the edge proxy that mints it, so there is no
  protocol-generic form to lift; the connection table is orchestration over the handle the kernel's
  transport layer already returns, not a second transport. Nothing new joins the ledger here.

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
  `now + max(L, E_max) + S` — the longest a record minted under it can still be presented. `L` is
  the token lifetime and `S` the skew (§7/§8); `E_max` is the tenant's maximum registration expiry
  (location-service §5.2 E5, default 86 400 s), which is what bounds a **flow reference** minted
  under it, since a reference carries no expiry of its own and leaves circulation only when the
  binding holding it refreshes (§11.4 FM7). With the defaults, `L = E_max = 86 400 s`, so the two
  terms are equal; without the `E_max` term a single configuration with `E_max > L` and **no
  rotation at all** would let the mint key expire out from under references it minted. BI6's clamp
  only ever lowers a connection-bound grant, so `E_max` stays a valid ceiling.
- No two entries share an `id` while both verify windows are open — ids may wrap over the years,
  never overlap.
- A key id absent from the current key set is an **unknown key**: verification fails (§8, S2).

**Rotation is distribute-then-activate:**

| # | Step |
|---|---|
| K1 | Add key B with `mint: false`, `verify_from ≤ now` to the configuration; reload every node |
| K2 | Confirm every node holds B (a deployment concern — DP observability, not this spec) |
| K3 | Flip `mint` from A to B in one config change; reload |
| K4 | Keep A verify-valid until `t_switch + max(L, E_max) + S` — the last possible A-mint, plus the longest a record minted under it can still be presented, plus skew — then remove A |

Rationale for the order: between K1 and K3 every node can verify B-tokens before any node mints
one, so a reload wave can never produce a token some healthy edge rejects. Rationale for K4's
bound, one term per record family:

- `L` bounds A-minted **tokens**: every one of them expires by `t_switch + L`, so when A retires
  no *live* token references it.
- `E_max` bounds A-minted **flow references**, which have no expiry of their own (§11.4 FM7). A
  reference leaves circulation when the binding carrying it is refreshed — the refresh re-presents
  the reference the owner has since re-minted under B (§11.4 FM3) — and a binding that is *not*
  refreshed expires within its granted lifetime, capped by `E_max` (location-service §5.2 E5).
  Either way, by `t_switch + E_max` no live binding still carries an A-minted reference.

With the default `L` and `E_max` both 86 400 s the bound is numerically what it was; a deployment
that raises either one lengthens every rotation by the same amount. **The `E_max` term holds only
while a location binding is a reference's sole carrier** — see the caveat under §11.4.

**Tokens and references under a retired key** are hard-rejected: unknown key id → `403` for a
token (§8 S2), and `Invalid` at FV2 for a reference, which removes the target (§13.1 D3) and ends
in `480` once the set empties (D8). Followed procedure means only already-expired tokens and
already-replaced references are affected. Retiring early is permitted for exactly one reason —
key compromise — and its stated cost is now two-sided: mid-dialog requests of dialogs minted under
the retired key are answered `403`, and requests toward clients whose bindings still carry
references minted under it are answered `480` until each binding refreshes. Killing those dialogs
and delaying those calls is chosen over routing on records an attacker may now forge. Blast radius
of a compromise is otherwise bounded by `L`, `E_max` and rotation.

## 7. Mint rules

**[sipx-clstr] rules:**

| # | Rule |
|---|---|
| M1 | Tokens are minted where proxy-behavior §7 F4 record-routes: dialog-forming requests the platform stays in the path of. One entry per side — the **pair** — one fresh token per entry. Registrations (Path) follow in M3, see M7 |
| M2 | **Direction.** ORIG names the dialog side that sent the dialog-forming request; TERM the side it was forwarded to. Each token's direction field names the side that will *present* it: the entry facing the originating side carries ORIG, the entry facing the terminating side carries TERM. The mint pushes the ORIG entry first and the TERM entry on top of it, so route-set learning (RFC 3261 §12.1.1 in order for the UAS, §12.1.2 reversed for the UAC) hands each endpoint its own side's entry as the first of the pair |
| M3 | The pair's claims are identical except direction and nonce: same tenant, home shard, edge affinity, media node, policy version, expiry, **and byte-identical module-facts region**. Verification enforces this (§8, S9) |
| M4 | Nonces come from the injected randomness source (RFC 4086), fresh per token — the two entries of a pair MUST NOT share one. A `(key id, nonce)` pair MUST never repeat: nonce reuse under an AEAD key is catastrophic (RFC 8439 §4). A key MUST be rotated before 2³² mints; random 96-bit nonces then collide with probability ≤ (2³²)²∕2⁹⁷ = 2⁻³³ |
| M5 | `expiry = now + L`, with `L` the configured token lifetime. Default **L = 86 400 s (24 h)**; configurable, floor 600 s. The rotation overlap (§6 K4) scales with `max(L, E_max)` — a deployment raising `L` above the maximum registration expiry accepts slower key retirement, and one raising registration expiry above `L` does the same |
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

## 11. Flow references **[sipx-clstr]**

Everything a mid-dialog message needs to be routed rides in the message (§2–§10). A client's
TCP/TLS/WS connection is the one thing that cannot: it is a file descriptor in one node's kernel,
it cannot be serialized, and it cannot move. So the location binding does not store the
connection — it stores an authenticated **reference** to it, and the reference names the node that
owns it. Delivering to such a binding is: resolve → verify the reference → the owner falls out of
the reference itself → write locally, or one RPC to the owner (§13).

**The invariant this section exists for:**

> A flow reference resolves to the connection it was minted for, or to nothing at all. It never
> resolves to a different connection.

"Or to nothing" is not a weakness: a reference whose connection is gone is *detectably* dead, and
detectably dead is what lets the caller move to another registered flow instead of writing a
request into a stranger's socket. Everything in §11–§13 — the incarnation field, the generation
counter, the domain separation — is that invariant made mechanical.

### 11.1 Sans-IO contract

Mint and verify are pure functions, as §2. Keys are data, randomness is an injected source, and —
unlike the token — **there is no `now` at all**:

```rust
struct FlowId { node: u16, incarnation: u32, connection: u32, generation: u32 }   // §11.2
struct FlowClaims { tenant: u32, flow: FlowId, transport: Transport }
enum Transport { Tcp = 1, Tls = 2, Ws = 3, Wss = 4 }                              // §11.2
enum FlowVerdict { Valid(FlowClaims), Invalid(Reason) }   // Reason is telemetry-only, as §2

fn mint_flow(claims: &FlowClaims, key: &MintKey, nonce: [u8; 12]) -> FlowRef;
fn verify_flow(bytes: &[u8], keys: &KeySet, expect: &FlowExpect) -> FlowVerdict;
```

`FlowExpect` carries the same ingress context §2's `Expect` does — an optional pinned tenant — and
nothing else: there is no pair check, because a flow reference is presented alone (§8 S9's pair
rule is a property of the Record-Route pair, not of references).

**Why `verify_flow` takes no clock.** A flow reference has no time dimension (§11.4 FM7): its
liveness is a property of an object that either exists or does not, and asking that question is
resolution's job (§13.2), which reads a table rather than a clock. The one place time still enters
is the key set: §6 gives each key a `verify_from`/`verify_until` window, and the set handed to
`verify_flow` is by definition the **verify-valid** set at this moment. Narrowing it is the
driver's, on configuration reload or when a window elapses — a fired timer, per AGENTS.md rule 2 —
not a per-call clock read.

### 11.2 Byte layout (version 1)

Big-endian, fixed offsets, no variable region — the parse is offset arithmetic on a
constant-length buffer.

| Offset | Len | Field | Type | Meaning |
|---|---|---|---|---|
| 0 | 1 | version | u8 | `0x81` — the flow-reference family (§11.3). Anything else: reject (FV1) |
| 1 | 1 | key id | u8 | Selects key **and algorithm** from the §6 key set — the same key set that mints tokens |
| 2 | 12 | nonce | [u8; 12] | Mint-unique; the AEAD nonce in encrypted mode. The `(key id, nonce)` uniqueness rule spans both record families (FM5) |
| 14 | 4 | tenant | u32 | Logical tenant id, the §3 id space; `0` reserved (none/system) and invalid here (FV5) |
| 18 | 2 | node | u16 | Logical id of the **owning edge** — the §3 `edge affinity` id space; `0` invalid (FV5) |
| 20 | 4 | incarnation | u32 | The owner's incarnation: UNIX seconds at which that node began accepting connections (§12.2 CT2) |
| 24 | 4 | connection | u32 | Slot index in the owner's connection table (§12.1) |
| 28 | 4 | generation | u32 | Slot reuse counter, bumped every time the slot is re-allocated (§12.2 CT3/CT4); `0` invalid (FV5) |
| 32 | 1 | transport | u8 | `0x01` TCP, `0x02` TLS, `0x03` WS, `0x04` WSS; every other value rejects (FV5). `0x05` is reserved for the RFC 5626 UDP flows M3 adds (FM6) |
| 33 | 16 or 12 | tag | — | AEAD tag (16 B, encrypted mode) or truncated HMAC (12 B, authenticated-only mode) — §4 verbatim |

Bytes 0–13 are the **header**, identical in shape to §3's: version and key id must be readable
before any cryptography, and the AEAD needs the nonce. Bytes 14–32 are the **body**, cleartext in
authenticated-only mode and ciphertext in encrypted mode.

**Totals.** header 14 + body 19 + tag ⇒ **49 B** (`chacha20-poly1305`) or **45 B**
(`hmac-sha256-96`), fixed. There is no variable region, so length is an *equality* check rather
than a range (FV3), and the canonical text form (base64url, unpadded, per §5's encoding rules) is
66 or 60 characters. Text form is for telemetry, configuration dumps and vectors; a reference
travels between nodes as raw bytes over the owner RPC, and its M3 carriage in a Path URI is M3's
decision, not this section's.

**Flow identity.** Bytes 18–31 — `node ‖ incarnation ‖ connection ‖ generation`, 14 bytes — are
the **flow identity**: the tuple that names one connection, once, for all time. Two references
name the same flow **iff** their identities are equal; nonce, key id, tag and encoding are not
part of it. Consumers asking "is this the connection I already have?" compare identity, never
bytes (§13.1 D6) — bytes change under a re-mint (FM3) while the identity does not.

**Width rationale.** `node` and `tenant` reuse §3's widths and id spaces deliberately: the edge
that mints a token with `edge affinity = 5` is the node a reference with `node = 5` names, and a
second id space for the same nodes would be a second thing to get wrong in configuration.
`connection` u32: the slot index is bounded by the node's `max_connections` (§12.2 CT8), far below
2³²; the width is chosen so the slot space never has to be recycled tightly. `generation` u32: at
one reconnect per second on the *same slot* it lasts 136 years, and it is never wrapped (CT4).
`incarnation` u32: whole seconds, valid until 2106 like §3's expiry, and monotone across restarts
by CT2 rather than by luck — within the clock assumption CT2 states. `transport` gets a full byte for the same reason `direction` does —
a fixed-offset byte keeps the parse trivial and leaves 250 values to reject as corruption. Per the
design rule, no field is a hostname, a port or a raw node identifier: a reference is meaningless
without the cluster's own configuration.

### 11.3 Domain separation from the affinity token

Both families are minted under the same keys (§6), and in M3 both may appear as URI parameters on
platform URIs. Accepting one where the other is expected would be a cross-protocol confusion with
real consequences — a token body reinterpreted as a flow body would name an arbitrary node and
slot. The separation is therefore **cryptographic, not conventional**:

| # | Rule |
|---|---|
| DS1 | Version `0x01` is an affinity token; version `0x81` is a flow reference. The high bit marks the family; `0x00` and `0x80` are never assigned, so a zeroed buffer is not a valid record of either kind |
| DS2 | Byte 0 is inside the authenticated input of both algorithms — the AEAD's AAD is the header (§4), and the HMAC covers `header ‖ body`. Rewriting byte 0 to change a record's family therefore invalidates the tag, and no attacker without the key can repair it (FR-20) |
| DS3 | The reverse direction is already normative: §8 S1 rejects any version other than `0x01`, so a flow reference presented to `verify` fails before key lookup (FR-19); FV1 does the same for a token presented to `verify_flow` (FR-18) |

Reusing the version byte rather than adding a type field is deliberate: the parser dispatches on
byte 0 either way, and one shared 14-byte header means one prologue, one key lookup and one AEAD
call site for both families.

### 11.4 Mint rules

**[sipx-clstr] rules:**

| # | Rule |
|---|---|
| FM1 | A reference is minted by the edge that **accepted** the connection, at accept time, and cached in that connection's table row (§12.2 CT3). No other node may mint a reference naming it — an owner is the only thing that can know a slot's generation |
| FM2 | Claims come from the row, never from a message: `tenant` and `transport` from the accepting listener's trust context and transport class, `node`/`incarnation`/`connection`/`generation` from the row's identity |
| FM3 | **The bytes are stable for the life of the connection.** `mint_flow` runs again for a live connection only when the mint key changes (§6 K3); such a re-mint preserves the flow identity and changes only nonce, key id and tag. Rationale: a reference is an identifier, and one that changed under a client's retransmission would make "is this the same flow?" depend on when the question was asked. Consumers compare identity (D6), which survives a re-mint anyway; byte stability is the stronger property that makes the weaker one cheap to rely on. It is **not** an idempotency requirement — the registrar's retry check does not compare `flow_ref` at all (BI5) |
| FM4 | Every reference is authenticated; **encrypted mode is the default**. §4 applies verbatim — same algorithms, same key attributes, same constant-time comparison, same rule that no decision branches on unauthenticated body bytes |
| FM5 | Nonces come from the injected randomness source (RFC 4086), fresh per mint. §7 M4's `(key id, nonce)` uniqueness requirement spans **both** families: tokens and flow references minted under one key draw from one nonce space, and the 2³² per-key mint ceiling counts both. Reusing a nonce across families under one AEAD key is exactly as catastrophic as reusing it within one |
| FM6 | **Connection-oriented transports only.** A binding registered over UDP carries no reference (`flow_ref: None`): a UDP client has no connection for anyone to own, and a request toward it is carried by the dataplane's 5-tuple stickiness (proxy-behavior §11), not by an owner. Transport value `0x05` is reserved for the RFC 5626 outbound flows M3 adds |
| FM7 | **No expiry field** — see below |

**Why a flow reference has no expiry.** The token needs one (§7 M5) because it names a *dialog*,
which has no representation anywhere in the cluster; only time can bound it. A reference names an
object that exists — a row in one node's table — and that object's death is the bound: resolution
fails the instant the connection closes, the slot is reused, or the node restarts (§13.2). That is
strictly tighter than any clock, and it needs no clock. Adding one on top would buy nothing and
cost one of two things: either a healthy long-lived connection loses reachability with no recovery
path (the client's refresh re-presents the same expired bytes), or the owner re-mints on a live
connection and FM3 breaks. What a reference confers is bounded instead by the connection's own
life, and the routes to using one are bounded further — it is reachable only through a live
location binding, which expires under the tenant's registration maximum (location-service §5.2 E5).

**Consequence for key rotation — why §6 carries an `E_max` term.** A token expires, so `L` bounds
how long an old key must keep verifying one. A reference does not, so something else must bound
it: it leaves circulation when the binding holding it is refreshed (the refresh re-presents the
re-minted bytes) or when that binding expires unrefreshed, and both are capped by the tenant's
maximum registration expiry `E_max`. That is why §6's mint-window rule and K4 both read
`max(L, E_max) + S` rather than `L + S`. This is stated once, normatively, in §6 — the config
loader is what enforces it — and the reasoning lives here.

**Caveat, and it is the load-bearing assumption:** the `E_max` term bounds circulation only while
a **location binding is a reference's sole carrier**. That holds in M1/M2 by construction — a
reference exists in exactly two places, the owner's table row and the bindings registered over
that flow. It would stop holding the moment a reference is carried anywhere that is not refreshed
on a registration cadence; M3's Path-URI carriage (§11.2) is exactly that, because a route set an
endpoint has learned is never recomputed (RFC 3261 §12.2.1.2). Whatever M3 decides about carrying
references in Path must therefore either re-bound rotation or give the reference an expiry after
all — it cannot inherit this argument unexamined.

### 11.5 Verification

The normative algorithm, in order — the first failing step wins, exactly as §8:

| # | Step | On failure |
|---|---|---|
| FV1 | Structure: length ≥ 45 (the smallest valid reference) and `version == 0x81` | Invalid (parse) |
| FV2 | Key lookup: key id present in the verify-valid key set (§11.1) | Invalid (unknown key) |
| FV3 | Length is **exactly** the algorithm's: 49 (`chacha20-poly1305`) or 45 (`hmac-sha256-96`). No variable region exists, so this is equality, not a range | Invalid (parse) |
| FV4 | Authenticate: AEAD open (AAD = header, nonce from header) or constant-time compare of the truncated HMAC (§4). Only now is the body readable | Invalid (tag) |
| FV5 | Field validity: `transport ∈ {0x01…0x04}`, `tenant ≠ 0`, `node ≠ 0`, `generation ≠ 0` | Invalid (field) |
| FV6 | Context scope: if the caller pins a tenant (`FlowExpect`), it MUST equal the reference's tenant | Invalid (scope) |

**[sipx-clstr] failure behavior.** Every failure collapses to `Invalid`, and `Reason` is telemetry
only — §8's rule and its rationale (no debugging oracle on the wire) apply unchanged. What the
proxy does with an `Invalid` target is §13.1 D3: the target is removed before any branch exists.
A verified reference is *not* a delivery guarantee — it says who to ask, and §13.2 says what the
answer can be.

**What possession grants.** §9's replay reasoning carries over with the expiry bound replaced by
the connection's own life: a reference is re-presented on every delivery toward that binding by
design, verification is
stateless, and there is no ledger. What a captured reference grants is "deliver this request to
that connection" for as long as that connection lives — which is the same capability any request
addressed to that AoR already has, since the binding is what published it. It authenticates
nothing about the sender, and it addresses no node directly: `node = 5` is meaningless without the
configuration that maps 5 to an address.

## 12. The connection table **[sipx-clstr]**

One table per node, owned by the driver — it wraps sockets, so it is not sans-IO. Its *decision*
content is: which identity a new connection gets, and what a given identity resolves to. Both are
pure functions over the rows, and both are what the harness executes; the socket is not.

### 12.1 Schema

One row per accepted connection.

| Field | Form | Source | Notes |
|---|---|---|---|
| `connection` | u32 | table | slot index; unique within one `(node, incarnation)` |
| `generation` | u32 | table | slot reuse counter, ≥ 1 (CT3/CT4) |
| `transport` | `Tcp` / `Tls` / `Ws` / `Wss` | listener | fixed at accept; the reference's transport byte. A `sips` target MUST NOT be delivered over `Tcp` or `Ws` (RFC 3261 §26.2.2), which is checkable from the reference alone |
| `local` | (listener id, ip, port) | listener | which listener accepted it; the listen/advertise split is DP-5's |
| `remote` | (ip, port) | transport driver | the observed peer address — the value location-service §4 stores as `received` (RFC 3261 §18.2.1, RFC 3581) |
| `tenant` | logical tenant u32 | listener trust context | fixed at accept, never taken from a message |
| `principal` | opaque authenticated id, optional | RG-2 | absent until a request on this flow authenticates; set at most once (CT7) |
| `tls` | optional `{version, cipher suite, peer identity, verified}` | TLS driver | `None` for `Tcp`/`Ws`. `peer identity` is the validated certificate identity where one exists (RFC 5923) — the fact that decides what may be sent, rather than the 5-tuple |
| `opened_at` | instant | accept | |
| `last_activity` | instant | driver | last byte read **or** written on the connection; the input the idle timer fires against (§12.3) |
| `flow_ref` | the minted bytes | FM1 | cached at accept; handed to the registrar path for every REGISTER arriving on this flow (§13.3 BI1) |
| `state` | `Open` / `Draining` / `Closed` | §12.2 | |

The row deliberately holds no dialog, transaction or binding state: a connection is a pipe with an
owner, and everything about what travels through it lives in the location service or in the
message.

### 12.2 Lifecycle, identity and the generation rules

**[sipx-clstr] rules:**

| # | Rule |
|---|---|
| CT1 | **Node ids are unique.** A `node` id names exactly one node at every configuration version; AF-6/DP-1's config validation enforces it. Two nodes sharing an id would give two different connections one flow identity — the only way to break §11's invariant from outside this spec, and the reason the check is a validation error rather than a warning |
| CT2 | **Incarnation.** A node takes an incarnation — UNIX seconds — once at startup, before accepting its first connection, and it is an **input** to the table, not a clock the table reads. A node MUST NOT begin accepting until its incarnation is strictly greater than that of its previous run on the same `node` id; a restart inside the same second waits for the next. Rationale: without it a restarted node re-issues `connection`/`generation` from the beginning, and a reference minted before the restart could resolve to a connection accepted after it (FR-9). **Mechanism** (no persistence required): take the process-start second, then delay the first accept until the wall clock strictly exceeds it — at most a one-second wait. **Limit, stated rather than assumed:** this rests on a clock that does not step backwards across a restart. An NTP step back over the incarnation gap reopens FR-9's scenario exactly, so a deployment that cannot rule one out MUST seed the incarnation from a persisted counter instead (AF-6/DP-1's schema) |
| CT3 | **Accept.** Take a free slot; set `generation` to that slot's previous generation + 1, or 1 if the slot has never been used; state `Open`; mint and cache the reference (FM1) |
| CT4 | **Generation never wraps.** `generation` is monotone per slot within an incarnation and is never reused. On reaching `0xFFFFFFFF` the slot is **retired** — off the free list for the rest of the incarnation — rather than wrapped. A wrap is precisely the aliasing CT2 and CT3 exist to prevent, and retiring one slot after 2³² reuses costs nothing |
| CT5 | **Close.** Peer close, transport error, idle timeout (§12.3), drain deadline and shutdown all move the row to `Closed`. A closed row resolves to nothing (§13.2 RS4) |
| CT6 | **Free.** A `Closed` row's slot returns to the free list immediately. Safety comes from CT3's bump, not from a quarantine delay: a reference to the old occupant carries the old generation and can never match the new one |
| CT7 | **Principal is set once.** The first authenticated request on a flow labels it (RG-2). A later request presenting a *different* principal does not relabel the flow; what happens to that request is RG-2's decision, but the table never rewrites a flow's identity underneath references already minted for it |
| CT8 | **Bounded.** `max_connections` per node bounds the table and therefore the slot space; an accept beyond it is refused at the listener rather than by evicting a row, because evicting would kill a live registration to admit an unauthenticated stranger |
| CT9 | **Drain.** On graceful shutdown every row moves to `Draining` and the listeners stop accepting. `Draining` still **delivers** — the socket is open and refusing would drop callable traffic for no gain — until `T_drain` elapses, at which point the rows close (CT5) and every reference naming them is dead |

### 12.3 Timers

Time enters the table as fired timers, never as a clock read (AGENTS.md rule 2). `now` arrives
with the timer.

| Timer | Default | Fires when | Effect |
|---|---|---|---|
| `T_idle` | **3900 s** | `now − last_activity ≥ T_idle` | Close the connection (CT5). Configurable; `0` disables both this timer and BI6's clamp. It MUST exceed the **granted** expiry of every binding the flow carries — not the tenant's *default* interval, which is only E3's fallback and says nothing about what E1/E2 may request and E5 may grant. §13.3 **BI6** is what makes that true, by clamping the grant to `T_idle − M` rather than by binding this timer to a tenant setting the node cannot see. Rationale: until RFC 5626 keepalives arrive in M3, a REGISTER refresh is the only traffic on an idle registration flow, so an idle timer shorter than the refresh cadence closes the very flow the platform just promised to deliver on. The default is location-service §5.2 E3's 3600 s plus the 300 s margin `M`, so a default deployment still grants 3600 s and refreshes comfortably inside the timer |
| `T_drain` | **30 s** | graceful shutdown started `T_drain` ago | Every `Draining` row closes (CT9) |

## 13. Delivery: resolution and binding integration **[sipx-clstr]**

### 13.1 The chain

| # | Rule |
|---|---|
| D1 | The location service returns targets carrying `flow_ref: Option<Bytes>` verbatim (location-service §7 L7). `None` is delivered by ordinary next-hop resolution (proxy-behavior §7 F7); the rest of this section is the `Some` case |
| D2 | **Verify, and the owner falls out.** `verify_flow` (§11.5) is a pure function of the bytes and the key set the node already holds; `claims.flow.node` *is* the owner. There is no directory to consult, no membership query, no cluster-wide lookup — which is the whole point (AGENTS.md rule 5). The chain is one store read and two pure functions |
| D3 | An `Invalid` verdict makes the target **unusable**: it is removed from the target set *before* a branch exists — before proxy-behavior §7 F8 pushes a Via, before F10 forwards. No transaction is started, nothing is sent, and telemetry records the reason the wire never learns |
| D4 | `claims.flow.node == self.node && claims.flow.incarnation == self.incarnation` → **local delivery**: resolve against this node's own table (§13.2) and write. No RPC, and this is the common case — a client's registration and the requests toward it usually meet at the same edge |
| D5 | Otherwise → **the owner RPC** (AF-3) to `claims.flow.node`, carrying the reference and the request. This is the platform's only cross-node signalling hop |
| D6 | **Same-flow comparison is by identity, never by bytes** (§11.2). A consumer asking whether a reference names a connection it already knows compares the 14 identity bytes of the decoded claims; comparing encodings would report a re-mint (FM3) as a different flow |
| D7 | A `sips` target MUST NOT be delivered over a `Tcp` or `Ws` flow (RFC 3261 §26.2.2). The check runs on `claims.transport` at the caller, before D5 — a reference is enough to refuse without asking the owner |

### 13.2 Resolution at the owner

A pure function of the owner's table: `resolve(FlowId, &ConnectionTable) -> Resolution`. Steps in
order, first match wins.

| # | Condition | Result |
|---|---|---|
| RS1 | `id.node ≠ table.node` or `id.incarnation ≠ table.incarnation` | `FlowDead` — a different node, or a previous run of this one (CT2) |
| RS2 | no row at `id.connection` | `FlowDead` |
| RS3 | row's `generation ≠ id.generation` | `FlowDead` — the slot was reused; **this is the reconnect case**: the client came back, got a fresh generation, and every reference to the old connection died at that instant |
| RS4 | row's `state == Closed` | `FlowDead` |
| RS5 | otherwise (`Open` or `Draining`) | `Live(row)` — the write proceeds (CT9) |

**The outcome taxonomy.** AF-3 specifies the RPC that carries these; this spec fixes what they
mean and what each one costs the request, because a taxonomy that collapses them turns a dead
connection into a server error:

| Outcome | Produced by | Meaning | Consequence for the request |
|---|---|---|---|
| `Delivered` | the owner | written to the connection | the normal path |
| `FlowDead` | RS1–RS4, locally or at the owner | the named connection provably does not exist | the target is removed as in D3. The binding is **not** invalidated here: binding lifetime is the location service's (location-service §6), and the next REGISTER refresh replaces the reference |
| `FlowRejected` | the owner | the owner is alive and refusing — its bounded queue for that flow is full, or policy refuses the write | a **branch failure**: `503` semantics per proxy-behavior R10, and therefore R8 (it becomes `500` upstream if it is the best response). The owner is up and saying "not now", which is a server condition, not an unavailable user |
| `OwnerUnreachable` | the caller | the RPC did not reach the owner | the target is removed, as `FlowDead`. Telemetry keeps them distinct: `FlowDead` is a fact about the connection and is final; `OwnerUnreachable` is a fact about *this caller's* view of the network and may heal on its own without anything changing about the flow |

| # | Rule |
|---|---|
| D8 | When every target has been removed by D3, the context concludes `480 Temporarily Unavailable` — proxy-behavior §7's empty-target-set rule, reached by removal rather than by an empty lookup. There is no fallback to a different node, no broadcast, and no guessing: a request toward a connection nobody owns is not deliverable, and saying so is the honest answer |
| D9 | **M3.** RFC 5626 defines `430 Flow Failed` for exactly the `FlowDead` case, so an upstream registrar can drop a dead binding instead of waiting for it to expire. Adopting it is M3's, with the rest of outbound; until then D8's `480` is the answer and no `430` is emitted |

### 13.3 Binding integration (with RG-1)

| # | Rule |
|---|---|
| BI1 | The accepting edge puts the row's cached reference (FM1) into `RegisterCommand.flow_ref` (location-service §2) for every REGISTER arriving on that connection. A REGISTER arriving on a UDP listener carries `None` (FM6) |
| BI2 | The location service stores and returns it **verbatim and uninterpreted** — location-service §4 and §7 L7 already say so, and nothing here changes it. In particular the store never verifies a reference: verification needs the key set, which belongs to the signalling layer, and a store that verified would have to be redeployed on every rotation |
| BI3 | `lookup` yields `Target { contact, route_set, flow_ref, q, expires_in }` (location-service §7). D2 turns `Some(flow_ref)` into an owner with no further lookup. That chain — `lookup` → `verify_flow` → `node` → RPC — is what "a lookup yields the owner" means here, and it touches one store read and two pure functions |
| BI4 | **Tenant agreement.** The reference's `tenant` is §3's logical u32; the location service keys bindings by an opaque UTF-8 tenant id (location-service §4). They are the same tenant under a configuration-owned mapping (AF-6/DP-1). Delivery MUST pin `FlowExpect.tenant` to the tenant the lookup was keyed by (FV6): a reference carrying another tenant's id is a scope violation, not a routing hint |
| BI5 | **The reconnect/idempotency interaction — flagged, not silently decided.** A UA whose connection drops mid-REGISTER reconnects and retransmits: same `Call-ID`, same `CSeq`, new flow reference. location-service §5.3 B4 compares "same granted expiry base, same contact set effect" and says nothing about `flow_ref`, so as written that retransmission is a Noop and the binding keeps a reference whose flow is dead. **The safety invariant is untouched** — the stale reference resolves to nothing (RS3), never to the new connection — so this is a reachability gap of at most one refresh interval, not a mis-delivery, and it self-heals. [RG-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/RG-8-settle-b4-idempotency-so-a-retransmission-is-a-retry.md) has since settled that comparison over the granted duration and the stored Path vector, and deliberately not over `flow_ref` — so this is now an open item for **AF-7**, which is where the flow-aware half would be implemented, not a pending decision elsewhere. This spec's recommendation, for the record: treat a differing **flow identity** — not differing bytes (FM3, D6) — as a state difference the retry applies, replacing `flow_ref` and `received` and leaving the granted expiry and the contact set alone |
| BI6 | **A binding must not outlive the flow it names.** When `RegisterCommand.flow_ref` is `Some` and the accepting node's `T_idle` is non-zero, the effective maximum granted expiry for that binding is `min(tenant max, T_idle − M)`, with the refresh margin `M` = **300 s** by default. This changes no rule in location-service: §5.2 E5 already lowers an over-long request to the maximum silently and states the granted value in the `200` (RFC 3261 §10.3 step 7 permits shortening) — BI6 supplies a second input to that maximum, for connection-bound registrations only. `T_idle = 0` disables the timer and the clamp together |

**Why BI6 exists, in one failure.** E5's tenant maximum defaults to 86 400 s and a client that
asks for it gets it — and a registered client is then idle *by design*, because refreshing is the
only traffic a registration generates until RFC 5626 keepalives arrive in M3. Without the clamp,
an entirely default deployment closes the connection at `T_idle` and the binding survives roughly
23 hours naming a `Closed` row: every request toward that AoR verifies, resolves `FlowDead` at
RS4, loses its target to D3 and is answered `480` by D8 — with nothing to shorten it, since §13.2
deliberately does not invalidate bindings and D9 defers `430 Flow Failed` to M3. The clamp binds
the two lifetimes to each other at the one place that knows both. Its useful side effect: on a
healthy registration the client's own refresh cadence keeps the flow non-idle, so `T_idle` never
fires at all, and it is left doing the job it is for — closing flows whose client has stopped
refreshing.

## 14. Flow-reference test vectors

Fixtures are fixed and documented; every byte below is deterministic and reproducible. The key set
is **§10's** — deliberately, because one key set mints both families (FM5) and a vector suite that
used a second one would not exercise that.

**Flow fixture:** tenant 7, node 5, connection (slot) `1234` = `0x000004D2`, transport `0x02`
(TLS). Incarnations `I1 = 1785239000` (`0x6A6895D8`) and, after a restart of the same node,
`I2 = 1785318000` (`0x6A69CA70`). Nonces, distinct from §10's `N1…N8` because the nonce space is
shared (FM5): `P1 = 0102030405060708090a0b0c`, `P2 = 1112131415161718191a1b1c`,
`P3 = 2122232425262728292a2b2c`, `P4 = 3132333435363738393a3b3c`,
`P5 = 4142434445464748494a4b4c`, `P6 = 5152535455565758595a5b5c`.

Body plaintexts (per §11.2: tenant ‖ node ‖ incarnation ‖ connection ‖ generation ‖ transport):

```
generation 1, I1        : 0000000700056a6895d8000004d20000000102   (19 B)
generation 2, I1        : 0000000700056a6895d8000004d20000000202
generation 1, I2        : 0000000700056a69ca70000004d20000000102
generation 1, transport 0x07 : 0000000700056a6895d8000004d20000000107
generation 1, tenant 8  : 0000000800056a6895d8000004d20000000102
```

Flow identities (bytes 18–31 of the reference — body bytes 4–17):

```
(5, I1, 1234, 1) : 00056a6895d8000004d200000001
(5, I1, 1234, 2) : 00056a6895d8000004d200000002
(5, I2, 1234, 1) : 00056a69ca70000004d200000001
```

### Flow round-trip vectors

**FR-1 — mint, encrypted mode, generation 1** (key `0x01`, nonce P1):

```
header    : 81010102030405060708090a0b0c
ciphertext: 6488446d0957cedbdd314547d0738c0ebefd91
tag       : 556fe054b680ba0840b887afbba42d0d
ref (49B) : 81010102030405060708090a0b0c6488446d0957cedbdd314547d0738c0ebefd91556fe054b680ba0840b887afbba42d0d
text (66c): gQEBAgMEBQYHCAkKCwxkiERtCVfO290xRUfQc4wOvv2RVW_gVLaAughAuIevu6QtDQ
```

`verify_flow` with no pinned tenant → `Valid{tenant: 7, flow: (5, I1, 1234, 1), transport: Tls}`.

**FR-2 — mint, authenticated-only mode, generation 1** (key `0x02`, nonce P2, body cleartext):

```
ref (45B) : 81021112131415161718191a1b1c0000000700056a6895d8000004d2000000010239ebd1c4d7c0693b9c0fb697
text (60c): gQIREhMUFRYXGBkaGxwAAAAHAAVqaJXYAAAE0gAAAAECOevRxNfAaTucD7aX
```

`verify_flow` → FR-1's claims. Note the body is readable: in authenticated-only mode a peer sees
the node id, the slot and the generation, which is why FM4 keeps encrypted mode the default.

**FR-3 — parse round-trip.** Decode FR-1's text form (base64url, unpadded) → exactly the 49 bytes
above; split header = bytes 0–13, tag = last 16, ciphertext between; AEAD open with key `0x01`,
nonce = bytes 2–13, AAD = bytes 0–13 → the `generation 1, I1` body byte-exact. Re-encoding the 49
bytes reproduces the text character-exact. Length is exactly 49 (FV3), not a range.

**FR-4 — identity, not bytes.** FR-1 and FR-2 are the same flow minted under different keys and
algorithms: 49 bytes versus 45, no byte in common after offset 1, and the **identical** identity
`00056a6895d8000004d200000001`. D6's comparison reports *same flow*; a byte comparison would
report a different one, which is the bug FM3 and D6 exist to prevent.

**FR-5 — mint after a reconnect on the same slot, generation 2** (key `0x01`, nonce P3):

```
ref (49B) : 81012122232425262728292a2b2caff4fdb93edd23ccc75a0e197432d8666bd0fae9c6f85788094285614199ad5e7bc361
text (66c): gQEhIiMkJSYnKCkqKyyv9P25Pt0jzMdaDhl0Mthma9D66cb4V4gJQoVhQZmtXnvDYQ
```

`verify_flow` → `Valid{…, flow: (5, I1, 1234, 2), …}`. The identity differs from FR-1's in the
generation field alone — one connection replaced another on the same slot.

### Flow resolution vectors

**Connection-table fixture** — node 5, incarnation `I1`, `max_connections` 4096:

| slot | generation | state | transport |
|---|---|---|---|
| 1234 | 2 | `Open` | TLS |
| 1235 | 4 | `Draining` | TCP |
| 1236 | 2 | `Closed` | WS |
| 1300 | — | never allocated | — |

| # | Given | Expect |
|---|---|---|
| FR-6 | `resolve` FR-5's identity `(5, I1, 1234, 2)` | `Live` (RS5) → `Delivered` |
| FR-7 | `resolve` FR-1's identity `(5, I1, 1234, 1)` — the client reconnected onto the same slot | `FlowDead` at RS3. **The generation-bump vector**: the reference verifies perfectly and still resolves to nothing, which is the invariant of §11 doing its work |
| FR-8 | `resolve (5, I1, 1300, 1)` | `FlowDead` at RS2 — no row |
| FR-9 | The node restarts into `I2`; its first connection takes slot 1234, generation 1. Its reference is the 49 bytes below (key `0x01`, nonce P4). `resolve` FR-1's identity at the restarted node | `FlowDead` at RS1. Without the incarnation field the pre-restart `(5, 1234, 1)` and the post-restart `(5, 1234, 1)` would be one identity and FR-1 would deliver into a stranger's connection — this row is why CT2 is a MUST |
| FR-10 | FR-1 evaluated at **node 6** | Not a failure: `verify_flow` is `Valid`, D4 does not match, D5 sends it to node 5 over the owner RPC. The owner came out of the reference; nothing was looked up |
| FR-11 | `resolve (5, I1, 1235, 4)` — the `Draining` row | `Live` (RS5) → `Delivered`. Draining is not dead (CT9) |
| FR-12 | `resolve (5, I1, 1236, 2)` — the `Closed` row | `FlowDead` at RS4 |

```
FR-9 (49B): 81013132333435363738393a3b3c07ad7737a245f43b37f5efdcc12d0f9c140f2200f40c4fa8f339a3a0936d46d38322ad
```

### Flow negative vectors

`verify_flow` runs against the full §10 key set with no pinned tenant unless the row says
otherwise. Every rejection is `Invalid`; §13.1 D3 is what it costs the request.

| # | Given | Expect |
|---|---|---|
| FR-13 | FR-1 with the last tag byte XOR `0x01`: `…a42d0c` | Invalid at FV4 (tag) |
| FR-14 | FR-1 truncated to 48 bytes | Invalid at FV3 — the length is an equality check, so 48 fails even though it is ≥ 45 |
| FR-15 | The 49 bytes below (key `0x01`, nonce P5): transport byte `0x07`, tag valid | Invalid at FV5 (field) — the tag verifies and the value is still rejected |
| FR-16 | FR-1 against a key set holding only key `0x02` (key `0x01` retired) | Invalid at FV2 (unknown key) — before any cryptography |
| FR-17 | The 49 bytes below (key `0x01`, nonce P6): tenant 8, verified with `FlowExpect{tenant: 7}` | Invalid at FV6 (scope) |
| FR-18 | AT-1's 50 bytes (an affinity token) presented to `verify_flow` | Invalid at FV1 — `version 0x01 ≠ 0x81`, before key lookup (DS3) |
| FR-19 | FR-1's 49 bytes presented to `verify` (§8) | Invalid at S1 — `version 0x81 ≠ 0x01`, before key lookup (DS3) |
| FR-20 | AT-1 with byte 0 rewritten to `0x81` (bytes below) | Invalid at FV3 first — 50 bytes is neither 49 nor 45. Were the lengths ever to coincide, FV4 still rejects it: byte 0 is inside the AAD, so the rewrite breaks the tag and no attacker without the key can repair it (DS2) |

```
FR-15 (49B): 81014142434445464748494a4b4c1d468df3f7c3931607672a6ef7a2539eca2e842616b40a3336377c5a06f156546d055a
FR-17 (49B): 81015152535455565758595a5b5cbebb2ae7b8c01b3b452e750911c7ac34067893249bb3a471c6dd9c757b7fb9fb2f6b81
FR-20 (50B): 8101a0a1a2a3a4a5a6a7a8a9aaab0cab78584de5c2a8a10ffa14fcfad491f4b593bf8948568afa1022a7d5269545afdeb99b
```

### Flow integration vector

**FR-21 — a lookup yields the owner.** location-service's LS-L fixture, with target B's binding
carrying FR-5's bytes as `flow_ref`:

1. `lookup(t1, K, now)` returns B first and carries the reference verbatim (LS-L-1, LS-L-6) — the
   store neither parsed nor verified it (BI2).
2. `verify_flow(bytes, keys, FlowExpect{tenant: 7})` → `Valid{flow: (5, I1, 1234, 2), …}` (BI4).
3. At node 6: D5 → owner RPC to node 5. At node 5 with incarnation `I1`: D4 → local resolution →
   FR-6 → `Delivered`.

Total lookups outside the node: one location-service read, which the request needed anyway. No
directory, no membership query, no dialog store — AGENTS.md rule 5 holds on this path by
construction, not by care.

**FR-22 — the binding never outlives the flow (BI6).** Node 5 with `T_idle = 3900`, `M = 300`,
tenant maximum at its 86 400 s default. A client registers over the TLS flow of FR-1's fixture
with `Expires: 86400`:

| Step | Expect |
|---|---|
| a | The command carries `flow_ref: Some(…)` (BI1), so the effective maximum is `min(86 400, 3900 − 300)` = **3600 s**; E5 lowers the grant silently and the `200` states `expires=3600` |
| b | The client refreshes on that cadence, so `last_activity` advances every ~3600 s and `T_idle` never fires — a healthy registration keeps its own flow alive |
| c | The client stops refreshing. `T_idle` fires at 3900 s, CT5 closes the row; the binding has already expired at 3600 s, so no binding ever names a `Closed` row |
| d | Same registration with `T_idle = 0`: no clamp, grant is the tenant's 86 400 s, and no idle timer exists to close the flow underneath it — the two settings move together |
| e | Contrast, the defect BI6 removes: without the clamp, step a grants 86 400 s, step c closes the flow at 3900 s, and every request toward that AoR for the next ~23 h verifies, hits RS4, and is answered `480` (D8) |

AF-7 derives its connection-table and `mint_flow`/`verify_flow` tests from these vectors verbatim,
and its multi-node harness scenario from FR-7, FR-9 and FR-21; FR-22 is a registrar-path test and
belongs with the flow-aware half of AF-7's binding work. The natural home for the two functions is
AF-4's crate, since both families share the key set, the algorithms and the AEAD call site.
