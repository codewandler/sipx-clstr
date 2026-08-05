# Spec: Membership, keys and the shard map

**Status:** normative · **Crate:** _none of its own — the loader is
`crates/sipx-clstr-node`'s (`DP-8`)_ · **Stories:** AF-6 · **Design:**
[cluster-affinity](../designs/cluster-affinity.md)

Three sections of the cluster configuration document, and the operating procedure that goes with
them. [cluster-config](cluster-config.md) §10 fixed the seam — where they live, that they are
versioned with the document and reloadable, and what each must provide — and left the fields
themselves, the rotation runbook's calendar and the question of what a dynamic membership service
would replace to this document. This is that content, and nothing else: **no rule of §1–§9 of that
spec moved when this one landed**, which is what its §10 A6 asked for. What did move is three
pointers — the §7 registry rows, the §10 lead-in and one sentence of the §1 out-of-scope paragraph
now name this file instead of naming a story.

The posture in one sentence, because every rule below follows from it: **the document is the
membership.** A node learns the cluster from bytes it was handed, not from a peer it discovered,
and there is no consensus system, no discovery protocol and no key exchange anywhere in v1
([cluster-affinity](../designs/cluster-affinity.md)).

## 1. Normative references

- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- **RFC 4086** — randomness requirements. Key material and every per-use value §8 tabulates are
  held to it; §8 is where the requirement is stated once for the whole subsystem.
- **RFC 3339** §5.6 — the instant form `verifyFrom` / `verifyUntil` take (§4 KY4).
- RFC 3986 §3.2.2 / RFC 5952 — the address forms a member's `rpc` endpoint may take, already
  parsed and refused by `DP-5`'s rules and inherited through
  [cluster-config](cluster-config.md) §5 P7 rather than restated.
- This repo's specs, each of which owns what this one consumes:
  [cluster-config](cluster-config.md) §2 D1–D5 (pure loading, one cluster-scoped document, three
  encodings, `lowerCamelCase` keys), §3 D7/D9/D10/D11, §5 P1/P3/P7, §6 I1–I4, §7 (the registry row
  per section), §8 V2/V3/V4/V8/V9/V10, §9.1 RL1–RL4, §9.3 RL9–RL12, §9.4 DS1–DS7;
  [affinity-token](affinity-token.md) §3 (the wire widths of the ids), §4 (the two algorithms and
  the encrypted-mode default), §6 (the key entry's required attributes and rotation steps K1–K4),
  §7 M4/M5 (the nonce source and the token lifetime `L`), §8 S2/S6 (the verify window and the skew
  allowance `S`), §11.4 FM7 (a flow reference carries no expiry), §12.2 CT1/CT2 (node-id
  uniqueness and the incarnation), §12.3;
  [location-service](location-service.md) §5.2 E5 (`E_max`, the maximum registration expiry) and
  §8 (the shard key this map assigns ownership over);
  [sipx-cluster-crd](sipx-cluster-crd.md) §7 S1/S4/S6 (`status` is observation, silence is
  `Unknown`, and `KeysDistributed` — the observable §7 RB3 requires).

**Out of scope.** The token and flow-reference formats, their cryptography and their verification
order ([affinity-token](affinity-token.md)); the connection-owner RPC's protocol, authentication,
queueing and failure answer ([owner-rpc](owner-rpc.md)) — this spec fixes only the endpoint a peer
dials; the hash
function, the weights and the rebalancing policy behind a shard assignment (`RG-5`); how a secret
is *stored* and how a reference is resolved into it (driver work, [cluster-config](cluster-config.md)
§8 V9); the operator's staging of a change across nodes and zones
([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md));
the spelling of the observability fields §6 RD8 requires (`DP-3`); and the loop-cookie key, which
is per process and stays there (§8, `PX-15`).

**Upstream considerations** (AGENTS.md rule 6): **no — this is orchestration.** Distributing keys
across *our* cluster's membership names this platform's own node set, its zones, its shard map and
its configuration document, none of which the kernel has a concept of; the primitives underneath
— AEAD, HMAC, access to the operating system's CSPRNG — are already kernel- or crate-level and are
consumed here rather than re-implemented. Nothing new joins [the ledger](../upstream.md).

## 2. What this is, and what it is not

| # | Rule |
|---|---|
| CM1 | **These three sections carry no policy that another spec owns.** `membership` names nodes, `keys` names key entries, `shardMap` assigns ownership. Everything about what a key *is*, what a node *does* with a shard, and what a token *means* belongs to the spec that owns it, and this document cites rather than restates — [cluster-config](cluster-config.md) §8 V3's rule about defaults applies to citations too: a value adopted here is adopted unchanged, and where this spec declares a default it is because no other spec had one. |
| CM2 | **The document is the same bytes on every node** ([cluster-config](cluster-config.md) §2 D2). There is no per-node key set, no per-node membership view, and no field whose value a node computes for itself. A node that held a different key set from its peers is precisely the node [affinity-token](affinity-token.md) §6 K1 exists to prevent, and it would be invisible: it verifies its own mints. |
| CM3 | **A membership entry is a declaration, not an observation.** An entry may name a node that has never started, and a running node may have no entry ([cluster-config](cluster-config.md) §5 P3). Nothing in loading, reloading or validation waits on a node to appear, because a document that could only be accepted once the fleet matched it would be a consensus protocol with a YAML syntax. What is observed is reported in `status`, never written back into the document ([sipx-cluster-crd](sipx-cluster-crd.md) §7 S1). |
| CM4 | **Refusing to start is the only failure mode** ([cluster-config](cluster-config.md) §8 V10). No rule below degrades, warns and continues, or falls back to a previous value for one field. |

## 3. `membership`

A sequence of members, one per node the deployment intends to run. The sequence shape is
[cluster-config](cluster-config.md)'s (`cluster.membership[…]` is the path its errors already
name); this section fixes the entry.

