# Design: Roles, topology & operations

**Status:** proposed · **Pillar:** Cluster · **Epic:** `deployment` ·
**Stories:** DP-1 … DP-4

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
- **Service mesh / HTTP ingress in front of signalling.** Rejected: SIP over UDP with Via/rport
  semantics and owned TCP flows is invisible to HTTP-shaped dataplanes; L4 with source
  preservation is the requirement.
- **Multi-region from day one.** Deferred: the design keeps the decision explicit (home region
  per AoR, regional transaction state, global config replication) but v1 ships one region, three
  zones.

## Risks & open questions

- Kubernetes versus plain hosts as the *reference* (k8s manifests are DP-2's deliverable, but
  bare-metal/systemd must remain first-class for media nodes).
- Config reload semantics for the shard map: what happens to in-flight REGISTER writes during a
  resharding — likely "drain then switch," specified in DP-1.
- Whether the L4 VIP or DNS SRV is primary for edge discovery in the reference deployment.

## Acceptance / done

The union of DP-1 … DP-4: a node boots into configured roles from a validated config file; the
3-zone reference deployment stands up from the repo's manifests; the invariant metrics exist and
the M2 kill-a-node scenario shows service HA within the documented bounds; the HA table published
in the docs matches what the harness demonstrates.
