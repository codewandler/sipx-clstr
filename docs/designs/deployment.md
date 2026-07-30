# Design: Roles, topology & operations

**Status:** proposed · **Pillar:** Cluster · **Epic:** `deployment` ·
**Stories:** DP-1 … DP-15

## Why

The operational contract: roles by config, a reference topology, and an honest HA statement.

A clustered platform that cannot be deployed honestly is a demo. This epic owns the operational
contract: how roles map onto processes, what the reference topology looks like, which metrics
prove the architectural invariants rather than merely decorating dashboards, and exactly what HA
is promised. SIP makes this harder than a web service — long-lived connections pin to nodes, UDP
needs source-address fidelity, and a generic HTTP ingress understands none of it.

## Approach

**One binary, roles by config.** Every signalling node runs the same binary; configuration
selects roles — `edge`, `registrar`, `inbound-proxy`, `outbound-proxy`. At small scale all roles
share one process; at large scale they scale separately, without SDK changes. DP-1 designs the
config schema (typed, versioned, reloadable where safe: trunk state, token keys, shard map) —
config-first membership per `cluster-affinity`.

**The schema is [cluster-config](../specs/cluster-config.md)** (DP-1), and the reason it is a
normative spec rather than a paragraph here is that it arrived with three artefacts already
asserting a schema: the chart's `cluster:` tree, the provisional `NodeConfig`/`Listeners` types in
`sipx-clstr-node`, and half a dozen specs that each name configuration they expect to exist. The
story's job was to reconcile those into one definition, not to add a fourth. Four decisions carry
it.

