# Spec: Cluster configuration schema

**Status:** normative · **Crate:** _future — lands with the loader (`DP-1`'s implementing story)_ ·
**Stories:** DP-1 · **Design:** [deployment](../designs/deployment.md)

One document describes a cluster. Every node reads that same document, projects it through its own
identity, and either starts or refuses — and the operator's `SipxCluster` resource is the same
document under another skin. This spec fixes the document: how it is versioned, how a node projects
it, which sections exist and who owns each, what a refusal says, and which changes may be applied to
a running node instead of restarting it.

It deliberately owns **very little policy**. Almost every section's content is another spec's; what
is here is the frame that stops there being a second one.

## 1. Normative references

- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- RFC 3261 §16.6 step 3 (`Max-Forwards` inserted at 70 when absent — the value behind V6 below),
  §17.1.1.2 / §17.1.2.2 (Timer B, Timer F) and §16.6 step 11 (Timer C > 3 minutes) — the timer
  names §7 adopts. §19.1.1 (the `host [":" port]` shape a listener advertises).
- RFC 3986 §3.2.2 / RFC 5952 — the address forms an advertised host may take, already parsed and
  refused by `DP-5`'s rules ([deployment](../designs/deployment.md)) and inherited here by §5 P7.
- YAML 1.2 core schema and RFC 8259 (JSON) — the two accepted encodings of §2's data model.
- This repo's specs consumed, each of which owns the content of the section §7 assigns it:
  [affinity-token](affinity-token.md) §3 (the logical-id widths of §6), §6 (key entry attributes
  and the rotation rules of §9.3), §12.2 CT1 (node-id uniqueness), §13.3 BI4 (the tenant
  name↔id mapping);
  [location-service](location-service.md) §4 (tenant id and binding fields), §5.2 E3/E5/E6 and
  §5.5 (per-tenant expiry and quota, which is why §7 puts them under `tenant[]` and not under
  `registrar`), §5.1 S1 (served domains), §8 (the shard key the map of §9.4 assigns);
  [registrar-auth](registrar-auth.md) §2 (the policy this platform owns: which tenants
  authenticate, in which realm, from which credential source, under which algorithm), §4;
  [media-relay](media-relay.md) §8 K6 (the seven NG timers and their bound), §13.1/§13.2
  (`TrunkMediaPolicy`, MP7, MP12), §13.5 (G-M1…G-M6 — startup refusals this loader performs),
  §6.2 C2 (the cluster-unique edge id behind an NG cookie — the same id as §6's node id here,
  not a second one);
  [number-normalisation](number-normalisation.md) §3 (the profile data model), §8 N23 (the two
  binding points), §10 (which says in as many words that DP-1 owns the file syntax), N22 (its
  configuration errors are load-time, like every rule here);
  [hook-framework](hook-framework.md) §6 (the module manifest), §7 G1–G6 and §8 (a profile is a
  named module set plus role flags — the `profile` field of §7);
  [e2e-probe](e2e-probe.md) §5 T1–T6 (probe targets), §9/§11 (the role set, and the one role
  combination R6 refuses);
  [proxy-behavior](proxy-behavior.md) §5 (a node recognizes its own `Route`, which is why P5 makes
  the identity set the union over projected listeners).

**Out of scope.** The *content* of every section §7 assigns to another owner — this spec fixes
where it lives, who validates it and when it may be reloaded, never what its fields mean. Also out
of scope: the transport by which a new document reaches a node (a signal, a file watch, an API —
driver, not decision); the operator's staging of a change across nodes and zones
([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md));
the `SipxCluster` custom resource's Kubernetes surface — `metadata`, `status`, admission webhooks,
printer columns
([KO-1](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md));
secret *storage* (§8 V9 fixes only that the document never carries a secret's value); and the
membership and key sections' fields, which are
[cluster-membership](cluster-membership.md)'s
([AF-6](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/AF-6-design-config-first-membership-and-key-distribution.md))
and are integrated by §10 rather than restated.

**Upstream considerations** (AGENTS.md rule 6): **no — none of this is the kernel's.** A
configuration schema names this platform's own concepts: roles, zones, shards, tenants, trunks,
media pools, deployment profiles. The kernel has no opinion about any of them, and the one place
this document touches kernel surface — a listener's bind/advertise split — maps onto
`sipx_transport::Config`'s existing `bind` / `sent_by` fields rather than re-deriving them
(`DP-5`, [deployment](../designs/deployment.md)). Nothing new joins
[the ledger](../upstream.md).

## 2. What this is: one document, two readers

```rust
/// The whole of loading. No socket, no clock, no file system, no DNS.
fn load(
    document: &[u8],
    identity: &NodeIdentity,
    env: &BTreeMap<String, String>,
) -> Result<Config, Vec<ConfigError>>;

/// Reload: the same validation, plus everything that can only be judged against what is running.
fn reload(
    active: &Config,
    document: &[u8],
    identity: &NodeIdentity,
    env: &BTreeMap<String, String>,
) -> Result<ReloadPlan, Vec<ConfigError>>;
```

| # | Rule |
|---|---|
| D1 | **Loading is a pure function of its three inputs** (AGENTS.md #2). Interpolation values arrive in `env` as data rather than being read from the process environment inside the loader; the current time is not an input, so no validation rule may depend on one; nothing resolves a name, opens a socket, or reads a second file. The whole of §8 and §9 is therefore a fixture in the deterministic harness, and every row of §12 is bytes-in/verdict-out. |
| D2 | **The document is cluster-scoped; a node reads a projection of it** (§5). The same bytes are correct on every node, which is what makes "the config file" and "the desired state" one document, and what makes a diff between two versions reviewable in one place. |
| D3 | **Three encodings, one data model:** YAML 1.2 (core schema), JSON, and TOML. The normative artefact is the typed tree, not any spelling; a loader MUST produce identical `Config` values for any two encodings of the same tree, and a conformance test MUST assert that for each pair it accepts. JSON is a subset of YAML and needs no separate reader; TOML is a distinct grammar and is converted into the same tree before any rule in §8 sees it, so validation has exactly one path. The encoding is **detected from the bytes**, never from the file name — a document renamed is not a document changed. TOML has no null and no datetime-typed field in this schema; a TOML datetime is carried as its string spelling. |
| D4 | **Keys are `lowerCamelCase`; enumerated values are `kebab-case`.** One convention, mechanically checkable, chosen because the same tree is the `SipxCluster` custom resource and Kubernetes is the platform's named deployment target. §7's registry is spelled this way and §13 lists the renames this forces elsewhere. |
| D5 | **No inheritance, no includes, no templating, no anchors that cross sections, no expression language.** The only substitution is §8 V4's `${NAME}`. A document that computes is a document nobody can diff, and diffing two versions is what §9 is built on. |

## 3. Versioning — two numbers that mean different things

```yaml
apiVersion: sipx.dev/v1alpha1     # which schema this document is written against
version: 42                       # which configuration this document *is*
cluster: { … }                    # §7
```

| # | Rule |
|---|---|
| D6 | **`apiVersion` is the schema version** and is required. A node implements a finite set of them and MUST refuse a document naming any other, naming the versions it does implement. It MUST NOT parse a document it does not fully implement — not on a best-effort basis, not by ignoring what it does not recognise. A half-understood security posture is worse than a node that will not start. |
| D7 | **Compatibility within one `apiVersion` is additive only.** A new field with a declared default may be added. Removing a field, renaming one, narrowing its type, changing its default, or changing what an existing value means requires a **new** `apiVersion`. A node MAY implement two `apiVersion`s at once so that a cluster can roll; conversion between them is the operator's ([KO-1](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md)). |
| D8 | **`v1alpha1` carries no compatibility promise at all.** It may break in any release. Saying so here is the point: an alpha version that is quietly treated as stable is how a schema acquires fields nobody can remove. |
| D9 | **`version` is the configuration version:** a `u32`, required, and strictly increasing over the sequence of documents a cluster is given. It is the value `affinity-token` §3 stamps into a token's `policy version` field at mint, which is the whole reason it is a `u32` and not a hash or a timestamp — the width is fixed by that field, not chosen here. In Kubernetes the operator sets it from the resource's `metadata.generation`. |
| D10 | **A reload whose `version` is less than or equal to the active one is rejected** and nothing changes. Rolling back is publishing a *new*, higher version whose content is the old one — so that "which configuration is this node running" has one answer and `affinity-token` §6's "at any config version" and §12.2 CT1's "at every configuration version" are statements about something that exists. |
| D11 | **The active version changes only when every class of §9 has applied.** A staged change — §9.4's handoff is the only one — leaves the previous version active until it settles, so a token minted mid-handoff carries the version whose shard map actually decided it. This is the condition the operator reports as `ShardMapConverged`. |

## 4. Roles

| # | Rule |
|---|---|
| R1 | The role set is exactly `edge`, `registrar`, `inbound-proxy`, `outbound-proxy`, `e2e-tester`, `echo`. A value outside it is a load error naming the value and the closed set. `echo` is in the set because [e2e-probe](e2e-probe.md) §9 puts it there; a schema whose role set omitted it would leave the echo endpoint no way to be configured on the one binary. |
| R2 | **`roles` is a set, not a value.** One binary, any combination, subject to R6 — that is the whole of "roles by config", and a schema that could only express one role per process would be a role-specific binary with extra steps. |
| R3 | **A role selects which decision paths are wired; it never selects what a request decides.** A request's direction, tenant, scope and trunk come from the ingress binding it arrived on and from its own content, never from the node's role set. This is why R2 is safe: `inbound-proxy` and `outbound-proxy` on one node cannot be ambiguous, because neither of them is consulted when a request is classified. A section that had to ask "which role am I?" at request time would be a schema defect, and §7's registry is the check — a role column is about *wiring*, not about dispatch. |
| R4 | The empty role set is a load error. A node that runs nothing is a node that should not have been started. |
| R5 | A section present in the document but consumed by no role this node runs is **projected away, not an error** (§5). The document is cluster-scoped, so carrying other roles' configuration is its normal state. Typos are caught by V2's closed world, not by this rule. |
| R6 | **`echo` is refused in combination with any of `edge`, `registrar`, `inbound-proxy`, `outbound-proxy`** — [e2e-probe](e2e-probe.md) §11, which fixes the constraint and leaves the schema to enforce it: "a process running `echo` runs no proxy role, and a configuration asking for both is refused at load". `e2e-tester` is refused in the same combinations for the same reason in reverse: a probe that enters through the node it is probing measures a path no caller takes ([architecture](../architecture.md) draws it outside the border deliberately). `e2e-tester` with `echo` is permitted — caller and callee of the same synthetic call, both off the call path. |
| R7 | Every section names the roles that consume it (§7). A role's wiring is the union of the sections its column marks, and there is no other way for a role to acquire behaviour — no role-conditional field, no `if role == …` anywhere in the document. |

## 5. Node identity and projection

A node is given its identity **outside** the document, because the document is the same on every
node:

```rust
struct NodeIdentity {
    node:  NodeId,           // §6 — the u16 logical id, and the name it is spelled with
    zone:  ZoneName,
    roles: BTreeSet<Role>,   // §4
}
```

| # | Rule |
|---|---|
| P1 | `NodeIdentity` is an input to `load`, never a section of the document. In Kubernetes it comes from the downward API and the workload's role; on a plain host it comes from flags. Deriving it from the document would mean a per-node document, and then §3's version would not be a fact about the cluster. |
| P2 | **Projection is a pure, total function** `project(Config, NodeIdentity) -> NodeConfig`, and it never fails: everything that could fail has already failed in §8. It selects the listeners whose `roles` intersect this node's (§7), drops sections no configured role consumes (R5), and resolves the node's own membership entry. |
| P3 | **The document's membership section is cross-checked, not obeyed.** If it carries an entry for this node's id, that entry's zone and roles MUST equal the identity the node was started with; a mismatch is a load error naming both. If it carries no entry for this node, that is **not** an error — a node whose pod the operator has not yet published would otherwise be unable to start, and the failure would arrive as a crash loop rather than as a mismatch. What is an error is a duplicate id (§6 I2). |
| P4 | A projected node MUST hold at least one listener, or, if its only roles are `e2e-tester`/`echo`, at least the listener that role needs. `Listeners::new`'s `NoListener` is the same refusal seen from below. |
| P5 | **The node's SIP identity set is the union of the advertised addresses of its projected listeners** — every one of them, not just the receiving one — because any edge must recognize any of its own `Route` values ([proxy-behavior](proxy-behavior.md) §5). |
| P6 | Two projected listeners MUST NOT share a `(transport, bind)` pair; two sharing a `bind` across the cleartext pair MUST agree on what they advertise (`ListenerError::ClearTextDisagreement`). Beyond that the schema permits several listeners of one transport on one node — which is what lets `edge` and `registrar` co-reside on separate ports — and the implementing story keys message arrival on the receiving local address rather than on the transport alone, which today's `Listeners::receiving` does not yet do. Until it does, a projection with two listeners of one transport is refused at load, naming both, exactly as [media-relay](media-relay.md) §13.2 MP12 refuses a policy the relay cannot yet honour. Refusing is not a limitation on role combinations: co-resident roles share a listener and are dispatched by method. |
| P7 | Everything `DP-5` already fixes about a single listener is inherited verbatim and is not restated: an advertised address is refused when it is empty, unspecified (`0.0.0.0`, `::`) or names port `0`; an omitted advertised port means the port bound, never the scheme's default. A second spelling of those rules is exactly the defect this spec exists to prevent. |

## 6. Logical ids

[affinity-token](affinity-token.md) §3 carries `tenant` (u32), `home shard` (u16), `edge affinity`
(u16) and `media node` (u16) as **logical ids assigned by configuration** — "a logical id is
meaningless without the cluster's own configuration". This section is that configuration, and it is
the reason ids are not an implementation detail.

| # | Rule |
|---|---|
| I1 | Every tenant, shard, node and media node carries both a **name** (UTF-8, human, the spelling used in the document and in the location service's `tenant` column — [location-service](location-service.md) §4) and an **id** of the width `affinity-token` §3 fixes. The name↔id map is this document; `affinity-token` §13.3 BI4 names it as configuration-owned, and this is where it lives. |
| I2 | **Ids are unique within their kind at every configuration version**, and `0` is reserved (`affinity-token` §3 spells `0` as "none" for shard, edge and media node, and as "none/system" for tenant). A duplicate is a load error naming both holders. For nodes this is [affinity-token](affinity-token.md) §12.2 **CT1**, where it is a correctness input and not a convention: two nodes sharing an id give two different connections one flow identity. The same id is the edge id [media-relay](media-relay.md) §6.2 C2 requires to be cluster-unique for NG cookies, and it is one id, not two — MR-X-8 and CT1 are the same check. |
| I3 | An id, once assigned to a name, MUST NOT be reassigned to a different name in a later version while any record minted under the old assignment can still be presented — the bound of `affinity-token` §6 K4, `max(L, E_max) + S`. Reusing a retired id early is indistinguishable, on the wire, from the record it collides with. The loader enforces the version-to-version half of this (§9.3); the calendar half is the rotation runbook's. |
| I4 | Names are matched byte-for-byte. No case folding, no normalisation, no trimming — [location-service](location-service.md) §4's tenant id is opaque bytes, and a schema that folded case here would make two tenants one. |

## 7. The section registry

The table **is** the integration. Each row fixes where a section lives, which roles consume it, who
owns its content, and its reload class (§9). No row restates the owner's fields.

| Section | Roles that consume it | Content owned by | Reload class |
|---|---|---|---|
| `name`, `environment`, `zones` | all | this spec | rollout |
| `profile` | all | [hook-framework](hook-framework.md) §8, EX-5 | rollout |
| `listener[]` (`roles`, `transport`, `bind`, `advertise`, `connectionLifetime`, `maxConnections`, `tls`) | as declared per listener | `DP-5` / [deployment](../designs/deployment.md), §5 here | rollout |
| `management` (`bind`, `tls`) | all | this spec | rollout |
| `membership` | all | [cluster-membership](cluster-membership.md) §3 (**AF-6**, §10) | reloadable |
| `keys` | all | [cluster-membership](cluster-membership.md) §4 (**AF-6**, §10), attributes fixed by [affinity-token](affinity-token.md) §6 | reloadable — §9.3 |
| `shardMap` | `registrar`, `edge` | [cluster-membership](cluster-membership.md) §5 (**AF-6**, §10); assignment over [location-service](location-service.md) §8's key | reloadable — §9.4 |
| `locationStore` (`backend`, `dsnRef`, `ha`) | `registrar` | [location-service](location-service.md) §6.2/§6.3 | rollout |
| `registrar` (`usePath`, `methodFiltering`) | `registrar` | [location-service](location-service.md), [registrar-auth](registrar-auth.md) | rollout |
| `tenant[]` (`name`, `id`, `domains`, `auth`, `expiry`, `maxBindingsPerAor`) | `registrar`, `edge`, proxies | [registrar-auth](registrar-auth.md) §2/§4, [location-service](location-service.md) §5.2/§5.5/§5.1 S1 | reloadable |
| `normalisation` (named profiles) | proxies, `edge` | [number-normalisation](number-normalisation.md) §3 | reloadable |
| `trunk[]` (incl. `quirks`, `quirkConfig`, `quirkOverrides`, `media`, `normalisation`) | proxies | RT-2, [media-relay](media-relay.md) §13, [extension-framework](../designs/extension-framework.md) G13 | reloadable — §9.2 |
| `domain[]` (incl. `quirks`, `quirkConfig`, `quirkOverrides`) | proxies, `edge` | [extension-framework](../designs/extension-framework.md) G13 | reloadable |
| `destinationSet[]`, `routeRule[]`, `ingress[]` | proxies | RT-8/RT-9 | reloadable |
| `rateLimit[]` | `edge`, proxies | RT-3 | reloadable |
| `timers` (`t1`, `timerB`, `timerC`, `timerF`, `maxCallDuration`) | proxies, `edge` | [proxy-behavior](proxy-behavior.md), RFC 3261 §17 | reloadable |
| `security` (`unknownSource`, `maxForwards`, `sanityCheck`, `userAgentDenyList`, `internalZone`) — four of the five are **refused until a consumer applies them**, below | `edge` | this spec (§8 V6) + RT-3 | reloadable |
| `admission` (`maxInFlightTransactions`) | `edge`, proxies | [deployment](../designs/deployment.md), `DP-11` | reloadable |
| `nat` | `edge` | **unowned — see [deployment](../designs/deployment.md)** | reloadable |
| `mediaPool[]` (`mode`, `nodes`, `portRange`, `interfaces`, `ngTimers`) | proxies | KO-7; timers by [media-relay](media-relay.md) §8 K6 | rollout (`mode`) / reloadable (`nodes`) |
| `observability` (`metrics`, `logFormat`, `hep`, `cdr`) | all | DP-3, DP-6, DP-7 | reloadable |
| `probe` (`targets`, `schedule`, `tenant`) | `e2e-tester` | [e2e-probe](e2e-probe.md) §5 | reloadable |
| `echo` | `echo` | [e2e-probe](e2e-probe.md) §9 | reloadable |

**`security`'s four ingress controls, and why a document declaring one does not start.**
`unknownSource`, `sanityCheck`, `userAgentDenyList` and `internalZone` all answer "who may reach a
public SIP decision path", and no consumer in this build applies any of them. A document declaring one
is therefore **refused at load, naming every declared path** — §8 V10's rule reached from the direction
§5 P6 and [media-relay](media-relay.md) §13.2 MP12 already reach it: refuse a policy the
implementation cannot honour, rather than honour a different one. Starting without them is not a
useful degraded mode, because the posture they were declared to *narrow* is the one a node without them
serves. An absent or empty `security` block stays valid and carries only the fixed Max-Forwards of
§8 V6, which is not a knob and is refused as a key. The refusal is **per control, not per section**: a
story that specifies a consumer for one of the four removes that control from this paragraph and leaves
the rest refusing. Vectors: §12 CC-V-13 … CC-V-15.

| # | Rule |
|---|---|
| S1 | **A section has exactly one owner.** Where two owners would both like a field, it belongs to the one whose spec can validate it; where neither can, it is unowned and says so, which is a smaller lie than an owner who is not looking. |
| S2 | **Per-tenant policy lives under `tenant[]`, not under `registrar`.** Expiry defaults, minima, maxima and binding quotas are per-tenant by [location-service](location-service.md) §5.2/§5.5; realm, algorithm, credential source and whether authentication is required at all are per-tenant by [registrar-auth](registrar-auth.md) §2. A node-wide spelling of any of them would be a second, coarser policy that the registrar's own spec cannot see. |
| S3 | **Media policy is per trunk, not global.** [media-relay](media-relay.md) §13.1 makes `TrunkMediaPolicy` a field of the trunk and MP6 forbids selection derived from a domain, a pattern or a hostname; a cluster-wide codec or SRTP setting is unrepresentable here on purpose. What *is* cluster-wide is the pool: nodes, port ranges, interface names and the seven NG timers of §8 K6. |
| S4 | **A quirk binding lives on the object it binds to** — `trunk[].quirks` and `domain[].quirks`, with `quirkConfig` and `quirkOverrides` alongside ([extension-framework](../designs/extension-framework.md) G13: an override is declared at a binding, never in a profile). There is no top-level list of bound profiles, because a profile that names where it applies is the shape EX-10 removed. |
| S5 | **`ConfigKey` is this schema's.** The typed keys a quirk profile may read through `ValueLeaf::TrunkConfig` / `DomainConfig` are exactly the keys `trunk[]` and `domain[]` declare; a profile naming any other fails startup ([extension-framework](../designs/extension-framework.md) B2, and its closed-world check). |
| S6 | **The nonce secret is cluster configuration, not a per-node accident.** [registrar-auth](registrar-auth.md) §6 makes two properties follow from where it comes from: edges sharing a secret recognise each other's nonces, so a challenge issued at one edge is answerable at another and the round trip does not depend on transaction affinity; and an edge that generates its own at startup invalidates every outstanding nonce when it restarts. It therefore lives under `tenant[].auth`, by reference (V9), and `AuthConfig`'s provisional per-node literal in `sipx-clstr-node` is the shape this replaces. |

## 8. Validation and what a refusal says

```rust
struct ConfigError {
    path:     Path,          // `cluster.trunk[2].media.srtp` — the document's own spelling
    rule:     RuleId,        // `CC-V2`, `MR-G-M1`, `NN-N22`, … — this spec's or the owner's
    found:    Option<String>,
    expected: String,
}
```

| # | Rule |
|---|---|
| V1 | **Validation reports every error, not the first.** A document with five mistakes costs five seconds, not five restarts. Errors are ordered by `path`, so two runs over one document produce byte-identical output. |
| V2 | **Closed world: an unrecognised key is an error**, naming the path and the keys that are recognised at it. Not a warning, not ignored. `maxContact` for `maxContacts` silently dropped is a quota nobody is enforcing and nothing anywhere says so. |
| V3 | **Every default is declared in the owning spec and is fail-closed**; there is no implicit default and nothing is defaulted twice. Where a default already exists — expiry 3600/86400/60 s and `maxBindingsPerAor` 10 ([location-service](location-service.md) §5.2, §5.5), digest SHA-256 ([registrar-auth](registrar-auth.md) §4), `AsReceived`/`None`/`Disabled` and the seven NG timers ([media-relay](media-relay.md) §13.1, §8) — this schema adopts it unchanged and MUST NOT restate a different one. |
| V4 | **The only substitution is `${NAME}`,** where `NAME` matches `[A-Z_][A-Z0-9_]*`, resolved from `load`'s `env` argument. No nesting, no defaulting, no arithmetic, no command substitution. An undefined name is an error naming the variable and the path — never the empty string, which would turn `advertise: "${NODE_IP}:5060"` into an unparsable address and report the wrong problem. Substitution happens before typing, so a substituted value is validated exactly like a written one. |
| V5 | **Validation is over the whole document, not field by field.** Cross-section rules are ordinary rules: a trunk declaring `SrtpPolicy::Sdes` whose signalling transport is not TLS ([media-relay](media-relay.md) §13.5 G-M1), a normalisation binding naming an undeclared profile ([number-normalisation](number-normalisation.md) N22), a `quirkOverrides` entry whose target is not contested at that binding ([extension-framework](../designs/extension-framework.md) G13), a `shardMap` owner that is not a `membership` entry, a `probe.tenant` that is not a declared tenant. Each fails with the owner's rule id, not with one of this spec's. |
| V6 | **Where an RFC fixes a value, the schema does not offer a knob.** `security.maxForwards` is the value inserted when a request carries none, and its default is **70** (RFC 3261 §16.6 step 3, [proxy-behavior](proxy-behavior.md) §5); it is not a hop budget to be tuned downward, and a schema that presented it as one would invite a deployment to break transfers it never connects to the setting. The same posture, stated once: **no configurable status code where a spec has already fixed one** — [number-normalisation](number-normalisation.md) N20's `404`, [location-service](location-service.md) §5.5's `403` — and where a spec deliberately *does* admit one it says so and names the owner, as [media-relay](media-relay.md) §9 X3 does for `ME-4`'s rejection mapping. A knob this schema invents beside a fixed status is a deployment quietly disagreeing with a normative rule; a knob a spec asks for is that spec's field, carried here. |
| V7 | **Timers carry RFC 3261 §17 names**: `t1` (default 500 ms), `timerB` (default `64·t1`), `timerF` (default `64·t1`), `timerC` (default **240 s**, MUST be **> 3 minutes** — RFC 3261 §16.6 step 11). **The bound is strict, and the default does not sit on it.** §16.6 step 11 reads "the timer MUST be larger than 3 minutes": a MUST over a strict inequality, with no SHOULD, no tolerance and no rounding language anywhere near it, so reading it as `≥` would admit exactly the value the RFC forbids and would need an argument the RFC does not supply. The rule therefore stays `>`, and the **default** is what moved (`DP-12`). It had been 180 s — the floor exactly — which made it unsatisfiable by omission: a document carrying a `timers` section without naming `timerC` was refused for the value this schema had itself supplied, and no operator can fix a default by writing nothing. **240 s** is the smallest whole-minute value above the floor, stated in the unit the RFC states the floor in so that the bound and the default cannot be misread for each other, and it is ~7.5× Timer B (`64·T1` = 32 s, §17.1.1.2), leaving a branch room for a downstream transaction to time out and a serial attempt to follow before Timer C fires. It is deliberately not raised further: Timer C is the only bound on a branch that has gone quiet since its last provisional (§16.7 bullet 2 restarts it on each one), so every additional minute is a minute a wedged branch holds a proxied transaction and, since V11, an admission slot. A deployment that wants longer writes it. `maxCallDuration` is a session cap and has nothing to do with Timer C, which cannot outlive a final response; conflating them produces a Timer C set to hours in the belief that it protects long calls, and the wrong knob is then the one that is tuned. |
| V8 | **Declared ceilings, checked at load.** `quirks` per binding ≤ 8; quirk bindings per node ≤ 4096; `keys` entries ≤ 16; `zones` ≤ 64; listeners per node ≤ 32; `rateLimit[]` ≤ 256; `admission.maxInFlightTransactions` ≤ 65536 and ≥ 1. These bound startup validation, which is superlinear in bindings × profiles ([extension-framework](../designs/extension-framework.md) G10's disjointness check, whose bound that design explicitly leaves here). Raising one is a change to this spec, never a configuration flag. |
| V11 | **`admission.maxInFlightTransactions` bounds concurrency, not the queue.** The kernel already bounds its *incoming queue* and answers `503` with `Retry-After` when it is full; this bounds how many gated transactions a node will hold **at once**, which the queue does not, because a node drains that queue as fast as it can and a proxied transaction lives until Timer B. A request over the bound is refused with the same `503` and `Retry-After` the kernel uses, so a client sees one behaviour whichever layer shed it. Default **1024**, matching the kernel's queue capacity so the two limits are one number rather than two that can disagree; `0` is refused, because a node that admits nothing is a node that should not have started. **`REGISTER` and `ACK` are outside the bound**: a registration storm *is* the overload and a shed refresh makes a phone unreachable, turning a spike into an outage; and an ACK for a 2xx has no response in SIP at all (RFC 3261 §17.1.1.3), so there is no `503` to send it. Every other gated method — `INVITE`, `BYE`, `CANCEL`, `OPTIONS` — is subject to it: exempting the requests that *end* work is tempting because shedding them makes overload self-sustaining, but an unbounded method is an unbounded node, and a `503` with `Retry-After` to a `BYE` is a retry rather than a loss. |
| V9 | **No secret value appears in the document.** Key material, database credentials and TLS private keys are named by reference — `dsnRef`, `secretRef`, `keyRef` — and resolved by the driver into the value the owning spec describes (a `keys` entry's `secret` is `affinity-token` §6's exactly-32-bytes *after* resolution). The document is rendered into a ConfigMap and the references into Secrets by the operator; a schema that permitted an inline secret would make the safe shape the optional one. A reference that does not resolve is a **start-up** failure of the driver, not a load error, because resolution is IO (D1). |
| V10 | **Refusing to start is the only failure mode.** There is no partial application, no degraded mode, no "continue with the last good value for this field". [media-relay](media-relay.md) §13.5's G-M1…G-M6, [hook-framework](hook-framework.md) §7's G1–G6, [number-normalisation](number-normalisation.md) N22 and every rule here agree on this, and the agreement is the point: an operator learns one failure behaviour. |

## 9. Reload

### 9.1 Classes and atomicity

| # | Rule |
|---|---|
| RL1 | **The reload class is a property of the field, declared in §7** — not a verdict reached by diffing. The node and the operator must classify a change identically, or the operator will push a change no node applies. |
| RL2 | **A reload is atomic per document.** The whole document is validated (§8) and then either applied or not. A document that changes any `rollout`-class field is **rejected as a reload**, naming the fields, and the node keeps running the active version; applying it is a restart, staged by the operator ([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md)). |
| RL3 | `reload` additionally applies the **transition rules** of §9.2–§9.4, which are judged against the active document and cannot be judged from the new one alone. At first load there is no predecessor and every transition rule is vacuous. |
| RL4 | **No reload disturbs an established dialog.** A dialog's route set is not recomputed (RFC 3261 §12.2.1.2) and everything a mid-dialog request needs rides in the message, so the reloadable subset is precisely the state that no in-flight dialog reads. Where an in-flight *transaction* could read it, §9.2's stamping is what makes that safe. |

### 9.2 Trunks — stamp and retire

| # | Rule |
|---|---|
| RL5 | A `RoutePlan` already carries the policy version it was built under ([routing-trunks](../designs/routing-trunks.md)). On reload the new trunk table becomes the table **new plans are built from**; a plan already built completes against the trunk objects of the version stamped on it. |
| RL6 | **A trunk object removed or changed by a reload is retained while any in-flight plan references its version**, and released when the last one completes. So a trunk deleted mid-call never strands a transaction, and a trunk whose failover list changed never mixes two lists inside one plan. |
| RL7 | Per-trunk runtime state — breaker, CPS counters, registration state — is keyed by the trunk's declared identity and **survives** a reload that leaves that identity unchanged. What that identity is, is RT-2's; this spec requires only that it be declared, stable across versions, and not derived from a mutable field. A reload that resets every breaker would turn a routine config push into a synchronized retry storm. |
| RL8 | Whether a plan is *rebuilt* or *resumed* when the policy version changes mid-transaction is deliberately left to RT-2, which already holds that question open. RL5/RL6 make both answers implementable by keeping both versions resolvable. |

### 9.3 Keys — distribute, then activate

| # | Rule |
|---|---|
| RL9 | The key section's reload follows [affinity-token](affinity-token.md) §6 K1–K4 unchanged. This spec adds only what the loader must refuse, from the pair of documents: |
| RL10 | **A reload MUST NOT flip `mint` to a key that was absent from the active document.** That is K1 and K3 collapsed into one step, and it produces tokens some healthy node cannot verify yet. Error names the key id and both versions. |
| RL11 | **A reload MUST NOT remove, or close the verify window of, a key that is still within `max(L, E_max) + S` of having minted** ([affinity-token](affinity-token.md) §6 K4). Because D1 forbids reading a clock, the loader checks the *declared* windows — that the retiring key's `verifyUntil` is not brought forward, and that the incoming mint key's window covers the same bound — and leaves the wall-clock half to the runbook. |
| RL12 | **No call is disturbed by a key reload**, in either direction: a token already in circulation verifies under a key whose window RL11 keeps open, and no token is minted under a key RL10 has not already distributed. That is the entire content of "reloadable without restart" for keys — the two rules exist so that the claim is true rather than hoped for. |

### 9.4 The shard map — drain, then switch

A shard map assigns each shard of [location-service](location-service.md) §8's key space to an
owning node. Changing it moves ownership of a slice of the registration key space, and the failure
to prevent is two nodes accepting writes for one shard at once.

**State of one shard at one node, during a change from map `n` to map `n+1`:**

| State | Owner in `n` | Owner in `n+1` | Accepts new writes | Completes in-flight writes |
|---|---|---|---|---|
| `Unchanged` | this node | this node | yes | yes |
| `Draining` | this node | another | **no** | yes, bounded by `drainTimeout` |
| `Pending` | another | this node | **no** | — |
| `Owner` | — | this node | yes | yes |
| `Foreign` | another | another | no | no |

**Transitions:**

| From | Event | To |
|---|---|---|
| `Unchanged`/`Owner` | announce `n+1`, this node loses the shard | `Draining` |
| `Foreign` | announce `n+1`, this node gains the shard | `Pending` |
| `Draining` | last in-flight write completes | `Foreign`, and the node publishes `drained(shard, n+1)` |
| `Draining` | `drainTimeout` elapses | `Foreign`, forced (DS5) |
| `Pending` | `drained(shard, n+1)` observed, or `drainTimeout` elapsed | `Owner` |

| # | Rule |
|---|---|
| DS1 | **A node holds at most two maps**, `active` and `pending`. A third version announced while a handoff is in progress supersedes the pending one only if no shard has yet switched; otherwise it is queued, and only the newest queued version is kept (never interleaved — the operator's rule for the same situation, one level up). |
| DS2 | **A shard is never accepting at two nodes.** The losing node stops before the gaining node starts, and the gaining node starts only on `drained` or on the deadline. Even a forced switch cannot split: a late write from the old owner is fenced by the per-AoR revision it read ([location-service](location-service.md) §5 K3) and fails its CAS rather than being applied beside the new owner's. The deadline can therefore stall a handoff; it cannot corrupt one. |
| DS3 | **Only migrating shards drain.** A shard whose owner is unchanged between the two maps is never quiesced — which is the whole reason ownership is assigned by rendezvous hashing, and why a reshard is a small event rather than a cluster-wide pause. |
| DS4 | `shardMap.drainTimeout` is declared, default **30 s**, permitted range 5 s – 300 s. Below the location store's CAS retry budget ([location-service](location-service.md) §5.1 S10) a drain would expire while an ordinary contended write was still legitimately retrying. |
| DS5 | **A forced switch is counted, never silent.** Deadline expiry increments an invariant counter (`DP-3`); the invariant is that it reads zero, and a deployment where it moves has a node that stopped answering rather than a shard map that changed. Forcing is nonetheless correct behaviour: the common reason a drain never completes is that the old owner is gone, which is exactly when the shard must move. |
| DS6 | **No call is disturbed by a shard-map reload.** Shards own registration state, not dialogs; a mid-dialog request routes by token. What is at risk is an in-flight REGISTER write, and DS2 is what keeps it from being split. |
| DS7 | The handoff's registrar-side mechanics — which node a REGISTER arriving at a non-owner is served by, and how in-flight writes are drained — are RG-5's. What this spec fixes is the configuration half: both maps are available to the node throughout, the version is monotonic and comparable (D9), the switch point is explicit, and the active version does not advance until the last shard settles (D11). |

## 10. The seam with AF-6

`membership`, `keys` and `shardMap` are **[AF-6](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/AF-6-design-config-first-membership-and-key-distribution.md)'s
sections**, and this spec neither writes nor duplicates their fields. What it fixes is the seam, so
that AF-6 can be written against something. AF-6 has since written it:
[cluster-membership](cluster-membership.md) holds the three sections' fields, the rotation runbook
A5 leaves here, and the record of what a dynamic membership service would replace. **No rule of
§1–§9 moved when it landed**, which is what A6 asked for; three pointers did — §7's owner cells,
this paragraph, and one sentence of §1's out-of-scope list now name that document instead of the
story.

| # | Rule |
|---|---|
| A1 | **Where they live:** three top-level sections of `cluster`, listed in §7, versioned with the document (§3) and reloadable — which is the config-first, consensus-free posture [cluster-affinity](../designs/cluster-affinity.md) commits to. |
| A2 | **What `membership` must provide:** per member, a `node` id and name meeting §6 I1/I2 (CT1 uniqueness is not negotiable — it is a correctness input), the member's zone, and its role set, for P3's cross-check. Everything else — addresses, the owner-RPC endpoint, health, weights, whether a member may be listed before it exists — is AF-6's. |
| A3 | **What `keys` must provide:** entries carrying the attributes [affinity-token](affinity-token.md) §6 already fixes (`id`, `algorithm`, `secret`, `verifyFrom`, `verifyUntil`, `mint`), with `secret` supplied by reference per V9 and resolving to that spec's exactly-32-bytes. This spec restates none of them and adds only the two transition rules RL10/RL11, which are about *pairs* of versions and so could not live in a spec that sees one. |
| A4 | **What `shardMap` must provide:** a shard set with §6 ids, an owner per shard drawn from `membership`, and a `drainTimeout` (DS4). The hash function, the weights and the rebalancing policy are RG-5's; how a map is *authored* — by hand, by the operator, by a future membership service — is AF-6's. |
| A5 | **Left entirely to AF-6, named so it is not lost:** the rotation runbook's calendar half (the wall-clock arithmetic RL11 cannot do); the incarnation-from-persisted-counter option [affinity-token](affinity-token.md) §12.2 CT2 requires of a deployment whose clock may step backwards; the tenant name↔id assignment procedure (this spec fixes that the map exists and is unique, §6 I1/I2, not who assigns); and what a dynamic membership service would replace. |
| A6 | **Order of authorship:** AF-6 writes those sections' fields into this spec's §7 rows and its own document; nothing in §1–§9 changes when it does. That is what "integrated, not duplicated" means here — the frame is written so that the content has one place to land, and there is no second copy to keep in step. |

## 11. What is deliberately not expressible

| Not expressible | Why, and where it belongs |
|---|---|
| Per-node documents, node-conditional fields, `if role == …` | P1/R3. The moment two nodes read different bytes, "the configuration version" stops naming a thing, and §3's whole argument collapses. |
| Includes, inheritance, templating, an expression language | D5. A config that computes cannot be diffed, and §9 is entirely built on diffing two versions. Composition is the operator's, above this document. |
| A cluster-wide codec, SRTP or media policy | S3, [media-relay](media-relay.md) MP6. Media policy is a property of the peer, and the peer is the trunk. |
| A profile that declares where it applies | S4, [extension-framework](../designs/extension-framework.md) G13. Bindings bind; profiles do not. |
| A status code the platform may choose where a spec already fixed one | V6. A configurable `404` or `403` is a deployment quietly disagreeing with a normative rule; a mapping a spec asks for is that spec's field. |
| A regex, a selector, a matcher anywhere in this document | [number-normalisation](number-normalisation.md) §12 and the vision's "no routing DSL" non-goal. Selection is routing's, expressed as data by RT-8/RT-9. |
| A "warn and continue" validation outcome | V10. One failure behaviour, and it is refusal. |
| Raising a §8 V8 ceiling from the document | V8. A ceiling that a deployment can raise is not a bound on startup validation. |

## 12. Test vectors

Every row is normative and executes against `load` or `reload` (§2) — bytes and a `NodeIdentity`
in, a `Config` or an ordered `Vec<ConfigError>` out, with no socket and no clock. `E(path, rule)`
abbreviates "one error at `path` citing `rule`".

These rows **are** registered in the vector registry: `CF-8` registered the `CC` prefix in
`scripts/check-vectors.py` and listed the rows in `docs/reference/vector-scope.toml`, and `CF-17`
gave this spec's seven families their sections in `docs/reference/conformance.md`. What is still
deferred is coverage, row by row and with a reason each, in that same file — the gate enforces the
table rather than waiting on it. `number-normalisation` stands in the same place with its `NN` rows.

**The document, versioning and substitution (CC-D).**

| # | Given | Expect |
|---|---|---|
| CC-D-1 | `apiVersion: sipx.dev/v2` on a node implementing `v1alpha1` | `E(apiVersion, CC-D6)`, listing `sipx.dev/v1alpha1`. Nothing else is parsed or reported |
| CC-D-2 | The same tree in YAML and in JSON | Identical `Config`, field for field (D3) |
| CC-D-3 | `cluster.registrar.usePathh: true` | `E(cluster.registrar.usePathh, CC-V2)` naming the recognised keys at that path |
| CC-D-4 | `advertise: "${NODE_IP}:5060"`, `env` without `NODE_IP` | `E(cluster.listener[0].advertise, CC-V4)` naming `NODE_IP` — **not** an address error |
| CC-D-5 | `advertise: "${NODE_IP}:5060"`, `env["NODE_IP"] = "0.0.0.0"` | `E(cluster.listener[0].advertise, ListenerError::UnspecifiedHost)` — a substituted value is validated like a written one (V4, P7) |
| CC-D-6 | A document with three unrelated mistakes | Three errors, ordered by `path`, byte-identical across two runs (V1) |
| CC-D-7 | `reload` with `version` equal to the active version | Rejected, `E(version, CC-D10)`; the active configuration is untouched |
| CC-D-8 | `reload` changing only `cluster.trunk[0]`, plus `cluster.listener[1].bind` | Rejected as a reload, `E(cluster.listener[1].bind, CC-RL2)` naming it as `rollout`-class; the trunk change is **not** applied (RL2) |
| CC-D-9 | `version` absent | `E(version, CC-D9)` — there is no default configuration version |

**Roles and projection (CC-R).**

| # | Given | Expect |
|---|---|---|
| CC-R-1 | `roles: [edge, registrar, inbound-proxy, outbound-proxy]` | Loads. One binary, four roles, no ambiguity (R2, R3) |
| CC-R-2 | `roles: [echo, edge]` | `E(roles, CC-R6)` citing [e2e-probe](e2e-probe.md) §11 |
| CC-R-3 | `roles: [e2e-tester, echo]` | Loads (R6, second sentence) |
| CC-R-4 | `roles: [proxy]` | `E(roles, CC-R1)` listing the six role names — `proxy` is the chart's word, not a role |
| CC-R-5 | `roles: []` | `E(roles, CC-R4)` |
| CC-R-6 | A document carrying `probe`, projected onto a node whose roles are `[edge]` | Loads; `probe` is projected away, not reported (R5) |
| CC-R-7 | Identity `node: 7, zone: b`; membership entry `7` declares `zone: a` | `E(cluster.membership[…], CC-P3)` naming both zones |
| CC-R-8 | Identity `node: 9`; membership carries no entry for `9` | Loads (P3) — a node the operator has not yet published still starts |
| CC-R-9 | Two projected UDP listeners, `10.0.0.7:5060` and `10.0.0.7:5070` | `E(cluster.listener[1], CC-P6)` naming both, while `receiving` keys on transport alone |
| CC-R-10 | Projected listeners UDP and TLS, different advertised hosts | Loads; the identity set holds both (P5) |
| CC-R-11 | A member declaring `rpc` and `incarnationSource` ([cluster-membership](cluster-membership.md) §3) | Loads; both are fields of a member, and an omitted `incarnationSource` selects `boot-second` (MB8) |

**Logical ids (CC-I).**

| # | Given | Expect |
|---|---|---|
| CC-I-1 | Two `membership` entries with `node: 12` | `E(cluster.membership[1].node, CC-I2)` naming both holders, for [affinity-token](affinity-token.md) §12.2 CT1's reason |
| CC-I-2 | `tenant[0].id: 0` | `E(cluster.tenant[0].id, CC-I2)` — `0` is reserved |
| CC-I-3 | Two tenants named `acme` and `ACME` | Both load: names are byte-compared (I4) |
| CC-I-4 | `reload` reassigning id `12` from node `edge-a` to node `edge-b` | `E(cluster.membership[…].node, CC-I3)` — an id in circulation is not re-pointed |

**Validation across sections (CC-V).**

| # | Given | Expect |
|---|---|---|
| CC-V-1 | `trunk[0].media.srtp: sdes`, and that trunk's signalling listener is UDP | `E(cluster.trunk[0].media.srtp, MR-G-M1)` naming the trunk and the transport |
| CC-V-2 | `trunk[0].media.codecs: { restrict: [PCMA] }` while MP12 holds | `E(cluster.trunk[0].media.codecs, MR-G-M6)` — only the identity policy is admissible until `CF-3` is green |
| CC-V-3 | `trunk[0].normalisation: carrier-e164` with no such profile declared | `E(cluster.trunk[0].normalisation, NN-N22)` naming the binding and the profile |
| CC-V-4 | `shardMap` naming an owner absent from `membership` | `E(cluster.shardMap.shards[…].owner, CC-V5)` |
| CC-V-5 | `probe.tenant: e2e-test` with no such tenant | `E(cluster.probe.tenant, CC-V5)` |
| CC-V-6 | `quirkOverrides` entry whose `target` is contested by only one bound profile | `E(cluster.trunk[0].quirkOverrides[0], EX-G13)` naming binding, target and winner |
| CC-V-7 | A `trunk[]` entry carrying nine `quirks` | `E(cluster.trunk[0].quirks, CC-V8)` — the declared ceiling is 8 |
| CC-V-8 | `security.maxForwards` absent | Loads with 70 (V6) |
| CC-V-9 | `timers.timerC: 120s` | `E(cluster.timers.timerC, CC-V7)` — RFC 3261 §16.6 step 11 requires more than 3 minutes |
| CC-V-10 | `keys[0].secret: "<32 bytes inline>"` | `E(cluster.keys[0].secret, CC-V9)` — key material is named by reference, never written |
| CC-V-11 | `tenant[0].expiry` omitted entirely | Loads with 3600 / 60 / 86400 s ([location-service](location-service.md) §5.2, V3) |
| CC-V-12 | A `timers` section naming `t1` and omitting `timerC` | Loads with 240 s (V7) — the default satisfies the rule declared beside it, so omission is a legal way to accept it |
| CC-V-13 | `security.unknownSource: drop` | `E(cluster.security.unknownSource, CC-V10)` — no consumer in this build applies it, and the refusal describes what was declared rather than echoing it (V9) |
| CC-V-14 | A `security` block declaring `unknownSource`, `sanityCheck`, `userAgentDenyList` and `internalZone` | One error per declared control, each naming its own path, ordered by path (V1); no `Config` is produced (§7) |
| CC-V-15 | The same four controls carrying wrong-shaped values — a sequence where a scalar was declared, a scalar where a mapping was | Refused all the same: refusing an unappliable control before typing it is admissible, accepting a value for it is not |
| CC-V-16 | A member whose `roles` include `edge`, declaring no `rpc` | `E(cluster.membership[…].rpc, CC-MB5)` — a member on the call path owns flows, and a peer with no endpoint to dial cannot deliver toward a client it owns |
| CC-V-17 | A member whose only role is `echo`, declaring an `rpc` | `E(cluster.membership[…].rpc, CC-MB5)` — MB5 is fail-closed in both directions, and an endpoint on a node that owns nothing is a target nobody should reach |
| CC-V-18 | Two members advertising one `rpc` endpoint | `E(cluster.membership[…].rpc, CC-MB6)` naming both holders ([affinity-token](affinity-token.md) §13.1 D5 dials what a reference names and re-checks nothing) |
| CC-V-19 | `incarnationSource: persisted-counter` with no `incarnationRef` | `E(cluster.membership[…].incarnationRef, CC-MB8)` — a counter with nowhere to persist is a `boot-second` with extra words |
| CC-V-20 | A `keys` section in which no entry carries `mint: true` | `E(cluster.keys, CC-KY5)` — a cluster that mints nothing Record-Routes nothing, and would fail on its first dialog-forming request rather than at load |
| CC-V-21 | `shardMap.shards` carrying ids `1` and `3` | `E(cluster.shardMap.shards, CC-SM1)` naming the missing id — the list is the shard space and it is total |
| CC-V-22 | `shardMap` assigning a shard to a member whose `roles` omit `registrar` | `E(cluster.shardMap.shards[…].owner, CC-SM3)` — a shard owns registration state, so its writes would have nowhere to land |

**Key reload (CC-K).**

| # | Given | Expect |
|---|---|---|
| CC-K-1 | Active has key `A` minting; new document adds `B` with `mint: false` | Accepted; the plan reports `keys` changed, no restart, no call disturbed (RL12, K1) |
| CC-K-2 | Active has `A` and `B`, `A` minting; new document flips `mint` to `B` | Accepted (K3) |
| CC-K-3 | Active has `A` only; new document introduces `C` with `mint: true` | Rejected, `E(cluster.keys[…].mint, CC-RL10)` naming `C` and both versions |
| CC-K-4 | New document brings `A`'s `verifyUntil` forward while `A` was the mint key | Rejected, `E(cluster.keys[…].verifyUntil, CC-RL11)` citing `max(L, E_max) + S` |
| CC-K-5 | Two `keys` entries share `id: 3` with overlapping windows | `E(cluster.keys[1].id, affinity-token §6)` |
| CC-K-6 | New document declares two keys with `mint: true` | `E(cluster.keys, affinity-token §6)` — exactly one at any configuration version |

**Trunk reload (CC-T).**

| # | Given | Expect |
|---|---|---|
| CC-T-1 | Reload deleting `trunk[carrier-a]` while a plan stamped with the active version is in flight | Accepted; the plan completes against the retained object, and no new plan names the trunk (RL5, RL6) |
| CC-T-2 | Reload changing `trunk[carrier-a].proxies` only | Accepted; the trunk's breaker and CPS state survive — identity unchanged (RL7) |
| CC-T-3 | Reload renaming `carrier-a` to `carrier-a2` | Accepted; the new identity starts with fresh runtime state, and the old one is retained until its last plan completes (RL6, RL7) |
| CC-T-4 | Reload of the trunk table during an established call | The call is untouched; its route set is not recomputed (RL4, RFC 3261 §12.2.1.2) |

**Shard-map handoff (CC-S).**

| # | Given | Expect |
|---|---|---|
| CC-S-1 | Map `n+1` moves shard `4` from node `A` to node `B` | At `A`, shard 4 → `Draining`; at `B` → `Pending`; every other shard `Unchanged` at both (DS3) |
| CC-S-2 | `A` publishes `drained(4, n+1)` | `B` → `Owner`; the active version advances only when the last shard settles (D11) |
| CC-S-3 | `A` never publishes; `drainTimeout` elapses | `B` → `Owner`, forced; the forced counter increments (DS5) |
| CC-S-4 | A write from `A` for shard 4 lands after `B` became owner | The write fails its CAS on the per-AoR revision; it is never applied beside `B`'s (DS2, [location-service](location-service.md) K3) |
| CC-S-5 | Map `n+2` announced while `n+1` has switched no shard | `n+2` supersedes `n+1` as pending (DS1) |
| CC-S-6 | Map `n+2` announced after one shard has switched | `n+2` is queued; the node finishes `n+1` first; a later `n+3` replaces the queued `n+2` (DS1) |
| CC-S-7 | `shardMap.drainTimeout: 2s` | `E(cluster.shardMap.drainTimeout, CC-DS4)` — below the range |
| CC-S-8 | A shard map reload during an established call | The call is untouched (DS6) |
| CC-S-9 | A token minted while `n+1` is settling | Carries `policy version` = the still-active `n` (D11) |

## 13. Consequences for documents this spec does not own

Named here so that they are tracked rather than discovered. None is performed by this story.

| Where | What must change, and why |
|---|---|
| `deploy/helm/values.yaml` (KO-2) | The reconciliation table in [deployment](../designs/deployment.md) — every adopted, renamed and removed key |
| [number-normalisation](number-normalisation.md) §9/§10 | Its rendered examples are `snake_case`; D4 makes the file syntax `lowerCamelCase` keys with `kebab-case` values, and §10 already assigns the file syntax to DP-1. The §3 data model is unaffected |
| [extension-framework](../designs/extension-framework.md) §binding | Its binding examples are TOML with `quirk_config` / `quirk_overrides`; the document is YAML/JSON (D3) with `quirkConfig` / `quirkOverrides`. The G13 rule itself is unaffected |
| [k8s-deployment-operator](../designs/k8s-deployment-operator.md) | Its `SipxCluster` role list omits `echo` (R1) and its "hot-reloadable" list omits `tenant[]`, `normalisation` and `observability` (§7) |
| `crates/sipx-clstr-node` `listen.rs` / `driver.rs` | `NodeConfig` and `AuthConfig` say in their own doc comments that DP-1 replaces them. `Listeners::receiving` keys arrival on the transport; P6 needs it keyed on the receiving local address before per-role port separation is expressible |
