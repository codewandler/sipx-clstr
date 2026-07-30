# Design: Kubernetes operator, Helm packaging & autoscaling

**Status:** proposed · **Pillar:** Cluster · **Epic:** `k8s-deployment-operator` ·
**Stories:** KO-1 … KO-17

## Why

One `values.yaml` to a running, healthy, resizable cluster — delivered and kept true over time.

The deployment goal is one file: an operator installs from a Helm chart, reads a single
`values.yaml`, and stands up a working clustered environment — first on a local k3s cluster so the
loop from "edit config" to "clustered platform running" closes in minutes, later on real
multi-zone infrastructure, and later still resizing itself from Prometheus metrics. Today's
[deployment](deployment.md) epic answers *what the topology is* (roles, zones, SIP's constraints
on the dataplane) and DP-2 authors its Kubernetes expression as manifests; this epic answers *how
that topology is delivered and kept true over time*. The split matters: DP-1/DP-2 own the config
schema and the topology shape, this epic owns packaging (chart), automation (operator) and
capacity (autoscaling), and it must not invent a second configuration dialect — the custom
resource **is** DP-1's schema, versioned with it.

An operator rather than templates alone, because the lifecycle events that break a SIP cluster are
exactly the ones a template cannot express: rolling an edge means draining long-lived TCP/TLS/WS
flows whose connections cannot move; changing replica counts on registrar shards remaps AoRs by
rendezvous hash and needs drain-then-switch; token keys must be distributed to every node before
the first token minted with them is verified elsewhere; a role/profile combination that is not
provably compatible must fail at reconcile time, not on a call. Those are reconciliation
invariants with observable status — which is what an operator is for.

## Approach

**Phase 1 — one config file, one cluster.**

*The chart is the product.* `helm install` deploys the operator, its CRDs, RBAC and a single
`SipxCluster` custom resource rendered from `values.yaml`. `values.yaml` maps 1:1 onto the CR
spec — no derived dialect, no second source of truth — so "the config file" and "the desired
state" are the same document.

*The `SipxCluster` CR* (KO-1) carries: zones and their nodes; roles and replica counts (`edge`,
`registrar`, `inbound-proxy`, `outbound-proxy`, `e2e-tester`); listeners per transport with their
addresses and port ranges; the deployment profile (`extension-framework` — an incompatible set
fails validation); the location store (PostgreSQL connection/HA shape); the media pool (rtpengine
nodes, RTP port ranges, and the managed-or-external mode); token keys and membership (`cluster-affinity`, AF-6); trunks and route
policy; probe configuration (`e2e-tester`). `status` reports conditions (`Ready`,
`ProfileCompatible`, `ShardMapConverged`, `KeysDistributed`), observed shard map, per-role ready
counts, and the last probe verdict.

*The reconcile loop* (KO-3) projects that CR onto Kubernetes objects: workloads per role (edges
with host networking or a source-preserving L4 path, media on dedicated host-network nodes or
outside the cluster entirely), Services, ConfigMaps for node config, Secrets for credentials and
token keys, PodDisruptionBudgets, NetworkPolicies keeping management interfaces private, and the
scrape configuration for the DP-3 metric set. Everything the manifests in DP-2 express by hand,
the operator derives — DP-2 stays the reference topology and the conformance target for what the
operator generates.

*Local k3s is a first-class target* (KO-2), not an afterthought: a single-node profile with one
PostgreSQL, one rtpengine, a small edge replica count and HA relaxed, host networking plus a
declared UDP port range, standing up on a laptop-class k3s. The acceptance is blunt: `helm install`
on local k3s, then two sipx CLI phones register and call through it, and the `e2e-tester` probe
([e2e-tester](e2e-tester.md)) reports `pass`.

*The media pool has two modes, both first-class* (KO-7). In **managed** mode the operator deploys
and operates the rtpengine pool itself — pods on host networking with declared RTP port ranges,
the NG control endpoint reachable only on the private network, readiness gated on NG responding,
and pool membership published into node config so media-node selection by rendezvous hash
([media-control](media-control.md)) sees exactly the nodes that exist. This is what makes the
Helm + local-k3s demo self-contained: one `helm install`, no external dependency, media flowing.
In **external** mode the pool is *declared, not managed* — production deployments commonly run
rtpengine on dedicated hosts or outside Kubernetes entirely, and there the operator owns no
lifecycle: it validates the endpoints (NG reachability, advertised port ranges, version/feature
compatibility), publishes them as pool membership, and reports them in status, but never creates,
restarts or scales them. The CR expresses the mode per pool, so a deployment can migrate from
managed to external without changing anything else, and one cluster can hold both (a managed pool
for a zone that has no dedicated media hosts, external pools elsewhere). What is identical in both
modes is the *contract*: `MediaRelay` talks NG to whatever the pool contains, and the signalling
process never links media — an operator-managed rtpengine is still a separate cluster, merely one
this operator happens to deploy.

*Config is re-deployable at any time* (KO-8). Changing the platform is editing `values.yaml` and
re-deploying — `helm upgrade`, or an edit to the `SipxCluster` — and the operator picks the change
up and works the cluster towards it gracefully. It diffs desired against observed and **classifies**
every field:

- **hot-reloadable** — DP-1's reloadable subset (trunks, token keys, shard map, route policy):
  pushed to running nodes, no pod restart, no call disturbed.
- **needs a rollout** — listeners, transports, profile, image, replica counts: turned into a
  *staged plan*, one role and one zone at a time, each stage using the drain machinery below, so a
  config change is never a simultaneous restart of every edge.
- **invalid or incompatible** — rejected at admission with the offending field named; the cluster
  keeps running the last good config and nothing is partially applied.