*One document, two readers.* The document is **cluster-scoped**, and a node reads a projection of
it through an identity — node id, zone, role set — supplied from outside (spec §5). This is what
makes the chart's claim ("the same tree a node reads") true without giving every node its own
file: `values.yaml`, the `SipxCluster` spec and what a node loads are one set of bytes, so a diff
between two versions is reviewable in one place, and "which configuration is this cluster running"
has an answer. Loading is a pure function of `(bytes, identity, environment)` — no socket, no
clock, no second file — so the whole of validation runs in the harness (AGENTS.md #2). Even
`${NODE_IP}` interpolation takes its values as an argument rather than reading the process
environment, which is the difference between a schema that can be tested and one that can only be
deployed.

*Typed and versioned means two numbers, not one.* `apiVersion` versions the **schema** — additive
changes only within one, refuse rather than best-effort parse an unknown one, and `v1alpha1`
promises nothing, said out loud so it is not quietly treated as stable. `version` versions the
**configuration**: a `u32`, strictly increasing, and deliberately not invented here — it is the
value `affinity-token` §3 already stamps into a token's `policy version` field, which is where the
width comes from. Two specs were already saying "at every configuration version"
(`affinity-token` §6, §12.2 CT1) about a thing nothing defined; now it exists and a rollback is a
new version carrying old content, rather than a number going backwards.

*A role set, and roles that never decide anything.* Six roles — `edge`, `registrar`,
`inbound-proxy`, `outbound-proxy`, `e2e-tester`, `echo`; the sixth because
[e2e-probe](../specs/e2e-probe.md) §9 puts it there. The property that makes "any combination"
safe is stated as a rule: **a role selects which decision paths are wired and never what a request
decides**. Direction, tenant, scope and trunk come from the ingress binding and the message, so
`inbound-proxy` and `outbound-proxy` in one process cannot disagree — there is nothing for them to
disagree about. The one refused combination is the probe roles beside the call-path roles, and it
is refused because [e2e-probe](../specs/e2e-probe.md) §11 requires it: a probe that enters through
the node it is probing measures a path no caller takes, which is why the architecture chart draws
it outside the border.

*Three reloadable things, three different semantics.* "Reloadable" was one word covering three
unrelated mechanisms, and naming them separately is most of the work:

| Subset | Semantics | What happens to work in flight |
|---|---|---|
| Trunks | **stamp and retire** — a `RoutePlan` already carries the policy version it was built under, so a plan completes against the trunk objects of its own version and the new table serves new plans. A trunk deleted mid-call strands nothing; per-trunk breaker and CPS state survives a reload that leaves the trunk's identity unchanged, because resetting every breaker turns a routine config push into a synchronized retry storm | Established dialogs untouched (RFC 3261 §12.2.1.2 — route sets are not recomputed); in-flight transactions finish on the stamped version |
| Token keys | **distribute, then activate** — `affinity-token` §6 K1–K4 unchanged. What DP-1 adds is the pair of transition rules a spec that sees one document cannot state: a reload may not flip `mint` to a key absent from the *previous* version (that is K1 and K3 collapsed, and it mints tokens some healthy node cannot verify), and may not bring a retiring key's verify window forward | Nothing disturbed, in either direction — that is what the two rules exist to make true rather than hoped for |
| Shard map | **drain, then switch** — the losing node stops accepting for a migrating shard, finishes what it holds, publishes `drained`; the gaining node starts only on that or on a bounded deadline. State table and transitions in spec §9.4 | No call is affected (shards own registrations, not dialogs). The at-risk work is an in-flight REGISTER write, and it cannot be split: a late write from the old owner fails its CAS against the per-AoR revision (`location-service` K3). The deadline can stall a handoff; it cannot corrupt one — and forcing is counted, because the usual reason a drain never finishes is that the old owner is gone, which is exactly when the shard must move |

Everything else is `rollout`-class, and the class is a **property of the field** rather than a
verdict reached by diffing — otherwise the operator and the node classify a change differently and
the operator pushes something no node applies. A reload is atomic per document: one
`rollout`-class field in it and the whole reload is refused, naming the field, with the last good
configuration still running.

**What the chart said, and what the schema says.** `deploy/helm/values.yaml` is a real proposal and
most of it is adopted verbatim — `name`/`environment`/`zones`, `listener` with `bind`/`advertise`,
`locationStore`, `destinationSet`, `routeRule`, `trunk`, `observability`, `probe`, `nat`,
`security`. Where a spec disagreed, the spec won:

| Chart | Schema | Why |
|---|---|---|
| `numbering:` — a flat global normaliser (`normalize`, `normalizeFields`, `ruriDigits*`) | `normalisation:` — named profiles bound at ingress scope and per trunk | [number-normalisation](../specs/number-normalisation.md) N23/N27: two binding points, no global default, no implicit `+`. The flat form is a second dialect for the same job |
| `numbering.outbound.e164Plus` | an `ensure_prefix: "+"` in an egress profile | N28 — a `+` appears only because a declared rule put it there |
| `numbering.outbound.anonymousFrom` | `privacy` policy, unowned here | RFC 5379 identity/privacy is RT-7's, not the normaliser's ([number-normalisation](../specs/number-normalisation.md) §2) |
| `media:` — one global block with `codecs.{offer,transcode,mask}` | `TrunkMediaPolicy` per trunk; a `mediaPool[]` for pool facts | [media-relay](../specs/media-relay.md) §13.1 makes policy a field of the trunk and MP6 forbids selection derived from a domain or a pattern. `offer`/`strip`/`transcode` are §13.3's *NG wire keys*, and exposing them as configuration puts the protocol in the config file |
| `media.codecs.offer: [PCMA, PCMU, …]`, `mask: all` | refused at load today | MP12/G-M6: until `CF-3` is green the only admissible policy is `{AsReceived, None, Disabled}`. As written, the default deployment set would not start |
| `media.ngTimeout`, `ngRetries` | `T_ng`, `B_ng` and the five other named timers | [media-relay](../specs/media-relay.md) §8 K6 fixes seven values with defaults and a bound; the retransmission schedule is four fixed attempts and is not a knob |
| `media.portRange`, `timeout`, `silentTimeout`, `strictSource`, `generateRtcp`, `rewrite` | `mediaPool[]` (KO-7) for the pool facts; the rest is not a platform surface | §14 assigns pool operation to KO-7, and E5 forbids emitting keys §7 does not list — a `rewrite` list is the relay's vocabulary, not this platform's |
| `registrar.{maxExpires,minExpires,maxContacts,realm,authBackend}` | `tenant[]` | [location-service](../specs/location-service.md) §5.2/§5.5 and [registrar-auth](../specs/registrar-auth.md) §2 make every one of these **per tenant**. A node-wide spelling is a coarser policy the registrar's own spec cannot see |
| `registrar.shards: 1` | `shardMap` | A count cannot express ownership, and drain-then-switch needs an owner per shard and a version to fence on |
| `quirkProfile: []` (top level) | `quirks` / `quirkConfig` / `quirkOverrides` on `trunk[]` and `domain[]` | [extension-framework](extension-framework.md) G13, after EX-10: a binding declares what it binds and which profile wins a contested target; a profile names nowhere it applies |
| `listener[].role: management`, `transport: http` | a `management:` section | `listener[]` is the SIP listener set, and every entry's advertised address is part of the node's SIP identity ([proxy-behavior](../specs/proxy-behavior.md) §5). An HTTP endpoint in that list makes the identity set wrong |
| `listener[].role: <one>` | `listener[].roles: [<role>…]` | A listener shared by co-resident roles has to be expressible, or the co-located case cannot be configured |
| `limit[]` and `limits:` | `rateLimit[]` and `timers:` | Two sections one letter apart, one a list and one a map, is a defect waiting for a mis-edit |
| `limits.{frTimer,frInvTimer,timerC}` | `timers.{t1,timerB,timerF,timerC}` | RFC 3261 §17.1.1.2 / §17.1.2.2 name these; the chart's `timerC: 600s` carries the comment "the 180s default silently caps long calls", and Timer C cannot — it is cancelled by the final response (§16.6 step 11). The cap belongs to `maxCallDuration`, and a knob explained by a wrong reason is the one that gets tuned |
| `security.maxForwards: 10` | default **70** | RFC 3261 §16.6 step 3, already fixed by [proxy-behavior](../specs/proxy-behavior.md) §5. It is the value inserted when a request carries none, not a hop budget |
| `security.rejectUserAgents: [<tool names>]` | `security.userAgentDenyList`, no default | The mechanism is worth having; a shipped list of names is a default that goes stale and that nobody reviews |
| `crd.{apiGroup,apiVersion}` | `apiVersion: sipx.dev/v1alpha1` on the document | One version field on the thing being versioned |
| — (absent) | `profile:` | [hook-framework](../specs/hook-framework.md) §8 and KO-1 both require a deployment profile; the chart has nowhere to name one |

**Membership, keys and the shard map are integrated by reference, not written here.** They are
AF-6's sections; DP-1 fixes only the seam (spec §10): where they live in the tree, that they are
versioned with the document and reloadable, that a `membership` entry must carry a node id meeting
`affinity-token` §12.2 CT1's uniqueness — a correctness input, not a convention, and the *same* id
[media-relay](../specs/media-relay.md) §6.2 C2 requires to be cluster-unique for NG cookies — plus
its zone and role set for the startup cross-check, and that a `keys` entry carries §6's attributes
with the secret supplied by reference. Deliberately left to AF-6 and named so it is not lost: the
rotation runbook's wall-clock arithmetic, the persisted-incarnation option CT2 requires of a
deployment whose clock may step backwards, the tenant name↔id assignment procedure, and what a
dynamic membership service would later replace. Nothing in the spec's §1–§9 changes when AF-6
lands; it writes into §7's rows.

*Considered for upstream: no.* A configuration schema names this platform's own concepts — roles,
zones, shards, tenants, trunks, media pools, profiles — and the kernel has no opinion about any of
them. The single place it touches kernel surface, a listener's bind/advertise split, maps onto
`sipx_transport::Config`'s existing fields rather than re-deriving them, as DP-5 already
established below.

**Reference topology** (DP-2): one region, three zones — 3+ edges, the PostgreSQL location
store in HA, 2+ routing/policy instances, 2+ media nodes per media network, DNS SRV for external
discovery, an L4 VIP or per-transport addresses. Exposure: UDP/TCP 5060 where required, TLS 5061,
WS/WSS on explicit endpoints, media on dedicated UDP ranges, management strictly private. The
Kubernetes expression is explicit about SIP's constraints: host networking or a source-preserving
L4 dataplane for public UDP with same-flow affinity — transaction-scoped messages
(retransmissions, CANCEL, ACK to a non-2xx) must keep landing on the edge that holds the
transaction; a correctness requirement, not tuning (see cluster-affinity) — no NAT/conntrack
layers in the media or signalling path, long-lived
TCP/TLS/WS flows pinned to their owning edge (`externalTrafficPolicy: Local`-class routing),
PodDisruptionBudgets with graceful connection draining, media nodes on dedicated host-network
nodes or outside the cluster entirely.

**A listener binds one address and advertises another** (DP-5). The two are declared
independently, per listener, and neither is derived from the other:

```yaml
listener:
  - transport: udp
    bind:      "0.0.0.0:5060"        # what the socket gets
    advertise: "203.0.113.9:5060"    # what a peer can route to
```

This is not a convenience. A node on a private address that is reached on a public one has two
addresses, and only one of them is an answer to "where do I reach you". Every place an address
enters a message — the `Via` sent-by (RFC 3261 §18.1.1), a `Contact` (§8.1.1.8), the
`Record-Route` a proxy inserts (§16.6 step 4) — must carry the advertised one. The bound address
is a fact about a socket, and a peer cannot send to it. The failure is not cosmetic and it is not
immediate: a wrong `Record-Route` starts calls that cannot be transferred or hung up, because the
mid-dialog request never comes back (`vision`'s state-rides-the-message rule). Refusing to
advertise an unspecified address (`0.0.0.0`, `::`) is part of the same argument — "everywhere" is
an answer to where to listen and not to where to be reached, so a node given nothing else to say
refuses to start rather than accepting calls it can only half-serve.

**Which address goes into a message is a decision**, in the sans-IO sense: a pure function of the
listener's declared configuration and the transport the message arrived on, with no socket in it.
It lives in `sipx-clstr-node`'s `listen` module because it is *configuration* semantics — DP-1's
schema, given meaning — and it runs in the deterministic harness without binding anything. The
receiving listener is what the answer is keyed on, because a node may advertise one address on
UDP and another on TLS; the identity **set** it recognizes is every listener's, since any edge
recognizes any edge's `Route` (proxy-behavior §5) and that starts with recognizing its own.

*Considered for upstream: no — the split itself is already the kernel's and stays there.*
`sipx_transport::Config` separates `bind` from `sent_by`/`sent_by_port` for exactly this reason
and stamps the sent-by into every `Via` it writes, so this platform maps onto that field rather
than re-deriving a `Via` beside it. What is cluster-specific is the *set* of listeners and the two
headers the kernel never writes: `Record-Route`, which is proxy orchestration, and a `Contact`
naming this platform. One gap does sit upstream: the kernel derives the TLS sent-by port from the
port it bound TLS on, so a TLS listener advertising a *different port* than it binds cannot be
expressed through `Config` — advertising a different host works on all three transports,
a different port on UDP and TCP. It has not bitten a deployment (the public and private TLS ports
are conventionally both 5061) and it is a kernel change, not a fork here.

**Observability proves the invariants** (DP-3). Beyond standard metrics/traces/structured SIP
event logs, the design names invariant metrics: the cross-node dialog-lookup counter (must read
zero — the M2 exit criterion), token verification failures by cause, flow-RPC delivery outcomes,
per-trunk breaker state, per-shard registration write latency. An invariant metric that moves is
a bug, and alerting is built on exactly those.

**The HA statement** (DP-4) is a user-facing table, not marketing: **service HA** — node loss
does not stop registration; new inbound and outbound calls continue; clients re-register to
another edge; upstreams retry another destination — is guaranteed from M2. **Call-survival HA** —
established calls surviving the loss of their signalling or media node — is explicitly out of
scope for v1; each failure mode gets a row: what breaks, what the endpoint observes, what
recovers automatically, and in what time bound.

## Alternatives considered

- **Role-specific binaries.** Rejected: multiplies build/release surface; roles differ by wiring,
  not by code.
- **A per-node configuration document** (DP-1). Rejected: the moment two nodes read different
  bytes, "the configuration version" stops naming anything, and both `affinity-token` §6's key
  rules and §12.2 CT1's id-uniqueness rule are statements about a version. One document plus a
  small out-of-band identity keeps every cross-node invariant checkable by reading one file.
- **Deriving the reload class by diffing** (DP-1). Rejected: the operator and the node would each
  compute it, and the first time they disagreed the operator would push a change no node applies.
  The class is declared per field, in the schema both of them read.
- **A schema version alone, without a configuration version** (DP-1). Rejected: two specs already
  say "at every configuration version", and a token's `policy version` field already needs the
  number on the wire. One of the two was going to be invented anyway; better the one whose width
  is already fixed.
- **Tolerating unknown fields** (DP-1). Rejected: a misspelled key that is silently ignored is a
  policy nobody is applying, and the first ones anybody misspells are `security` and quota fields.
  Closed-world validation costs a restart on a typo and saves the failure mode where the config
  file says one thing and the running node does another.
- **Service mesh / HTTP ingress in front of signalling.** Rejected: SIP over UDP with Via/rport
  semantics and owned TCP flows is invisible to HTTP-shaped dataplanes; L4 with source
  preservation is the requirement.
- **Multi-region from day one.** Deferred: the design keeps the decision explicit (home region
  per AoR, regional transaction state, global config replication) but v1 ships one region, three
  zones.

## Risks & open questions

- Kubernetes versus plain hosts as the *reference* (k8s manifests are DP-2's deliverable, but
  bare-metal/systemd must remain first-class for media nodes).
- ~~Config reload semantics for the shard map~~ — **settled** in
  [cluster-config](../specs/cluster-config.md) §9.4: drain, then switch, with the losing node
  stopping first and the gaining node starting on `drained` or on a bounded, counted deadline. An
  in-flight REGISTER write is never split, because a late write from the old owner fails its CAS
  against the per-AoR revision rather than landing beside the new owner's.
- Whether the L4 VIP or DNS SRV is primary for edge discovery in the reference deployment.
- **`nat:` has no owning spec.** The schema gives it a home and a reload class, and no more:
  detection, `received`/`rport` handling (RFC 3261 §18.2.1, RFC 3581) and contact aliasing are
  real behaviour with no normative text behind them, and the chart's field names came from
  somewhere other than an RFC. It needs a story before it needs more schema.
- **The listener set is keyed by transport, and needs to be keyed by arrival address.**
  `Listeners::receiving` answers "which listener did this arrive on" from the transport alone,
  which is right for one endpoint per node and wrong the moment two roles want separate ports on
  one node. Until it keys on the receiving local address, [cluster-config](../specs/cluster-config.md)
  §5 P6 refuses that projection at load rather than serving it wrongly — the same posture
  `media-relay` MP12 takes toward a policy the relay cannot yet honour. Co-resident roles sharing
  one listener are unaffected, so no role combination is blocked by it.
- **The chart has to follow the schema** (KO-2 owns `deploy/helm/**`): the reconciliation table
  above is the change list, and until it lands the default deployment set's `media.codecs` block
  is a configuration `media-relay` G-M6 refuses to start on.
- How many quirk bindings a large deployment really carries. §8 V8 sets declared ceilings — 8
  profiles per binding, 4096 bindings per node — because startup validation is superlinear in
  their product, and `extension-framework` left that bound here. The numbers are a first estimate
  and are raised by a spec change, never by a config flag.
- Whether a `Record-Route` on a TLS listener should be `sips:` rather than `sip:;transport=tls`.
  The transport parameter (§19.1.1) says "come back over TLS" about this hop and nothing more;
  `sips:` is a claim about the whole remaining path. DP-5 takes the narrower one deliberately and
  leaves the policy to whoever specifies the platform's TLS posture.
- Serving a TLS listener at all: it needs a server identity (certificate and key). **Settled in
  shape** — `listener[].tls` names it, and by [cluster-config](../specs/cluster-config.md) §8 V9
  the document carries a *reference* rather than the material, so the operator renders the
  document into a ConfigMap and the references into Secrets and the safe shape is the only shape.
  Resolving a reference is IO and therefore a start-up failure of the driver, not a load error.
  Its advertised address is already decided, so the listener is correct the moment it can be
  bound.

## Acceptance / done

The union of DP-1 … DP-5: a node boots into configured roles from a validated config file; a
listener binds one address and advertises another, and what arrives on the bound address is
answerable at the advertised one; the 3-zone reference deployment stands up from the repo's
manifests; the invariant metrics exist and the M2 kill-a-node scenario shows service HA within the
documented bounds; the HA table published in the docs matches what the harness demonstrates.

## Validated review remediation (2026-07-30)

`DP-13` carries projected roles into dispatch and refuses unavailable roles, `DP-14` bounds all
admitted work including registrar and refusal paths, and `DP-15` makes the built and tested release
artifact immutable and reproducible. These are deployment orchestration; protocol parsing and
transaction mechanics stay in sipx.
