# Design: Roles, topology & operations

**Status:** proposed · **Pillar:** Cluster · **Epic:** `deployment` ·
**Stories:** DP-1 … DP-5

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
- Whether a `Record-Route` on a TLS listener should be `sips:` rather than `sip:;transport=tls`.
  The transport parameter (§19.1.1) says "come back over TLS" about this hop and nothing more;
  `sips:` is a claim about the whole remaining path. DP-5 takes the narrower one deliberately and
  leaves the policy to whoever specifies the platform's TLS posture.
- Serving a TLS listener at all: it needs a server identity (certificate and key), which is
  configuration DP-1 owns. Its advertised address is already decided, so the listener is correct
  the moment it can be bound.

## Acceptance / done

The union of DP-1 … DP-5: a node boots into configured roles from a validated config file; a
listener binds one address and advertises another, and what arrives on the bound address is
answerable at the advertised one; the 3-zone reference deployment stands up from the repo's
manifests; the invariant metrics exist and the M2 kill-a-node scenario shows service HA within the
documented bounds; the HA table published in the docs matches what the harness demonstrates.