Between stages the operator gates on health: the `e2e-tester` verdict and DP-3's invariant metrics
must still be good, or the rollout **pauses** — reported in status, no further stages, no
automatic thrashing. A newer config landing mid-rollout supersedes the plan rather than
interleaving with it: the operator re-plans from current observed state, which is what makes
"re-deploy whenever you like" safe rather than merely possible. Convergence is always towards the
CR, so an aborted or paused rollout leaves a cluster that is running, describable and resumable.

*SIP-aware lifecycle* (KO-4) is the part that justifies the operator: rolling updates drain an
edge before terminating it (stop accepting new registrations and calls, let clients re-register
elsewhere, wait a bounded drain window, then terminate); registrar shard changes execute
drain-then-switch on the shard map (DP-1) instead of a silent rehash; token key rotation is
two-phase (distribute-then-activate) so verification never precedes distribution; media nodes are
never reaped while sessions are anchored on them.

**Phase 2 — autoscaling on SIP signals** (KO-5, KO-6).

*Scale on what SIP actually costs.* CPU is a lagging, misleading proxy for SIP load — a
registration storm, a UDP flood and a media-anchored call load look nothing alike in CPU terms.
The scaling signals come from the DP-3 metric set via Prometheus: registrations per shard and
registration write latency, calls per second and in-flight transactions per edge, active dialogs,
media sessions per relay node, per-trunk breaker state, and the overload-control shed rate
(RT-3). Recording rules turn those into per-role capacity ratios; the operator (or an HPA fed by
a custom-metrics adapter — KO-5 decides, KEDA and the Prometheus Adapter are the integration
candidates) reconciles replicas from them.

*Scale-in is the dangerous direction.* Scaling out is additive and safe; removing a replica
touches ownership. Every scale-in goes through the same drain path as a rolling update: an edge
being removed stops accepting and waits for its owned connections to re-register, a registrar
replica hands off its shards before exiting, a media node is only removed at zero anchored
sessions. Hard guardrails: stabilization windows and hysteresis so shedding-induced metric relief
cannot trigger a scale-in during an incident (load sheds → metrics improve → shrinking would
deepen the outage); a floor per zone so no zone can be scaled to zero; and an **invariant gate** —
if DP-3's cross-node dialog-lookup counter is non-zero or probes are failing, the cluster does not
autoscale, because that is a correctness signal, not a capacity signal.

## Alternatives considered

- **A Helm chart with no operator.** Rejected as the end state, kept as the delivery vehicle: the
  chart cannot drain connections, sequence a shard-map change, or two-phase a key rotation. The
  operator ships inside the chart.
- **Plain manifests / Kustomize overlays.** Rejected: no reconciliation loop, no status conditions,
  no place for lifecycle invariants. DP-2's manifests remain the readable reference of what the
  operator produces.
- **HPA on CPU/memory.** Rejected: it neither sees the constraints that bind (connections, shard
  write latency, media sessions) nor reacts in the right direction under shedding.
- **A hosted control plane / GitOps-only workflow.** Complementary rather than alternative — the
  chart and CR are GitOps-friendly by construction — but a running cluster still needs a
  reconciler for in-cluster lifecycle events.
- **Autoscaling in phase 1.** Rejected: scale-in without the drain machinery of KO-4 would break
  connection ownership, so autoscaling is explicitly sequenced after it.

## Risks & open questions

- **Two schemas drifting.** The CR spec must be generated from, or generate, DP-1's config schema.
  If they are hand-maintained in parallel they will diverge; KO-1 decides the single-source
  mechanism.
- **k3s fidelity.** A single-node local cluster cannot demonstrate zone spread, and host
  networking plus wide UDP ranges behave differently there than on managed clusters. The local
  environment is "clustered but co-located" and must say so.
- **Source preservation.** Whether `externalTrafficPolicy: Local`-class routing is sufficient for
  UDP 5060 on the target clusters, or whether edges need host networking unconditionally.
- **rtpengine in managed mode.** Large contiguous UDP port ranges, host networking and kernel
  forwarding behave differently on k3s than on managed clusters; and draining a managed media node
  means waiting for anchored sessions to end, which has no upper bound without a forced cutoff
  policy. KO-7 decides that policy (grace window, then what).
- **Autoscaling stability.** Flapping, storms, and the shed-rate feedback loop; needs explicit
  hysteresis parameters and a way to test them — ideally as harness scenarios rather than only in
  a live cluster.
- **Operator upgrades and CRD versioning**, and where token keys live (Kubernetes Secrets versus an
  external KMS) for deployments with stricter key custody.
- **Scope creep**: the operator must stay a deployment reconciler and not become a runtime control
  plane for calls.

## Acceptance / done

The union of KO-1 … KO-8: `helm install` with a single `values.yaml` stands up a clustered,
self-contained environment on local k3s — media included, no external rtpengine — where two sipx
CLI phones register and call through the platform and the `e2e-tester` probe passes; the same
chart points at an external media pool with only a mode change; the operator reconciles roles,
config, keys and media pool from one `SipxCluster` resource and reports honest status conditions;
re-deploying an edited config at any moment is picked up and rolled out gracefully — hot-reloaded
where DP-1 allows it, staged and drained where it does not, paused when health regresses, rejected
outright when invalid; a rolling update and a replica change drain connections and hand off shards
without dropping registrations; and autoscaling raises and
lowers replicas from Prometheus-derived SIP signals within its guardrails — never scaling in
during shedding, never below the per-zone floor, and never while an invariant metric says the
cluster is wrong rather than busy.

## Validated review remediation (2026-07-30)

`KO-16` keeps the current Helm skeleton honest while `KO-2` remains blocked: chart metadata and
story notes name only the `SipxCluster` custom resource that is rendered, do not claim an installed
operator, CRDs, or RBAC, and name `KO-3` plus the probe work as the path to a runnable cluster.
