<p align="center">
  <img src="docs/assets/logo.svg" alt="" width="180">
</p>

<h1 align="center">sipx-clstr</h1>

<p align="center">
  <strong>A clustered SIP proxy and registrar that behaves like one correct proxy.</strong><br>
  Many nodes, one observable behaviour — proved in deterministic simulation, not on a lucky network.
</p>

<p align="center">
  <a href="https://codewandler.github.io/sipx-clstr/"><img src="https://img.shields.io/badge/docs-codewandler.github.io%2Fsipx--clstr-E2622A" alt="Documentation"></a>
  <a href="#where-this-actually-is"><img src="https://img.shields.io/badge/status-two%20nodes%20%C2%B7%20one%20registrar-2F3A45" alt="Status: two nodes sharing one registrar"></a>
  <a href="docs/reference/conformance.md"><img src="https://img.shields.io/badge/vectors-157%2F586%20proved-2F3A45" alt="Conformance: 157 of 586 vector rows proved"></a>
  <a href="https://github.com/codewandler/sipx"><img src="https://img.shields.io/badge/built%20on-sipx-F98A3C" alt="Built on sipx"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-2F3A45" alt="MIT or Apache 2.0 license"></a>
</p>

<p align="center">
  <a href="#five-minutes-to-a-forwarded-call"><strong>Quick start</strong></a> ·
  <a href="#the-problem"><strong>The problem</strong></a> ·
  <a href="#how-it-works"><strong>How it works</strong></a> ·
  <a href="#where-this-actually-is"><strong>Status</strong></a> ·
  <a href="https://codewandler.github.io/sipx-clstr/"><strong>Documentation</strong></a>
</p>

---

> **Read this first.** sipx-clstr is **early, and now real**. Two nodes sharing one location service
> are a cluster in the smallest honest sense: a user who registers through one node can be called
> through the other, with audio. That is scripted and repeatable, locally
> ([`two-node-call.sh`](scripts/two-node-call.sh)) and on Kubernetes
> ([`k8s-two-node-call.sh`](scripts/k8s-two-node-call.sh)) — a proof, not a claim.
>
> What does *not* exist yet: affinity tokens, so **one address in front of both nodes will not work**
> — each node record-routes its own address, and in-dialog requests must come back to it. Nor trunks,
> media control, authentication, or the operator. If you need a production clustered proxy today, this
> is not it.

## Five minutes to a forwarded call