```yaml
cluster:
  membership:
    - node: 7
      name: edge-a
      zone: eu-west-1a
      roles: [edge, registrar]
      rpc: "10.0.0.7:7223"
      incarnationSource: boot-second
```

| Field | Form | Required | Default | Meaning |
|---|---|---|---|---|
| `node` | u16, `1`–`65535` | yes | none | The logical node id [affinity-token](affinity-token.md) §3 carries as `edge affinity` and §11.2 carries in a flow reference. Unique at every configuration version ([cluster-config](cluster-config.md) §6 I2, §12.2 CT1); `0` is reserved for "none" |
| `name` | UTF-8, non-empty | yes | none | The human spelling. It is what `shardMap[].owner` names and what an error message quotes; byte-compared, never folded (§6 I4) |
| `zone` | a name declared in `zones` | yes | none | Cross-checked against the identity the node was started with, never obeyed (§5 P3) |
| `roles` | a set of §4 R1 roles | yes | none | Cross-checked the same way (§5 P3), and subject to R6's refused combinations |
| `rpc` | `host [":" port]`, port required | when `roles` intersects the call-path roles (MB5) | none | The **advertised** endpoint a peer dials for the connection-owner RPC (`AF-3`) |
| `incarnationSource` | `boot-second` \| `persisted-counter` | no | `boot-second` | Which mechanism of [affinity-token](affinity-token.md) §12.2 CT2 this node takes its incarnation from |
| `incarnationRef` | a reference name | when `incarnationSource` is `persisted-counter` | none | The driver-resolved handle the counter is read from and written to, named by reference for the same reason a secret is (§8 V9) |

| # | Rule |
|---|---|
| MB1 | **The closed world is exactly the seven fields above** ([cluster-config](cluster-config.md) §8 V2). An unrecognised key at a member is an error naming the path and the seven, not a warning and not ignored. |
| MB2 | **The entry is cross-checked, never obeyed** (§5 P3, unchanged). A member entry for *this* node whose `zone` or `roles` differ from the identity the process was started with is a load error naming both; a node with no entry starts. This spec adds no exception to either half. |
| MB3 | **`node` uniqueness is a correctness input, not a convention** ([affinity-token](affinity-token.md) §12.2 CT1). Two members sharing an id give two different connections one flow identity, and it is the same id [media-relay](media-relay.md) §6.2 C2 needs cluster-unique for NG cookies. A duplicate is refused naming both holders (§6 I2); nothing anywhere may treat it as a warning. |
| MB4 | **`name` is unique in the document**, byte-compared (§6 I4). `shardMap[].owner` resolves by name (SM2), so two members answering to one name would make an ownership assignment ambiguous — and an ambiguous owner is DS2's "a shard accepting at two nodes" reached through the front door. |
| MB5 | **`rpc` is required for a member whose `roles` intersect `{edge, registrar, inbound-proxy, outbound-proxy}` and MUST be absent otherwise.** Those are the roles that accept connection-oriented transports and therefore own flows; `echo` and `e2e-tester` are off the call path (§4 R6, [e2e-probe](e2e-probe.md) §11) and a peer never dials them. Fail-closed in both directions: a missing endpoint on a flow-owning node makes every request toward a client it owns undeliverable, and an endpoint on a node that owns nothing is a target nobody should reach. |
| MB6 | **`rpc` is an advertised address and is unique in the document.** [cluster-config](cluster-config.md) §5 P7 is inherited verbatim — empty, unspecified (`0.0.0.0`, `::`) and port `0` are refused — and the port is **required**, with no default: the RPC endpoint is a deployment fact, and a defaulted port is one the far side has to guess. Two members advertising one endpoint is a load error naming both, because [affinity-token](affinity-token.md) §13.1 D5 dials the owner the reference names and nothing re-checks that the answer came from it. |
| MB7 | **The bind side is not here.** `rpc` is what a peer dials; which address the owning node binds is a listener concern (`DP-5`, `AF-3`), and this spec deliberately does not open a second listener vocabulary. The document is cluster-scoped (D2), so a per-node bind address can only arrive through §8 V4's `${NAME}` substitution or from the identity — and deriving identity from the document is what P1 forbids. |
| MB8 | **`incarnationSource` is a member's field, not a cluster-wide switch.** [affinity-token](affinity-token.md) §12.2 CT2 makes `boot-second` correct only where the clock does not step backwards across a restart, and that is a property of a machine rather than of a fleet: a deployment that cannot rule a backwards step out on *some* of its nodes MUST set `persisted-counter` on those, and expressing it per member is what lets it do so without penalising the rest. `boot-second` is the declared default because it is the mechanism CT2 specifies and it needs no storage; it is not a silent one — omitting the field selects a documented mechanism whose stated limit is written down beside it. |
| MB9 | **A member may be removed while its node is running.** Removal is not a shutdown signal and no node stops because its own entry left the document (CM3, P3). Draining and terminating a node is `KO-4`'s, and a configuration change that could stop a process would make a reload the most dangerous operation in the system. What removal *does* constrain is the id: §7.2 holds a retired `node` id out of circulation for the same window a retired key waits out. |

