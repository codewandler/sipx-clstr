---
title: "High availability"
description: "Exactly what survives losing a node — and the guarantee this platform deliberately does not make."
---

# High availability

:::caution Preview
This page describes the guarantee the cluster is designed to make. **The cluster does not exist
yet.** One node runs today, so today there is no high availability of any kind — read
"Today there is none" below before you read the rest as a promise.
:::

## The guarantee, in one sentence

**Service HA: new calls and registrations succeed after a node loss.**

That is the whole claim. It is stated that narrowly on purpose, because the claim a reader wants
to hear is a wider one, and this platform does not make it.

| | |
|---|---|
| A node is lost; a phone registers again | **succeeds** — that is the guarantee |
| A node is lost; a new call is placed, inbound or outbound | **succeeds** — that is the guarantee |
| A node is lost; a call already in progress through it continues | **not guaranteed**, and not in v1 |

Established calls surviving the loss of their signalling node is an **explicit, later, opt-in
feature**. The project's vision says it plainly, and the wording is deliberate: *it is never
silently promised*. If a page, a chart value or a release note ever appears to offer call survival
without you having switched something on and read what it costs, that page is wrong.

## Why the line falls exactly there

Two facts about the architecture decide it, and they pull in opposite directions.

**The cluster holds no shared call state.** What a node needs in order to route the next request
travels in the message itself, signed, in `Record-Route`, `Route` and `Path`. A node that has never
seen a dialog can route its `BYE` correctly, because everything required is in the request. That is
[how the cluster works](../clustering/how-it-works.md), and it is the reason service HA is
achievable at all: **the routing information survives a node loss because it was never on that node
to begin with.** There is no dialog database to fail over, no cache to warm on the replacement, and
nothing to hand over when a node is drained.

**A connection has exactly one owner, and ownership cannot be handed over.** A TCP, TLS or
WebSocket connection belongs to the edge that accepted it. A registration binding stores an opaque
flow reference — signed node, incarnation, connection, generation, transport — and never a socket,
because a socket is not a thing another process can be given. Reaching that client means an RPC to
the owning edge, which writes to the connection it holds.

So when a node dies, its connections die with it. Not because the design forgot about them: because
the file descriptor was in that process, and no amount of shared state would have made another
machine able to write to it. The signed reference is built so that this is *detectable* rather than
silent — the generation counter is bumped on reconnect, the incarnation stops a restarted node from
re-issuing an identity a live reference already names, and a reference resolves to the connection it
was minted for or to nothing at all. Detectable is not the same as survivable, and the design does
not pretend otherwise.

That is the whole asymmetry:

- Routing information was never on the node, so losing the node does not lose it. → **new calls and
  registrations work.**
- Connections *were* on the node, and cannot be moved. → **calls in progress across it do not
  survive.**

## What recovery looks like

The pieces of the designed answer, none of which are shipped:

| Failure | What the endpoint observes | What recovers it |
|---|---|---|
| An edge is lost | Its connections drop; requests towards those bindings find a flow reference that resolves to nothing | Clients re-register — through DNS or the VIP — and land on another edge. Callers use another registered flow, or treat the binding as temporarily unavailable |
| A proxy instance is lost | An in-flight transaction on that instance fails; mid-dialog requests are unaffected, because they carry their own routing | A retry reaches another instance, which can route the request from the token inside it |
| A registrar shard's owner is lost | Registration writes for that shard stall | The shard moves. A late write from the old owner fails its compare-and-swap against the per-address revision, so a handoff can stall but cannot corrupt |
| A trunk destination is lost | Egress attempts fail | The circuit breaker opens and routing selects another destination in the set |
| A media node is lost | Audio stops on the calls it was relaying | Nothing, for those calls. Media survival is a separate question from signalling survival, and is not promised either |

The bounds are the part that is missing, and they are missing honestly. A table like this is only
worth reading if every row has a number and a test behind it, so the published version — what
breaks, what the endpoint sees, what recovers, and **in what time bound** — owes a harness scenario
per row, so that the table and the tests cannot drift. Until that exists, treat the column above as
a description of the mechanism and not as a service-level objective.

## Today there is none

Two nodes can share one registrar, and that is not the same thing as surviving the loss of one.

What the shared PostgreSQL location store does buy is **registration survival**: bindings live
outside both processes, so a node that dies takes no registration with it, and a node that restarts
comes back to the same set. With `backend: memory` none of that holds — a restart is
indistinguishable from a node loss and every phone is unreachable until it re-registers.

What it does not buy is anything else on the table above. Calls in progress on the lost node are
gone, its connections are gone, and — because nothing mints an affinity token — **callers have to be
pointed at a specific node**, so losing one is visible to whoever was addressing it. You cannot put
one address in front of the two and let it fail over: in-dialog requests must return to the node that
forwarded them, and a balancer will send them elsewhere. That is the gap between "two nodes share a
registrar" and "node loss is survivable", and it is affinity tokens, flow ownership and drain.

If you need availability today, you need it from something in front of this, and that something
cannot route mid-dialog requests correctly yet.

## Reading a chart or a values file

The configuration schema, the Helm chart and the Kubernetes operator all describe multi-node
deployments. They are designs, and a replica count of three in a values file is not evidence that
node loss is survivable — it is evidence that a chart can render three of something.

The properties that would make three nodes an actual cluster — source-preserving L4 routing for
UDP, long-lived flows pinned to their owning edge, disruption budgets with graceful connection
draining, one replica of a role per node — are named in the deployment design, and they are
prerequisites for the guarantee at the top of this page rather than consequences of the replica
count.

## Where the rules live

| Document | What it fixes |
|---|---|
| [vision](https://github.com/codewandler/sipx-clstr/blob/main/docs/vision.md) | The non-goal in its original words: service HA is the guarantee, call survival is a later opt-in and is never silently promised |
| [deployment](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/deployment.md) | The HA statement, the failure-mode table it owes, and the reference topology the guarantee assumes |
| [cluster-affinity](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/cluster-affinity.md) | Flow ownership: why a connection has exactly one owner, and what a reference to a dead one does |
| [affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md) | Normative: the token and the flow reference, including the incarnation and generation fields this page leans on |
| [location-service](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md) | Normative: shard ownership and the compare-and-swap that makes a stalled handoff safe |
| [roadmap](https://github.com/codewandler/sipx-clstr/blob/main/docs/roadmap.md) | M2's exit criterion — killing any single node leaves new calls and registrations working |

## Where to go next

- [Observability](observability.md) — the invariant metrics and the outside-in probe that turn the
  claim on this page into something you can check continuously.
- [Does this fit?](../guides/does-this-fit.md) — the qualification page, with this guarantee stated
  from the other direction.
- [How the cluster works](../clustering/how-it-works.md) — the no-shared-state idea this whole page
  is a consequence of.