You need a [Rust toolchain](https://rustup.rs) and Python 3. No PBX and no account; a node is
configured by a document, in YAML, JSON or TOML.

```sh
git clone https://github.com/codewandler/sipx-clstr && cd sipx-clstr
cargo build --bin sipx-clstr --features postgres

cat > cluster.yaml <<'YAML'
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: local
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:5060
      advertise: 127.0.0.1:5060
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      rpc: 127.0.0.1:7223
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [127.0.0.1]
YAML

./target/debug/sipx-clstr run --config cluster.yaml --node 1 --zone a --roles edge,registrar
```

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

Then, in a second terminal, register two users and place a call between them — raw SIP over UDP,
standard library only:

```sh
python3 scripts/sip_demo.py 127.0.0.1:5060
```

```text
[PASS] REGISTER alice -> 200 (contact echoed: <sip:alice@127.0.0.1:44870>;expires=3600)
[PASS] REGISTER bob   -> 200 (contact echoed: <sip:bob@127.0.0.1:43859>;expires=3600)
[PASS] INVITE reached bob from 127.0.0.1:5060 (the node)
       Via headers stacked: 2 (proxy added its own)
       Record-Route: <sip:127.0.0.1:5060;lr>
[PASS] 200 OK returned to alice -> 200

RESULT: PASS — registrar stored both bindings and the proxy forwarded between them
```

The demo registers `alice@127.0.0.1`, which is why the tenant declares `domains: [127.0.0.1]`: a
`REGISTER` for a domain the tenant does not serve is answered **`403`**. Declare the domain your
phones actually put in their address-of-record, or leave `domains` out to serve any.

> [!WARNING]
> **This is an open registrar.** The tenant above declares no `tenant[].auth`, so anyone who can
> reach the port can register any address-of-record in a served domain. Declaring `auth` does not
> fix that: digest is implemented and vector-proved, but there is no credential store yet, so a node
> whose nonce `secretRef` resolves **refuses to start** and one whose reference does not resolve
> challenges every `REGISTER` into a `401` nobody can answer. There is no configuration today that
> gives you an authenticated registrar — and the digest that *is* there carries a known kernel defect:
> the nonce has nothing per-challenge in it, so two users of one tenant challenged in the same second
> share a nonce and a replay counter, and the second correct password is refused
> ([filed upstream](docs/upstream.md)). With `backend: memory` a restart also forgets every binding.
> Keep it on loopback or a trusted network.

A node is configured by a document — YAML, JSON or TOML — not by flags. Two nodes sharing one
PostgreSQL location service are a cluster: a user who registers through one can be called through the
other. `scripts/two-node-call.sh` proves it locally and `scripts/k8s-two-node-call.sh` in Kubernetes,
both ending in a call with audio.

**→ [Full walkthrough: a node, a cluster, and dialling in to hear it answer](https://codewandler.github.io/sipx-clstr/docs/getting-started)**

## The problem

Run one SIP proxy and its behaviour is RFC 3261's. Run five and something changes that no RFC
describes: a mid-dialog `BYE` arrives at the node that did not see the `INVITE`, a re-`REGISTER`
lands on a different shard, a `CANCEL` races the fork it was meant to stop, and the client behind
NAT is reachable only through the one edge holding its connection.

The usual answers make the cluster the source of truth — a shared dialog database every node reads,
a registry of sockets, sticky load balancing that is one failover away from being wrong. Each works
until the day it is load-bearing.

**sipx-clstr takes the other route: the cluster carries no shared call state at all.**

## How it works

**State rides the message.** Everything a mid-dialog request needs — tenant, registrar shard, media
node, expiry — is packed into a signed opaque token placed in `Record-Route`, `Route` and `Path`.
Any healthy node can route the next request by reading it. There is no dialog database to consult,
so there is none to lose, and the metric that proves it is a counter of cross-node dialog lookups
that must read **zero**.

**Every resource has exactly one owner.** A client's connection belongs to the edge that accepted
it; reaching that client is an RPC to that edge, not a lookup. A registration belongs to one shard
by rendezvous hash, and changing the shard count is a drain-then-switch, never a silent rehash.

**Media is a separate cluster.** The signalling process controls media relays over a network
protocol behind a `MediaRelay` trait. No RTP is ever linked into the process that parses SIP — so
media scales, drains and fails independently of signalling.

**Deterministic before distributed.** Every cluster behaviour must reproduce in a seeded,
virtual-time, multi-node simulation before it touches a socket. Time is fired timers and randomness
is injected, so a failure is a seed you can replay rather than a flake you re-run. A flaky
multi-node test is treated as a bug in the design, not in the test.

```
     carriers / trunks / UAs
              │
   DNS NAPTR/SRV + source-preserving VIP
              │
   ┌──────────┴──────────┐        every edge can serve every request:
   │  edge   edge   edge │◄──┐    the token in the Route header says where
   └──────────┬──────────┘   │    the dialog belongs, so no node has to ask
              │              │
    registrar shards ────────┘    one owner per AoR, by rendezvous hash
              │
      media relays (rtpengine over NG) — a cluster of its own
```

## Where this actually is

| | |
|---|---|
| **Working** | **Two nodes sharing one registrar.** RFC 3261 §16 forwarding with forking; `REGISTER` over a compare-and-swap location store, in memory or in PostgreSQL; configuration by one cluster-scoped document in YAML, JSON or TOML, with the tenant's served domains, binding quota and expiry bounds enforced from it and a bounded number of in-flight transactions answering `503` above it; declared roles gate runtime dispatch; media flowing directly between endpoints. Register through one node, be called through another — proved with independent `sipx` CLI phones, with audio, locally and on Kubernetes |
| **In the engines but not on the wire** | Read this row before trusting the one above. The pure decision cores model more than the driver performs, and the gap is **not** covered by vector rows that prove effect *production*: matched `CANCEL` and Timer C are produced as effects and then discarded by the real driver. Outbound target selection is UDP-only. The affinity-token library and `Record-Route`/`Route` round trip are proved across two simulated edges, but no production key loader or connection-owner delivery hop exists yet. Filed forward as `PX-12`, `RT-12`, `DP-16` and `AF-7` |
| **Written** | **Fifteen specifications** with normative rules and byte-level test-vector tables, covering proxy and registrar behavior, authentication, affinity and membership, configuration, routing policy, media control, probing, optional session service, and the `SipxCluster` resource |
| **Proved by** | The deterministic multi-node harness — seeded, virtual-time, byte-identical on replay — plus a real-socket end-to-end test against independent `sipx` CLI phones, which runs in CI on every push. The [conformance report](docs/reference/conformance.md) is generated, not written: **157 of 586 vector rows proved**, 19 covered for shape only, 410 deferred, each naming a reason and an owner. A row proves what the engine *emits*; where the driver does not perform that effect, the row above says so |
| **Accepted but not applied** | Seventeen sections of the schema — `profile`, `management`, `keys`, `shardMap`, `registrar`, `normalisation`, `trunk`, `domain`, `destinationSet`, `routeRule`, `ingress`, `rateLimit`, `nat`, `mediaPool`, `observability`, `probe`, `echo` — load without error and change nothing, as do a listener's `tls`, `maxConnections` and `connectionLifetime`. The node logs the exact paths at startup, and the security-relevant ones on their own line. `tenant[].auth` has **left** this list: it is applied, and a document this build cannot honour is refused rather than accepted |
| **Not yet** | **One address in front of the cluster** — the Route-token round trip works, but configuration cannot load its keys and a request still cannot reach the edge that owns a client's connection. Also: trunk routing, media control, a user-credential store (so there is no authenticated registrar, only an open one or a refusal), registrar sharding, the operator and chart |
| **Built on** | [sipx](https://github.com/codewandler/sipx) 1.0.0-beta.4 — the SIP kernel this platform orchestrates, pinned to a tag rather than a branch. Protocol logic belongs there; this repo adds clustering |

**Milestones.** M0 foundation on paper *(complete)* → M1 one node that proxies and registers
*(complete)* → **M2 a cluster you can deploy *(next)*** → M3 modern reachability (Outbound, GRUU,
WebSocket, push) → M4 service families. M1 was
[fourteen stories in a fixed order](docs/roadmap.md#m1-in-detail), each with the vectors that
prove it, and all six of its exit criteria hold; the rest, and what "done" means for each, is in
**[the roadmap](docs/roadmap.md)**.

## Why specs first

Because the interesting failures in a SIP cluster are agreement failures, and those are cheapest to
find in prose. Every spec here carries a test-vector table, so the implementation has something to
fail against on day one. The [working agreement](AGENTS.md) makes that a rule rather than an
intention: spec before code, no panics in library code, and protocol fixes go upstream to sipx
rather than being shadowed here.

If that sounds like ceremony, the alternative is the thing this project exists to avoid — a cluster
whose behaviour is whatever the code happened to do, discovered one incident at a time.

## Documentation

There are two documentation trees, and they have different readers.

**📖 [codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/)** — the public
site: what this does, how to run it, and what it deliberately does not do yet.

| | |
|---|---|
| [Getting started](https://codewandler.github.io/sipx-clstr/docs/getting-started) | A node and a forwarded call, from nothing |
| [Does this fit?](https://codewandler.github.io/sipx-clstr/docs/guides/does-this-fit) | The qualification page — read it before building on this |
| [Addressing](https://codewandler.github.io/sipx-clstr/docs/guides/addressing) | Bind versus advertise, the one flag that will bite you |
| [Migrating in](https://codewandler.github.io/sipx-clstr/docs/migrate/from-kamailio) | Concept maps, including what does not carry over |
| [CLI reference](https://codewandler.github.io/sipx-clstr/docs/reference/cli) | Every flag, exit code and output line |

**In the repository** — the internal material, which is *not* published: [vision](docs/vision.md) ·
[architecture](docs/architecture.md) · [specs](docs/specs/) · [designs](docs/designs/) ·
[roadmap](docs/roadmap.md) · [board](docs/stories/README.md). More detailed and more volatile than
the site. When the two disagree, the code and its tests win.

## Contributing

Work is tracked in-repo: every unit of work is a story under `docs/stories/`, and the board is
generated from story frontmatter. Start at **[AGENTS.md](AGENTS.md)** — it is written for coding
agents but it is the same loop a human follows, and it is the fastest way to understand how the
project is put together.

Deployments of this platform live in their own repositories and carry only configuration and
values — never a chart, an image or protocol code. That boundary is a rule, not a convention.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this project by you shall be dual-licensed as above, without any additional terms or
conditions.