## 4. `keys`

A sequence of key entries. The attributes are [affinity-token](affinity-token.md) §6's, spelled per
[cluster-config](cluster-config.md) §2 D4; this section adds no attribute and renames none.

```yaml
cluster:
  keys:
    - id: 3
      algorithm: chacha20-poly1305
      secretRef: affinity-key-3
      verifyFrom: "2026-07-28T12:00:00Z"
      verifyUntil: "2026-08-04T12:00:30Z"
      mint: true
```

| Field | Form | Required | Default | Meaning |
|---|---|---|---|---|
| `id` | u8, `0`–`255` | yes | none | The wire key id [affinity-token](affinity-token.md) §3 carries in byte 1 and §8 S2 looks up. **`0` is a valid key id here**, unlike every other id in this schema: [cluster-config](cluster-config.md) §6 I2's reserved zero covers tenant, shard, node and media-node ids, and a token's key-id byte has no "none" value — every record names a key |
| `algorithm` | `chacha20-poly1305` \| `hmac-sha256-96` | no | [affinity-token](affinity-token.md) §4's — adopted here, declared there | Fixes the construction and therefore the tag length and the valid total-length range (§3, §8 S3) |
| `secretRef` | a reference name | yes | none | Resolved by the driver into §6's exactly-32-bytes. The only way material enters a node (§8 V9) |
| `secret` | — | never | none | **Reserved and always refused** (KY3) |
| `verifyFrom` | RFC 3339 instant, UTC | yes | none | The window §8 S2 checks `now` against — opens |
| `verifyUntil` | RFC 3339 instant, UTC | yes | none | …and closes |
| `mint` | boolean | no | `false` | Whether new records are minted under this key. Exactly one entry per document version carries `true` ([affinity-token](affinity-token.md) §6) |

| # | Rule |
|---|---|
| KY1 | **The six attributes are the interface, and they are stable.** `id`, `algorithm`, `secret` (as `secretRef`), `verifyFrom`, `verifyUntil` and `mint` are what [affinity-token](affinity-token.md) §6 requires and what `AF-4`'s mint/verify library consumes: a key set is looked up by `id`, the window decides whether a key verifies, and `mint` selects the one key that mints. Changing any of their names, types or meanings is a new `apiVersion` ([cluster-config](cluster-config.md) §3 D7), never an in-place edit — the library is proved against [affinity-token](affinity-token.md) §10's vectors, so this is a proved surface and a change to it after `AF-4` lands is a breaking change to something that already passes. |
| KY2 | **`secretRef` is the only way key material enters a node**, and resolution is the driver's: an unresolvable reference is a **start-up** failure, not a load error, because resolution is IO (§8 V9, §2 D1). A node MUST refuse to start rather than run with a partially resolved key set. A node holding a subset of the declared keys is exactly the node [affinity-token](affinity-token.md) §6 K1 is written to prevent, and it would report itself healthy: it can verify everything it minted. |
| KY3 | **`secret` is a reserved key, recognised so that writing it is refused for the right reason.** A document carrying an inline `secret` is refused citing §8 V9 — key material is named by reference, never written — rather than §8 V2's "unrecognised key". Both would refuse the document; only one tells the author what to do, and V2's message would read as "this schema has no notion of a secret", which is the opposite of true. **The refusal MUST NOT echo the offending value.** `ConfigError.found` (§8) carries a *description* of what was written — "an inline key secret" — never the bytes, and the same holds for every diagnostic on this path: log line, `status` message, operator output. A rule whose whole content is "this material must not appear in the document" cannot be enforced by a message that prints it, and this refusal fires exactly when a real secret is sitting in the field. This is a redaction requirement, not a formatting preference. |
| KY4 | **Validity windows are absolute instants, never durations.** `verifyFrom` and `verifyUntil` are both required, carry no default, and MUST satisfy `verifyFrom < verifyUntil`. A relative spelling (`verifyFor: 7d`) is unrepresentable on purpose: the loader has no clock (§2 D1), so the window would be resolved against whatever moment each node happened to reload — one document, as many windows as there are nodes, and the disagreement invisible until a token failed at one edge and passed at another. |
| KY5 | **`mint` defaults to `false`, and a document in which no entry carries `true` is refused**, naming the section. [affinity-token](affinity-token.md) §6 fixes "exactly one at any configuration version"; this spec supplies the default and the fail-closed half, because a cluster that mints nothing Record-Routes nothing and would fail on its first dialog-forming request rather than at load. |
| KY6 | **The ceiling is §8 V8's sixteen entries**, adopted unchanged, and it is generous on purpose: §7's rotation keeps at most two entries verify-valid at once, which is also what [affinity-token](affinity-token.md) §3's one-byte key id is sized around. A document approaching the ceiling is a document whose rotations are not being retired. |
| KY7 | **A key is scoped to one cluster and one environment.** Two clusters sharing key material make each one's tokens verify in the other, and a token is authority to be routed as that tenant, in that direction, toward that shard and that media node ([affinity-token](affinity-token.md) §9). Staging and production sharing a key is the same defect with a friendlier name. |
| KY8 | **The [affinity-token](affinity-token.md) §10 test keys MUST be refused**, by the driver at start-up, where the resolved value exists (KY2). They are published in a normative document, so a deployment running one is a deployment whose tokens anyone can mint. §10 already says they must not appear in any deployment configuration; this is the enforcement point, and it is a refusal to start rather than a warning. |
| KY9 | **No node is a source of key material.** A node consumes the key set; it does not generate, derive, wrap, unwrap, forward or re-distribute any part of it, and there is no node-to-node key request. This is [affinity-token](affinity-token.md) §6's "no key-exchange protocol, no key material in any message" stated from the configuration side, and §8 is why it is a rule rather than an omission. |

