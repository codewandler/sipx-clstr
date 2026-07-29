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
  <a href="docs/roadmap.md"><img src="https://img.shields.io/badge/status-M1%20complete-2F3A45" alt="Status: M1 complete"></a>
  <a href="https://github.com/codewandler/sipx"><img src="https://img.shields.io/badge/built%20on-sipx-F98A3C" alt="Built on sipx"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-2F3A45" alt="MIT or Apache 2.0 license"></a>
</p>

<p align="center">
  <a href="#the-problem"><strong>The problem</strong></a> ·
  <a href="#how-it-works"><strong>How it works</strong></a> ·
  <a href="#where-this-actually-is"><strong>Status</strong></a> ·
  <a href="https://codewandler.github.io/sipx-clstr/"><strong>Documentation</strong></a>
</p>

---

> **Read this first.** sipx-clstr is **early, and now real**. M1 is complete: **one node proxies
> and registers**, and two `sipx` CLI phones register through it and call each other with media
> flowing directly between them — a scripted, repeatable proof, not a claim
> ([`scripts/e2e-call.sh`](scripts/e2e-call.sh)). What does *not* exist yet is the cluster: no
> affinity tokens, no trunks, no media control, no deployment surface. That is M2. If you need a
> clustered proxy today, this is not it — but one node is a foundation rather than a demo, and
> every rule it follows is written down first.

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
| **Written** | Proxy behaviour, location service, affinity token, hook framework, registrar auth — five specs with normative rules and test-vector tables |
| **Working** | One node that proxies and registers: RFC 3261 §16 forwarding with forking, CANCEL and Timer C; REGISTER with server-side digest over an in-memory or PostgreSQL location store; the end-to-end call probe |
| **Proved by** | The deterministic multi-node harness — seeded, virtual-time, byte-identical on replay — plus a real-socket end-to-end test against `sipx` CLI phones. The [conformance report](docs/reference/conformance.md) says which vector rows are proved and which are deferred, with a reason for each |
| **Not yet** | The cluster. Affinity tokens, trunk routing, media control, roles by config, the operator and chart — all M2 and beyond |
| **Known defect** | A retransmitted REGISTER is answered `500` rather than treated as an idempotent retry (`RG-8`, the top of the backlog) |
| **Built on** | [sipx](https://github.com/codewandler/sipx) 0.7.0 — the SIP kernel this platform orchestrates, pinned to a tag. Protocol logic belongs there; this repo adds clustering |

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

**📖 [codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/)** — the vision and
the principles that break ties, the architecture, all four specifications, the epic designs, and the
roadmap.

In the repository: [vision](docs/vision.md) · [architecture](docs/architecture.md) ·
[specs](docs/specs/) · [designs](docs/designs/) · [roadmap](docs/roadmap.md) ·
[board](docs/stories/README.md)

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
