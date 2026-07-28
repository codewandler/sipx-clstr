# sipx-clstr — roadmap & status

The big picture: what's delivered, what's next, and the epics that group related stories. The
operational detail lives on the [board](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/README.md) (generated from story frontmatter); this
document is the hand-written narrative around it.

## Status

_As of 2026-07-28:_ **M0 is complete and M1 is defined.** The four load-bearing specs are
written and cross-reconciled — proxy behavior, location service, affinity token, hook framework
— the deterministic-harness design is accepted with its sipx-testkit upstream split decided, and
`CX-1` has filed the kernel gaps as stories in the sipx repo. M1 is scoped below: fourteen
stories, in order, with exit criteria that name the vectors that prove them. `CX-2` creates the
Cargo workspace as its first act.

The sipx kernel this platform builds on has shipped its own M0–M4 — sans-IO core, transactions,
UDP/TCP/**TLS/WS/WSS** transports, DNS, digest *client*, media, CLI phone — all released in
0.2.0, which is the version M1 pins. What the kernel still owes this platform is four small
things, none of which M1 blocks on: header editing operations (`S-15`), the server side of
digest (`S-16`), the `Service-Route` header (`T-16`) and the testkit's timer queue and loopback
link (`X-14`). See the [upstream ledger](upstream.md).

## Milestones

- **M0 — Foundation on paper.** The repo, the track scaffold, and the load-bearing specs.
  *Done means:* `docs/specs/proxy-behavior.md`, `docs/specs/location-service.md` and
  `docs/specs/affinity-token.md` exist with normative rules and test-vector tables; the
  deterministic-harness and hook-framework designs are accepted; every required sipx change is
  recorded in [upstream.md](upstream.md) with a filed sipx story. No code.
- **M1 — One node that proxies and registers.** The Cargo workspace; a sans-IO proxy engine
  (transaction-stateful with forking — stateless mode is specified in PX-1 but implemented in
  M2, when the token path gives it a consumer); a registrar with server-side digest and a
  `LocationStore` behind a trait (in-memory and PostgreSQL). *Done means:* two `sipx` CLI phones
  register through one sipx-clstr node and call each other via it with media flowing direct;
  CANCEL, forking and loop detection pass their spec vectors in the deterministic harness; the
  gate is green.
- **M2 — A cluster you can deploy.** Affinity tokens live in Record-Route/Route; flow_ref and the
  connection-owner RPC; trunk routing with circuit breakers and CPS caps; the rtpengine NG
  adapter behind `MediaRelay`; roles by config; the 3-zone reference deployment. *Done means:* in
  a 3-zone cluster, a call between UAs registered on different edges completes with relayed
  media; mid-dialog requests route by token with **zero** cross-node dialog lookups, asserted by
  metric; killing any single node leaves new calls and registrations working.
- **M3 — Modern reachability.** rport (RFC 3581), Path (RFC 3327), Outbound (RFC 5626), GRUU
  (RFC 5627), WebSocket clients (RFC 7118), push (RFC 8599), session timers (RFC 4028), 100rel
  (RFC 3262), overload control (RFC 7339/7415). *Done means:* a WSS client behind NAT registers
  with Outbound through an edge and is reachable; a push wakes an unregistered client into an
  answered call; session timers reap dead dialogs; overload sheds load in the simulated cluster
  without collapse.
- **M4 — Service families.** SUBSCRIBE/NOTIFY framework (RFC 6665), REFER and transfer, presence,
  STIR/PASSporT, SIPREC, IMS profile options, and the B2BUA service (queues, IVR, conference).
  Scope is selected when M3 nears completion, not now.

### M1 in detail

M1 is the milestone where the specs become a program. Everything below is deliberately *one
node*: nothing here needs a second one to be true, which is what makes it a foundation rather
than half of M2.

**The story set, in dependency order.** Priorities on the board match these numbers, so
`/track:next` walks M1 top to bottom.

| # | Story | What it adds | Proved by |
|---|---|---|---|
| 1 | `CX-2` | the Cargo workspace, the lints, the gate | the gate runs green on an empty workspace |
| 2 | `PX-2` | design: one server transaction fanning out to N client transactions | design accepted; the effect vocabulary is named |
| 3 | `CF-5` | the deterministic harness — seeded, virtual-time, multi-node | a scenario replays byte-identically from its seed |
| 4 | `RG-3` | REGISTER on the in-memory `LocationStore` | `LS-C-*`, `LS-R-*` |
| 5 | `PX-5` | stateful forwarding with forking and response aggregation | `PB-V-*`, `PB-F-*`, `PB-R-*` |
| 6 | `PX-6` | CANCEL and Timer C | `PB-C-*` |
| 7 | `RG-6` | forking target sets built from location lookups | `PB-F-2` fed by `LS-L-*` |
| 8 | `PX-7` | the proxy torture vectors, run in the harness | the whole `PB-*` table, as a report |
| 9 | `RG-2` | server-side digest — challenge, verify, replay window | RFC 7616 vectors + a replayed `nc` |
| 10 | `RG-4` | the PostgreSQL `LocationStore` | the `LS-*` tables again, unchanged, on a second backend |
| 11 | `ET-1` | the e2e-tester role and the probe contract | spec accepted with a verdict taxonomy |
| 12 | `ET-2` | the sans-IO probe engine | failure scenarios as seeded harness tests |
| 13 | `ET-3` | the echo answering endpoint (signalling only) | a probe reaches it and gets `pass` |
| 14 | `CX-3` | the milestone's own end-to-end proof against real phones | two `sipx` CLI phones, one node, a real call |

**Exit criteria.** M1 is done when every one of these is true, and each is a command someone
else can run:

1. The gate is green: `cargo fmt --check`, `cargo clippy --all-targets --all-features
   -D warnings`, `cargo test --workspace --all-features`, the provenance check, the feature
   matrix. No `unsafe` anywhere; no `unwrap`/`expect`/`panic` in library code.
2. Two `sipx` CLI phones register through one sipx-clstr node, call each other through it, and
   hang up — **with media flowing directly between them**, no relay in the path. (`CX-3`)
3. Every vector in `docs/specs/proxy-behavior.md` §12 for validation, preprocessing, forwarding,
   responses and CANCEL passes in the harness. The stateless rows (`PB-S-*`) and the affinity
   rows (`PB-A-*`) are **not** in M1's set — they have no consumer until the token path exists.
   (`PX-7`)
4. Every vector in `docs/specs/location-service.md` §9 passes, and passes **identically** on both
   `LocationStore` backends — the in-memory one and PostgreSQL. A backend that needs its own
   version of a vector has broken the contract. (`RG-3`, `RG-4`)
5. A digest-challenged REGISTER authenticates; a replayed nonce-count is rejected; a
   *retransmitted* REGISTER with the same nonce-count is not mistaken for a replay. (`RG-2`)
6. The probe dials the node, reaches the echo endpoint and reports `pass`; with the node stopped
   it reports `fail(step, cause)` within its timer budget rather than hanging. (`ET-2`, `ET-3`)

**Explicitly out of M1, with the reason:**

| Out | Why |
|---|---|
| `PX-4` stateless forwarding | specified in `PX-1`, but nothing consumes it until mid-dialog requests carry tokens. Implementing it now means maintaining an untested second path for a milestone. |
| all `AF-*` (affinity tokens) | one node has no affinity problem. The token is M2's defining subsystem and M1 must not pre-empt its shape. |
| all `RT-*` (trunks, route plans) | M1 routes to registered contacts only. No carrier, no egress policy. |
| all `ME-*` (media control) | M1's media flows direct between endpoints; there is no relay to control. |
| `DP-1` config schema, `DP-2` topology, all `KO-*` | the deployment surface is a cluster surface. M1's node takes a **provisional** minimal config — listeners, the registrar realm, the store URL — which `DP-1` replaces rather than extends. Said out loud so nobody mistakes it for the schema. |
| `RG-5` sharding | one node is one shard. |

**M1 does not block on the kernel.** Three of the four filed sipx stories would make M1 code
nicer, not possible: without `S-15` the proxy rebuilds a header collection to pop a `Via`
(correct, just O(n) clones); without `X-14` the harness carries its own timer queue. `S-16` is
the one real dependency — digest verification is protocol-generic and this repo does not
shadow-implement kernel logic (AGENTS.md rule 6) — so `RG-2` sits at position 9, late enough
that the kernel work has room, and M1's other thirteen stories do not wait for it.

## Delivered

- **The M0 specs** (0.2.0): `docs/specs/proxy-behavior.md`, `location-service.md`,
  `affinity-token.md`, `hook-framework.md`, and the accepted deterministic-harness design —
  written concurrently, cross-reconciled (token budget 157 B ≤ 200 B end-to-end, hook phases
  aligned to the proxy pipeline, no-mid-dialog-token-refresh propagated to media-control and
  KO-9). Itemized history in [CHANGELOG.md](https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md).
- **The design scaffold** (0.1.0): vision, roadmap, eleven epic designs, the reviewed backlog,
  the upstream ledger, the architecture charts.

## Next

- `CX-2` — the Cargo workspace and the gate, the first act of M1. Then the M1 set above, in
  order. The operator epic advances in parallel (`KO-2` in progress) and the kernel stories
  `CX-1` filed advance on sipx's own schedule.

## Epics

An **epic** is a themed group of stories with a shared design doc. Stories join an epic via the
`epic: <slug>` frontmatter field, where `<slug>` matches a design doc at `docs/designs/<slug>.md`.
Use `/track:epic` to start one.

### Proxy engine

The heart of the platform: RFC 3261 §16 as a sans-IO engine — validation, Via handling,
Record-Route, forking, response aggregation, CANCEL, Timer C, loop detection per RFC 5393 — in
stateless and transaction-stateful modes, driven by a proxy-shaped driver over the sipx
transaction layer. Done when a node forwards, forks and recovers per the vectors of
`docs/specs/proxy-behavior.md` (PX-1's deliverable). [Design](designs/proxy-engine.md).

### Registrar & location service

REGISTER processing with server-side digest, and bindings in a strongly consistent
`LocationStore` whose per-AoR compare-and-swap contract makes updates serialize on any backend
(in-memory for the harness, PostgreSQL first in production). Done when lookup yields a routable
forking target set, not just a Contact. [Design](designs/registrar-location.md).

### Cluster affinity & connection ownership

The defining subsystem: signed opaque tokens in Record-Route/Route/Path and flow references that
let any healthy edge route a mid-dialog request with zero global lookups, plus the
connection-owner RPC that delivers requests to the edge owning a client's connection. Done when
the token spec's vectors round-trip and ownership survives node loss per the failure table.
[Design](designs/cluster-affinity.md).

### Outbound routing & trunks

Which egress, in what order, and when to stop: RFC 3263 route plans on an async shared-cache
resolver, trunk state (circuit breakers, CPS caps, failover rules), and overload control per
RFC 7339/7415. [Design](designs/routing-trunks.md).

### Media control

Media as another cluster: the `MediaRelay` trait with a null relay for tests and an rtpengine NG
adapter for production, media-node selection by rendezvous hash, and the chosen node riding in
the affinity token so every mid-dialog update reaches the same relay.
[Design](designs/media-control.md).

### Extension framework & RFC registry

Capabilities as declared modules over typed hook phases, a machine-readable RFC registry that
generates syntax artifacts and feeds conformance, and deployment profiles that select provably
compatible sets. Done when adding a syntax-only RFC is a registry entry, not a patch.
[Design](designs/extension-framework.md).

### Conformance & deterministic harness

The executable form of the north star: a seeded, virtual-time, multi-node simulation in which
every cluster behavior must reproduce before it touches a socket, plus per-requirement
conformance tracking with four coverage kinds (syntax, behavioral, role, interop).
[Design](designs/conformance-harness.md).

### Roles, topology & operations

One binary, roles by config; the 3-zone reference topology and its Kubernetes expression;
observability whose metrics prove the invariants (the cross-node dialog-lookup counter must read
zero); and the honest HA statement. [Design](designs/deployment.md).

### End-to-end call probe (`e2e-tester` role)

The outside view. Every other epic proves the platform from the inside; this one adds a role that
dials the border the way a customer does — through DNS/VIP, per edge and per transport, on a
schedule and on demand via a private trigger API — calls an echo endpoint in a dedicated test
tenant, and turns the result into a verdict (`pass` / `fail(step, cause)` /
`inconclusive`). Signalling echo only for now; a media assertion, when it comes, goes through the
relay, never through RTP in the process. The probe engine is sans-IO, so its failure scenarios are
seeded harness tests before they are a cron. Lands with the M1 node (spec and engine) and becomes
continuous with the M2 reference deployment; the trigger API doubles as a rollout gate.
[Design](designs/e2e-tester.md).

### Kubernetes operator, Helm packaging & autoscaling

The deployment end state: `helm install` with a single `values.yaml` deploys an operator that
reconciles one `SipxCluster` resource into every cluster resource — starting with a small but
genuinely clustered environment on local k3s, scaling up to the 3-zone topology. An operator
rather than templates because the events that break a SIP cluster are lifecycle events: draining
long-lived flows before an edge dies, drain-then-switch on shard-map changes, two-phase token key
rotation, and profile compatibility failing at reconcile time rather than on a call. Config is
re-deployable at any moment: the operator diffs the new `values.yaml` against observed state and
either hot-reloads it, stages a drained rollout role by role, or rejects it — pausing on any health
regression rather than pressing on. The media pool
has two first-class modes: **managed**, where the operator runs rtpengine itself so the local demo
is self-contained, and **external**, where a production pool on dedicated hosts is declared and
validated but never touched. Phase 2 adds
autoscaling on SIP-shaped Prometheus signals (registrations per shard, CPS and in-flight
transactions, media sessions, shed rate — never CPU), with scale-in routed through the same drain
path and gated on the invariant metrics. Boundary: DP-1/DP-2 own the config schema and the
topology; this epic owns packaging, automation and capacity, and the CR spec *is* DP-1's schema.
[Design](designs/k8s-deployment-operator.md).

### Live cluster visualization (`constellation`)

The north star, watchable: the architecture chart drawn live, fed by an SSE stream that is a
serialization of the harness trace — messages as particles, faults (drops, partitions, node
kills, timer storms) as visible events, and the DP-3 invariant counters as a HUD that must not
move. Three feeds share one schema and one page: a paced sim replay (works against the existing
harness), an interactive sim with fault injection by POST, and — once DP-3/ET-5/DP-7 land — the
real deployment. A dev tool, never part of the deployment surface.
[Design](designs/cluster-viz.md).

### B2BUA services

Deferred placeholder, recorded so the layers below are designed with it in mind: call queues,
IVR, conference focus and other dialog-terminating features, as a separate service consuming the
platform. Not scheduled before M3 nears completion. [Design](designs/services-b2bua.md).