## 5. `shardMap`

```yaml
cluster:
  shardMap:
    drainTimeout: 30s
    shards:
      - id: 1
        owner: edge-a
      - id: 2
        owner: edge-b
```

| Field | Form | Required | Default | Meaning |
|---|---|---|---|---|
| `drainTimeout` | duration, `5s`–`300s` | no | [cluster-config](cluster-config.md) §9.4 DS4's — declared there | How long a `Draining` shard waits for its last in-flight write before the switch is forced (DS5) |
| `shards[].id` | u16, `1`–`65535` | yes | none | The shard id [affinity-token](affinity-token.md) §3 carries as `home shard`; `0` is reserved for "none" |
| `shards[].owner` | a member `name` | yes | none | The node that accepts writes for this shard's slice of [location-service](location-service.md) §8's key space |

| # | Rule |
|---|---|
| SM1 | **The list is the shard space, and it is total.** Ids are `1..=N` with no gap and no repeat, and `N` is the length of the list — there is no separate count, because a count and a list are two spellings that can disagree. A partial map is refused naming the missing ids: a shard with no owner is a slice of the registration key space for which no REGISTER can be accepted, and it would surface as a tenant's phones going quiet rather than as a configuration error. |
| SM2 | **`owner` names a member by `name`** (MB4), and a name absent from `membership` is a load error at `cluster.shardMap.shards[…].owner` ([cluster-config](cluster-config.md) §8 V5). Name rather than id because a shard map is a table a human reviews in a diff, and sixty-four rows of `owner: 7` are unreviewable; the id the wire carries is one lookup away in `membership`, which is the map §6 I1 already requires to exist. |
| SM3 | **Only a member whose `roles` include `registrar` may own a shard.** A shard owns registration state; assigning one to a node that runs no registrar leaves its writes with nowhere to land. Refused at load naming the shard and the member. |
| SM4 | **The document records the assignment; it does not compute it.** A map may be authored by hand or emitted by the operator, and both produce the same bytes. Rendezvous hashing, the weights and the rebalancing policy are `RG-5`'s and may well be what *produced* a map, but a computed assignment that is not written down cannot be diffed (§2 D5) and cannot be fenced by a version (§3 D9/D11) — and DS2's drain-then-switch is built on both. |
| SM5 | **Changing `N` is not the same operation as changing an owner, and the calendar says so.** Moving an owner is the ordinary §9.4 handoff: only migrating shards drain (DS3), and nothing else in the cluster notices. Changing `N` re-partitions the key space, so every shard id means something new, and a token minted under the old map carries a `home shard` naming a slice of a partition that no longer exists — stale for the same `max(L, E_max) + S` window §7.1 computes, and reaching a consumer as a wrong hint rather than as an error. What a consumer does with a stale hint is `RG-5`'s; what this spec fixes is that `N` is chosen once at deployment, generously, and that ownership — not the partition — is what moves afterwards. That is the entire reason the map has shards in it instead of naming nodes directly. |

## 6. Reload without a restart

All three sections are `reloadable` in [cluster-config](cluster-config.md) §7, and §9.1 RL1–RL4,
§9.3 RL9–RL12 and §9.4 DS1–DS7 govern how. What this section adds is the property those rules add
up to, stated so it can be tested rather than believed, and the handful of transitions that are
this spec's because they are about *its* fields.

| # | Rule |
|---|---|
| RD1 | **"Reloadable" means, exactly:** applying a document that differs from the active one only in these three sections rebinds no listener, closes no connection, expires no registration, invalidates no token and no flow reference, and disturbs no established dialog and no in-flight transaction. Each clause has an owner and none is new here — [cluster-config](cluster-config.md) §9.1 RL4 (dialogs), §9.3 RL12 (keys, in both directions), §9.4 DS6 (shards), and [affinity-token](affinity-token.md) §12.2, whose connection table is not a function of the document at all. Stated as one sentence because that is the claim an operator relies on, and four rules in three sections is not a claim anyone can check. |
| RD2 | **Adding or removing a member is a reload**, with no restart and no quiescence. Adding one changes nothing until that node starts; removing one is MB9. |
| RD3 | **A reload that changes *this* node's own `zone` or `roles` is refused as a reload**, naming both, and the node keeps running the active version. Identity is a start-up input (§5 P1) and no document can change what a process was started as; applying it is a restart, staged by the operator ([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md)). This is §9.1 RL2's shape for a field that has no rollout class of its own — the mismatch is with the identity, not with the previous document. |
| RD4 | **A `node` id is not re-pointed to a different `name` by a reload** ([cluster-config](cluster-config.md) §6 I3). The loader enforces the version-to-version half; §7.2 is the calendar half. |
| RD5 | **A changed `rpc` takes effect for calls begun after the reload.** There is no retained-endpoint rule of §9.2 RL6's kind, because an owner-RPC call is one hop with an explicit outcome rather than a plan built from a table: a call already in flight completes, or fails with the answer `AF-3` specifies, and the caller's next attempt uses the new endpoint. |
| RD6 | **A `keys` reload is §9.3 RL9–RL12, unchanged.** This spec adds no transition rule, because the pair-of-documents rules are already there and a second copy would be one to keep in step. What §7.1 adds is the half RL11 explicitly cannot do: RL11 checks declared windows against each other, and only a wall clock can say whether one has actually elapsed. |
| RD7 | **A `shardMap` reload is §9.4, unchanged** — two maps held at once (DS1), the losing node stopping before the gaining node starts (DS2), only migrating shards draining (DS3), and the active version not advancing until the last shard settles (§3 D11). |
| RD8 | **A node reports what it is running, or the two-phase rules are unenforceable.** Each node exposes the configuration `version` it has applied, the key `id`s it holds at that version, and its shard-map state — nothing else, and never key material or a `secretRef`. The field spellings are `DP-3`'s; the *requirement* is here because it is what makes §7.1 RB3 an observation instead of a hope, and it is the input [sipx-cluster-crd](sipx-cluster-crd.md) §7 S6's `KeysDistributed` is computed from. A fleet that cannot answer "who holds key B" cannot safely be told to mint under it. |

