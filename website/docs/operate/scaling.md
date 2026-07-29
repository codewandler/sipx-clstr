---
title: "Scaling and autoscaling"
description: "Why CPU-based autoscaling is rejected, which SIP-shaped signals drive scaling instead, and why scale-in is a drain."
---

# Scaling and autoscaling

:::caution Preview
**Designed, not shipped**, and sequenced last on purpose. What runs today is a container and a
single-node k3d/devspace profile — one process, one replica, nothing that scales. See
[Docker and k3d](../guides/docker-and-k3d.md); the chart renders a custom resource that nothing
serves, because there is no operator image and no CRD.

Autoscaling comes deliberately *after* the drain machinery, not beside it. The reason is on this
page.
:::

## CPU is not a capacity signal for SIP

**HPA on CPU or memory is explicitly rejected.** Not deferred, not "good enough for now" —
rejected, because it does not see the constraints that actually bind.

What binds an edge is the **connections it owns**: long-lived TCP, TLS and WebSocket flows that
cannot be moved to another node, plus the transactions in flight on them. What binds a registrar is
**shard ownership** — which addresses-of-record it is responsible for, and how fast it can write
their bindings. What binds a relay node is anchored media sessions. None of those is a quantity of
CPU, and a node can be at its limit on every one of them while its CPU is flat.

They are not even alike as load. A registration storm, a UDP flood and a media-anchored call load
produce three completely different shapes of work, and CPU compresses all three into one number
that resembles none of them.

Then there is the direction problem, which is worse than the resolution problem:

> When the cluster starts shedding load, CPU falls.

Overload control sheds when the platform is past its limit. A CPU-driven scaler reads the resulting
relief as spare capacity and scales **in** — during the incident that the shedding is evidence of,
removing nodes from a cluster that is already turning customers away. The signal improves precisely
because the platform is failing.

And a scale-in is not a subtraction. Removing a replica removes an owner: of connections, of
shards, of anchored sessions. A CPU number has no way to know that any of that is being given up.

## What scaling reads instead

The signals come from the metric set the platform publishes anyway, via Prometheus. Recording rules
turn them into a **per-role capacity ratio** against a declared target — "how full is this role",
not "how busy is this process".

| Role | Signal | Why it is the binding one |
|---|---|---|
| `edge` | Calls per second | The arrival rate the transaction layer has to absorb |
| `edge` | In-flight transactions | What is actually resident; a transaction outlives the request that made it |
| `edge` | Owned connections and active dialogs | The flows this node cannot hand to another one |
| `registrar` | Registrations per shard | Ownership, expressed as work — a shard is the unit that moves |
| `registrar` | Registration write latency per shard | The store's answer to whether the shard is keeping up |
| media pool | Media sessions per relay node | Ports and sessions are the relay's real limit |
| trunks | Per-trunk breaker state | Egress capacity is the carrier's, not this cluster's |
| cluster | Overload shed rate | The one number that says the platform is already past its limit |

Two of these are worth dwelling on. **Registrations per shard** matters rather than registrations
per node, because a registrar replica change moves shards, and a capacity number that cannot be
attributed to a shard cannot be reasoned about across that move. **Shed rate** appears here as an
input to the guardrails below, never as a reason to grow: by the time it is non-zero the decision
has left the capacity domain.

The mechanism that acts on these — replicas reconciled by the operator, or an HPA fed by a
custom-metrics adapter — is not settled yet. What is already decided is the input: SIP-shaped
signals, and never CPU.

## The guardrails, and why they are correctness signals

An autoscaler with good signals and no brakes makes incidents worse. The brakes come in two kinds,
and the distinction is the whole point:

- A **capacity signal** says how full the cluster is. It is an argument for changing the replica
  count.
- A **correctness signal** says the cluster is **wrong rather than busy**. It is never an argument
  for a replica count. It disables the decision.

Four of them, and all four are correctness signals:

| Guardrail | What it means | What it does |
|---|---|---|
| **Overload control is shedding** | The shed rate is non-zero: the platform is refusing work | No scale-in. Metric relief produced by shedding is not spare capacity, and acting on it deepens the outage |
| **A non-zero invariant counter** | The cross-node dialog-lookup counter must read zero. Non-zero means a node went looking for state the architecture says it should never need to look for | Autoscaling is disabled entirely — this is a defect, not a load level |
| **A failing probe** | The synthetic `e2e-tester` call, dialled at the public border like a customer's, is not completing | Autoscaling is disabled. A cluster that cannot complete one call does not get resized |
| **A zone floor** | No zone may be scaled to zero | Scaling below the floor silently converts a three-zone deployment into a smaller one, so the floor holds regardless of what the capacity ratio says. Zones are failure domains; the floor is what keeps them that |

An invariant metric that moves is a bug — that is what "invariant" means here, and it is why these
gates are absolute rather than weighted into a decision. The cluster does not autoscale while one
of them says the platform is wrong.

On top of that sit the ordinary stabilization mechanics: stabilization windows and hysteresis, so a
scaler cannot flap a role in and out on a burst, and so the relief caused by its own last action is
not immediately read as a new reason to act.

## Scale-out is additive. Scale-in touches ownership

Growing is the easy direction, and this architecture makes it easier than most: because the cluster
holds no shared call state, a new node warms nothing. There is no cache to fill and no state to
replicate — what a node needs to route the next request travels in the message
([How the cluster works](../clustering/how-it-works.md)).

The one exception is the registrar, where a replica change in **either** direction moves shard
ownership, and moving a shard is drain-then-switch: the losing node stops accepting for the
migrating shard, finishes what it holds, publishes `drained`, and only then does the gaining node
start — on that signal, or on a bounded and counted deadline. No registration write is ever split
across the two, because a late write from the old owner fails its compare-and-swap against the
address-of-record's revision. See [Registrar shards](../clustering/registrar-shards.md).

Shrinking always touches ownership, per role:

| Role | What scale-in has to do first |
|---|---|
| `edge` | Stop accepting new registrations and calls, let clients re-register elsewhere, wait a bounded drain window, then terminate |
| `registrar` | Hand off its shards — drain-then-switch, as above — before exiting |
| media node | Wait for zero anchored sessions; a relay is never reaped while a call is on it |

## Every scale-in is a drain

This is the sentence the sequencing follows from:

> A scale-in goes through **the same drain path** as a rolling update.

Not a similar path — the same one. Removing a replica because a capacity ratio dropped, and
removing it because a new image is rolling out, are the same operation on ownership, and they run
the same machinery: stop accepting, wait bounded, hand off, terminate.

Which is why autoscaling is phase 2. Autoscaling in phase 1 was considered and rejected outright:
a scale-in without the drain machinery would break connection ownership — it would terminate the
node holding a client's flow with no window for that client to land elsewhere, and it would rehash
registrar shards under in-flight writes. The drain path has to exist and be trusted before
anything is allowed to trigger it automatically. Building them in the other order would
produce an autoscaler whose safe operating range was "do not scale in".

## What this costs you today

Nothing here runs. Sizing a deployment today means sizing it by hand, and the numbers above are the
ones to watch when you do — the same ratios a scaler would eventually read. The two limits that bite
first are not capacity at all:

- A host-networked role's replica count is bounded by the node count, so growing an edge tier means
  adding nodes. See [Deploy](deploy.md).
- Established calls do not survive the loss of their signalling node in v1, so replica counts are a
  statement about how much loss you can absorb, not only about how much load you can carry. See
  [High availability](high-availability.md).

## Where this is defined

- [k8s-deployment-operator](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/k8s-deployment-operator.md) —
  phase 2, the signal set, the guardrails, and why HPA on CPU is rejected.
- [deployment](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/deployment.md) —
  the invariant metrics the guardrails read, and the zones the floor protects.
- [cluster-config](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md) —
  the shard map, and the drain-then-switch state machine a scale-in runs.
