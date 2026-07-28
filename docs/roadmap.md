# sipx-clstr — roadmap & status

The big picture: what's delivered, what's next, and the epics that group related stories. The
operational detail lives on the [board](stories/README.md) (generated from story frontmatter); this
document is the hand-written narrative around it.

## Status

_As of 2026-07-28:_ the repository is the **design layer only** — vision, roadmap, epic designs
and the seeded backlog. M0 is in flight; there is no code and no Cargo workspace yet. The sipx
kernel this platform builds on has shipped its own M0–M4 (sans-IO core, transactions, all five
transports, DNS, digest client, media, CLI phone) with M5 (depth) in progress.

## Milestones

- **M0 — Foundation on paper.** The repo, the track scaffold, and the load-bearing specs.
  *Done means:* `docs/specs/proxy-behavior.md`, `docs/specs/location-service.md` and
  `docs/specs/affinity-token.md` exist with normative rules and test-vector tables; the
  deterministic-harness and hook-framework designs are accepted; every required sipx change is
  recorded in [upstream.md](upstream.md) with a filed sipx story. No code.
- **M1 — One node that proxies and registers.** The Cargo workspace; a sans-IO proxy engine
  (stateless and transaction-stateful, with forking); a registrar with server-side digest and a
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

## Delivered

- _Nothing yet — M0 is the first milestone._

## Next

- The M0 ready queue on the [board](stories/README.md): `PX-1` proxy behavior spec, `RG-1`
  location service spec, `AF-1` affinity token spec, `CF-1` harness design, `EX-1` hook framework
  spec, `CX-1` filing the upstream sipx stories.

## Epics

An **epic** is a themed group of stories with a shared design doc. Stories join an epic via the
`epic: <slug>` frontmatter field, where `<slug>` matches a design doc at `docs/designs/<slug>.md`.
Use `/track:epic` to start one.

### Proxy engine

The heart of the platform: RFC 3261 §16 as a sans-IO engine — validation, Via handling,
Record-Route, forking, response aggregation, CANCEL, Timer C, loop detection per RFC 5393 — in
stateless and transaction-stateful modes, driven by a proxy-shaped driver over the sipx
transaction layer. Done when a node forwards, forks and recovers per the
[spec](specs/proxy-behavior.md) vectors. [Design](designs/proxy-engine.md).

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

### B2BUA services

Deferred placeholder, recorded so the layers below are designed with it in mind: call queues,
IVR, conference focus and other dialog-terminating features, as a separate service consuming the
platform. Not scheduled before M3 nears completion. [Design](designs/services-b2bua.md).