## 7. The runbook

The half of these sections that a validator structurally cannot hold. [cluster-config](cluster-config.md)
§2 D1 forbids the loader a clock, so every rule that compares a declared instant to *now* lives
here, addressed to an operator rather than to a function.

### 7.1 Key rotation: the overlap window and the cutover

The window is [affinity-token](affinity-token.md) §6 K4's and is not restated with different terms:

| Term | What it is | Default | Declared by, and configured at |
|---|---|---|---|
| `L` | token lifetime | 86 400 s | [affinity-token](affinity-token.md) §7 M5, which declares the value, whether it is configurable and what bounds it |
| `E_max` | the **largest** `tenant[].expiry` maximum in the document | 86 400 s | [location-service](location-service.md) §5.2 E5, per tenant |
| `S` | skew allowance | 30 s | [affinity-token](affinity-token.md) §8 S6 |
| `W` | the overlap window, `max(L, E_max) + S` | 86 430 s | [affinity-token](affinity-token.md) §6 K4 |
| `D` | the distribution interval: from publishing a key to every node confirming it holds it | a deployment's own | §6 RD8's report is what measures it |
| `P` | the rotation period | a deployment's own | RB7 fixes only its floor |

| # | Rule |
|---|---|
| RB1 | **`E_max` is the maximum over every declared tenant, not one tenant's setting.** One tenant raising its registration ceiling lengthens every rotation for the whole cluster, because a flow reference carries no expiry of its own and leaves circulation only when the binding holding it refreshes ([affinity-token](affinity-token.md) §11.4 FM7, §6). An operator computes `W` from the document plus the three defaults above and needs nothing else. |
| RB2 | **Generate, then publish.** The secret is 32 bytes from a CSPRNG (§8 UQ2), produced off-node, once, and placed where `secretRef` resolves. Then publish a document adding the new key `B` with `mint: false`, `verifyFrom ≤ now` and `verifyUntil ≥ t_activate + W`, and reload every node — [affinity-token](affinity-token.md) §6 K1. **This document changes `keys` and nothing else**, so reverting it is a revert rather than a judgement (RB8). |
| RB3 | **Confirm distribution before activating, and confirm it from observation.** Every node that verifies records reports holding `B` at the new configuration version (§6 RD8); under Kubernetes that conjunction is `KeysDistributed: True` ([sipx-cluster-crd](sipx-cluster-crd.md) §7 S6). This is K2's "confirm every node holds B", named: the operator MUST NOT proceed while any node is unconfirmed **or unobserved** — [sipx-cluster-crd](sipx-cluster-crd.md) §7 S4 makes silence `Unknown`, and "we have not heard" is not a reason to mint. |
| RB4 | **Activate in one field.** Flip `mint` from `A` to `B` and change nothing else ([affinity-token](affinity-token.md) §6 K3). `t_activate` is the moment the **last** node has applied it, not the first: the earliest node to mint under `B` is minting for a fleet that must already verify it, which is what RB3 bought. |
| RB5 | **Wait `W`.** There is nothing to observe and nothing to do; the window is arithmetic, not a signal. `t_retire ≥ t_activate + W`, and `W` is recomputed if a tenant raises its expiry maximum during the wait — the term is the document's largest at the moment of retirement, not at the moment of activation. |
| RB6 | **Retire `A`** by removing its entry. By construction no live record names it: every `A`-minted token has expired (`L`) and every `A`-minted flow reference has left circulation with the binding that carried it (`E_max`), both plus skew. The id is free for reuse immediately on removal, for that same reason — the wait *is* the quarantine. |
| RB7 | **The next rotation opens no earlier than `t_activate + W + D`.** At most two keys are then verify-valid at once, which is exactly what [affinity-token](affinity-token.md) §3's width rationale assumes when it sizes the key id at one byte, and it is the floor on `P`. Rotating faster does not rotate more safely; it stacks windows. |
| RB8 | **One concern per document.** A rotation document touches `keys` only — never `membership`, never `shardMap`, never a trunk. The reason is the rollback: a document that changed two things has to be reasoned about before it can be reverted, and RB2/RB4 are exactly the two moments at which an operator may need to revert without reasoning. |
| RB9 | **Emergency retirement is restart-class, and deliberately so.** A compromised key must stop verifying now, and §9.3 RL11 refuses a reload that closes a verify window early — a safety rule that could be switched off from the document is one that gets switched off during an incident. The sequence is: run RB2–RB4 for `B` as fast as distribution allows, so that no node is left without a mint key; then perform a **rolling restart** with `A` removed, because `load` has no predecessor and §9.1 RL3 makes every transition rule vacuous at start-up. Nodes still running the old document keep verifying `A` while the roll proceeds, so the exposure is bounded by the roll rather than by `W`; shortening it is the operator's ([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md)). The cost is [affinity-token](affinity-token.md) §6's and is two-sided and already priced: `403` for dialogs minted under `A`, and `480` toward clients whose bindings still carry `A`-minted references until each refreshes. Killing those dialogs and delaying those calls is chosen over routing on records an attacker can forge. |

**The calendar, with every default taken** (`L` = `E_max` = 86 400 s, `S` = 30 s, so `W` = 86 430 s
— just over 24 h):

| Instant | What happens | Gate on proceeding |
|---|---|---|
| `t0` | RB2: `B` published with `mint: false` | none — adding a verify-only key is always safe (K1) |
| `t0 + D` | RB3: every node reports holding `B` | the conjunction is `True`, not `Unknown` |
| `t1 ≥ t0 + D` | RB4: `mint` flips to `B`; `t_activate` is the last node's apply | RB3 |
| `t1 + 86 430 s` | RB6: `A` removed | the wall clock, and nothing else |
| `t1 + 86 430 s + D` | the next rotation may open at RB2 | RB7 |

### 7.2 Id assignment and retirement

[cluster-config](cluster-config.md) §6 I3 fixes that an id must not be reassigned to a different
name while a record minted under the old assignment can still be presented, and assigns the
version-to-version half to the loader and the calendar half here. It applies to every kind of
logical id §6 I1 names — tenant, shard, node and media node — and the arithmetic is the same `W`.

| # | Rule |
|---|---|
| RB10 | **An id is allocated once, from the document, and recorded there.** There is no allocator, no registry service and no derivation from a name: the `name`↔`id` map *is* the document (§6 I1), and assigning the next free id is an authoring step a human or the operator performs against the file they are already editing. A generated id would have to be generated somewhere, and that somewhere would be a second source of truth for the one map [affinity-token](affinity-token.md) §13.3 BI4 says is configuration-owned. |
| RB11 | **A retired id waits `W` before it is reassigned to a different name**, for the reason RB6 gives for a key id: reusing it earlier is indistinguishable, on the wire, from the record it collides with. This is the calendar half of I3, and it binds a `node` id removed by MB9 and a `tenant` id decommissioned by an operator alike. |
| RB12 | **Prefer exhaustion to reuse.** The widths are generous on purpose ([affinity-token](affinity-token.md) §3: `u32` tenants, `u16` nodes, shards and media nodes), so the safe procedure is to allocate forward and never reuse, and to treat RB11 as the rule for the deployment that has genuinely run out rather than as routine practice. An id nobody reuses cannot collide with a record nobody expected. |

## 8. Uniqueness: what may never be derived

`CX-5` is the shape to avoid, and it is worth naming precisely because it looks nothing like a bug
while you are writing it. The kernel's digest nonce is a pure function of the second, the realm and
the secret — nothing per challenge — so two clients challenged in the same second receive
byte-identical nonces and share one replay counter, and the second one's correct password is
refused as a replay. Every producer computed the right answer; the answers collided. The defect is
entropy, not construction, and the rule that closes it is
[registrar-auth](registrar-auth.md) §6.1 N1 in a kernel that has to change.

A key-distribution design fails the same way if a node can *derive* something two nodes derive
identically from inputs they both hold. This section is why this one does not.

| # | Rule |
|---|---|
| UQ1 | **No value in this subsystem that must be unique is a pure function of inputs two nodes share.** Every such value is either assigned in the document and checked unique at load, or drawn per use from the injected randomness source. UQ4 is the whole list, and a new value that fits neither column is a design change rather than an implementation detail. |
| UQ2 | **Key material is generated, never derived.** Thirty-two bytes from a CSPRNG (RFC 4086) at the moment of creation, off-node, once per key. No passphrase, no KDF over the cluster name, the environment, the configuration version or a node id, and no per-node derivation from a cluster master secret. A derived key set is one two nodes can compute — and one an attacker who learns the inputs can compute too, at which point the token's whole authority follows from public facts. |
| UQ3 | **Distribution transports an opaque value; it does not agree on one.** Every node receives byte-identical material through its `secretRef` (§8 V9), from the same place, and KY9 forbids a node to be a source. There is no handshake, no negotiation and no message carrying key material, so there is no agreement step to attack and no state in which two nodes have agreed on different things. |
| UQ4 | **The values that must be unique, and where each gets its uniqueness** — the table below. It is exhaustive for this subsystem on purpose: a reviewer checking UQ1 should be able to check it by reading one table rather than by searching. |
| UQ5 | **The incarnation is the one value a node produces alone, and it is deliberately not random.** [affinity-token](affinity-token.md) §12.2 CT2 needs it *strictly greater* than the previous run's, which randomness cannot promise and monotonicity can. `boot-second` is a pure function of the clock and therefore has exactly CX-5's shape — two runs inside one second produce one value — which is why CT2's mechanism is to wait for the next second rather than to hope, and why `incarnationSource: persisted-counter` (MB8) exists for the deployment whose clock may step backwards. The failure is the same failure; the mechanism that closes it is not entropy, it is the check. |

| Value | Must be unique across | Where its uniqueness comes from | What refuses a collision |
|---|---|---|---|
| key `id` | the verify-valid entries | assigned in the document | load — [affinity-token](affinity-token.md) §6, `CC-K-5` |
| key secret | clusters, environments and time | a CSPRNG at creation, off-node (RB2) | procedure only (UQ2) — no gate can see a secret it must not read |
| token nonce | mints under one key | the injected randomness source | [affinity-token](affinity-token.md) §7 M4 and its 2³² mint ceiling |
| `node` id | the cluster, at every configuration version | assigned in the document | load — [affinity-token](affinity-token.md) §12.2 CT1, `CC-I-1` |
| member `name` | the document | assigned in the document | load — MB4 |
| `rpc` endpoint | the document | assigned in the document | load — MB6 |
| shard `id` | the map | assigned in the document, `1..=N` | load — SM1 |
| incarnation | one node's runs on one `node` id | the boot second, made strictly increasing, or a persisted counter | [affinity-token](affinity-token.md) §12.2 CT2, MB8 |
| `connection` / `generation` | one incarnation | the connection table's slot counter | [affinity-token](affinity-token.md) §12.2 CT3/CT4 |
| loop-cookie key | one process | the operating system's CSPRNG at start-up | `PX-15` |

**The randomness seam is `PX-15`'s and this spec does not open a second one.** Loop-cookie keys are
per process and stay per process: they are not `keys` entries, and nothing in §7 applies to them.
What is shared is the requirement and the shape — key material, cluster-wide or per-process, comes
from the operating system's CSPRNG at the driver boundary and is injected into the sans-IO core as
data (AGENTS.md #2), never read from a clock and never reconstructed from public facts. If
cluster-wide loop detection is ever needed, the loop-cookie key becomes an entry distinguished by an
additive field ([cluster-config](cluster-config.md) §3 D7) and everything in §7 applies to it
unchanged. That is a later decision, `PX-15` is not blocked on it, and until it is taken the six
attributes of KY1 stay exactly six.

## 9. What a future dynamic membership service would replace

Recorded so that config-first is a *choice with a successor*, not a corner. Nothing in §3–§8 has to
be undone for one to arrive, and the reason is DY1.

| # | Rule |
|---|---|
| DY1 | **It would replace the *authoring* of `membership` and `shardMap`, not their place in the document.** A service that emits a new document is a producer, and everything here keeps working: the version still fences (§3 D9/D11), the diff is still reviewable, and the node still reads bytes. A service a node *queried at run time* is something else entirely — a consensus dependency reached on the path that answers calls — and it is what [cluster-affinity](../designs/cluster-affinity.md) and the vision's second principle reject. The distinction is the whole record: **producer, not oracle.** |
| DY2 | **It would not replace `keys`.** A membership service that also distributed key material is a key-exchange protocol, which [affinity-token](affinity-token.md) §6 forbids in as many words, and it would make the node set the trust boundary for the token key: a node admitted to membership would be a node handed the authority to mint. Key distribution stays a reference resolved from a secret store the node was configured with (KY2, UQ3), whatever decides membership. |
| DY3 | **What must survive it, in full:** `node`-id uniqueness (CT1 — a registry that admits a duplicate is worse than a document that does, because nobody reviews a registry); a monotonic, comparable configuration version (§3 D9), without which DS2's drain-then-switch has nothing to fence a late write against; the refusal posture (§8 V10); and byte-identical inputs on every node (§2 D2). A service that weakened any of the four would not be a membership service, it would be a different platform. |
| DY4 | **A node MUST still start with no such service reachable.** The moment starting requires reaching a registry, every call in the cluster depends on that registry's availability, and the design that put routing state in the message to avoid exactly that coupling has reacquired it one layer down. |
| DY5 | **What it would actually remove**, exhaustively: `membership[].rpc` (discovered instead of declared), the hand-authored `shardMap.shards[]` (computed and announced instead — SM4 already anticipates it), and the operator's staging of a change to those two sections. `keys`, the runbook, §8's uniqueness rules and every reload rule in §6 are untouched. |

## 10. What is deliberately not expressible

| Not expressible | Why, and where it belongs |
|---|---|
| A member's health, liveness or weight | CM3, [sipx-cluster-crd](sipx-cluster-crd.md) §7 S1. Health is observed and reported in `status`; a declared health is a field that always agrees with itself. Weights are `RG-5`'s input to an assignment, and SM4 records the assignment rather than the arithmetic |
| A per-node key set, or a per-node view of membership | CM2, §2 D2. A node holding a different key set is the node K1 exists to prevent, and it reports itself healthy |
| A key's secret value, inline | §8 V9, KY3 — recognised and refused, so the error says *reference it*, not *no such field* |
| A derived key, a passphrase, a KDF over cluster facts | UQ2. Two nodes that can compute a key are two nodes an attacker can imitate once the inputs leak |
| A key-exchange, key-request or key-push protocol between nodes | [affinity-token](affinity-token.md) §6, KY9, UQ3. There is no handshake to attack because there is no handshake |
| A relative validity window (`verifyFor: 7d`) | KY4. The loader has no clock, so one document would mean as many windows as there are reload moments |
| A second `mint: true`, or none at all | KY5 and [affinity-token](affinity-token.md) §6. Two minting keys is a fleet minting records half of it may reject; none is a cluster that cannot Record-Route |
| A flag that lets a reload close a verify window early | RB9, §9.3 RL11. The emergency path is a restart on purpose: a rule that can be switched off from the document is one that gets switched off in an incident |
| A partial shard space, or an owner that is not a declared member | SM1, SM2. Both are a slice of the key space with nowhere to write |
| A shard owner that runs no registrar | SM3 |

## 11. Test vectors

**This spec registers no new vector prefix, and that is a deliberate sequencing decision rather
than an omission.** Its rules are configuration-validation rules, so they execute through
[cluster-config](cluster-config.md) §12's `load`/`reload` rows — bytes and a `NodeIdentity` in, a
`Config` or an ordered `Vec<ConfigError>` out — and that prefix is already registered in
`scripts/check-vectors.py` and already carries the families these rules fall into. The rows below
are the ones that execute a rule of this spec today; every one of them is deferred with a reason in
[vector-scope.toml](../reference/vector-scope.toml), because the reload half of the loader has no
test yet.

| Rule | Executed by, in [cluster-config](cluster-config.md) §12 |
|---|---|
| MB2 | `CC-R-7` (a zone mismatch names both), `CC-R-8` (no entry still starts) |
| MB3 | `CC-I-1` (two members with one id, naming both holders) |
| RD4, RB11 | `CC-I-4` (an id in circulation is not re-pointed) |
| KY1 (`id`, adopted from [affinity-token](affinity-token.md) §6) | `CC-K-5` (two entries sharing an id with overlapping windows) |
| KY3 | `CC-V-10` (an inline `secret` is refused citing V9, not V2) |
| KY5 (the "exactly one" half; the default and the none-case have no row yet) | `CC-K-6` (two keys with `mint: true`) |
| RD1, RD6 | `CC-K-1`, `CC-K-2` (accepted, no restart, no call disturbed), `CC-K-3`, `CC-K-4` (the two transition refusals) |
| RD1, RD7 | `CC-S-1` … `CC-S-9`, of which `CC-S-8` is the "no call is disturbed" half |
| SM2 | `CC-V-4` (an owner absent from `membership`) |
| §5's `drainTimeout` (DS4, adopted) | `CC-S-7` (below the permitted range) |

The fields this spec introduces — `rpc` (MB5, MB6), `incarnationSource` and `incarnationRef` (MB8),
KY5's refusal of a document with no mint key, SM1's totality and SM3's registrar-owner rule — have
no rows yet. They get them in
the same commit as the loader code that implements them, under the `CC` prefix and its existing
families, so that a row and the test that executes it arrive together. Writing the rows first would
put them in the deferral ledger with a story attached, which is the shape `CF-8` and `EX-12` both
paid for: a row nothing executes is prose with a table around it.

## 12. Consequences for documents this spec does not own

Named so they are tracked rather than discovered. None is performed by this story except the first.

| Where | What must change, and why |
|---|---|
| [cluster-config](cluster-config.md) §7 and §10 | The three registry rows, the §10 lead-in and one sentence of §1's out-of-scope paragraph name **AF-6**, a story, as the owner of these sections' content. They now name this document. **Done by this story** — those pointers and nothing else; no rule of §1–§9 moved, per §10 A6 |
| [cluster-config](cluster-config.md) §8, beside V9 | KY3's redaction requirement is stated here because this spec owns this refusal, but it is not a property of keys — it is a property of `ConfigError`, which §8 declares, and it binds `dsnRef`, `keyRef` and `tenant[].auth.secret` identically. The general form belongs beside V9 as one sentence: **a `ConfigError` on a V9 path carries a description of what was written, never the value.** The loader already does exactly this, twice and unwritten — `crates/sipx-clstr-node/src/config/mod.rs:1278` reports `"an inline DSN"` and `:1431` reports `"an inline nonce secret"` — so the rule would be writing down a convention DP-8 already follows rather than asking for a change |
| [affinity-token](affinity-token.md) §1 "Out of scope" | It defers "the key configuration schema and reload mechanics" to `DP-1`. The reload mechanics are [cluster-config](cluster-config.md) §9.3 and the schema is §4 here; the pointer moves, no rule does |
| [sipx-cluster-crd](sipx-cluster-crd.md) §5 | Its three `spec.membership` / `spec.keys` / `spec.shardMap` rows name **AF-6** for the same reason and can name this document. `KeysDistributed` (§7 S6) is unchanged and is §7.1 RB3's observable, which is the one place the two documents have to agree |
| `crates/sipx-clstr-node/src/config/mod.rs` | `MemberSpec` and `read_membership`'s closed world are `{node, name, zone, roles}`, and `keys` and `shardMap` sit in `DEFERRED_SECTIONS`. A document written to this spec is refused by today's loader — by §8 V2, naming `rpc` — until the implementing story adds the fields. That is the closed world working, not a defect, but it means the schema and the loader are one story apart |
| `deploy/helm/values.yaml` | Its comment block already explains that these are AF-6's sections and holds no keys for them. A chart that cannot express `keys` cannot perform a rotation, so `KO-2`'s successor has §4 and §7.1 to render |
