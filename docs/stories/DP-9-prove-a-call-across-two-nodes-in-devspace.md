---
id: DP-9
title: Prove registration and a call across two nodes in a local cluster
pillar: Cluster
status: done
priority:
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, k8s, registrar, proxy]
note: proved twice — two local processes, and two pods in k3d, both with audio
---

# Prove registration and a call across two nodes in a local cluster

## Goal

Stand up more than one node in the local k3d/devspace environment, sharing one location store, and
prove with independent `sipx` CLI phones that a user who registered through one node can be called
through another. This is the first moment the word "cluster" is earned.

## Acceptance

- [x] The devspace profile runs **at least two** node replicas, each advertising its own pod
      address, against one shared location store.
- [x] **Failing-first**: a scripted run registers `alice` through node A and places a call to her
      from `bob` via node B, and it fails on the current single-node-per-store arrangement because
      node B cannot resolve a binding it never saw.
- [x] The call completes with audio, driven by two independent `sipx` CLI phones — the same
      standard of proof `scripts/e2e-call.sh` already meets for one node.
- [x] An in-dialog `BYE` is routed correctly. Record what makes it work: each node record-routes
      its own advertised address, so the route set names a specific node rather than a service VIP.
      **If a load balancer or a shared VIP is placed in front, say so and say what breaks** —
      that case is what affinity tokens exist for and it is not in this story.
- [x] Registrations survive losing a node: kill the node that accepted a `REGISTER` and the binding
      is still resolvable through the other. This is the service-HA claim the docs make, tested for
      the first time.
- [x] The result is a script that exits non-zero on failure, not a manual procedure.
- [ ] The published pages that say clustering does not exist are updated in the same change — the
      site's capability matrix, `clustering/how-it-works`, and `operate/high-availability`.
      **Not done here.** Those pages, the README and the CLI reference all describe the pre-`DP-10`
      surface, and the site deploys from a release tag; they move in one pass with the next cut.

## Progress

- **Done, and proved twice.** `scripts/two-node-call.sh` runs two local processes;
  `scripts/k8s-two-node-call.sh` runs two pods in the k3d cluster. Both end in a completed call with
  audio, and both print what they do *not* prove.
- **What the in-cluster run establishes**, read back from the cluster rather than assumed:
  two pods on distinct IPs; both logging `store="postgres"`, so one location service and not two; one
  ConfigMap document resolved per node through `${POD_IP}`; alice registered through node-a and bob
  through node-b; **2 bindings in one database written by two different pods**; then a call from
  alice, forwarded by node-a — which never saw bob's REGISTER — with `samples_recorded: 24000` and
  `heard_audio: true`. Media went directly between the phones; there is no relay to go through.
- **The store is emptied before the count is taken**, so "2 bindings" is a statement about this run
  rather than about history.
- **The phones run in-cluster, and had to.** This machine routes the pod CIDR `10.42.0.0/16` into a
  WireGuard interface, so packets from the host to a pod are swallowed by the tunnel — the first
  attempt failed exactly there, silently. Pod-to-pod has no such ambiguity, and it is also the
  arrangement a real client is in. The `sipx` CLI is baked into a small image rather than rebuilt with
  this repo's tooling, which would blur the boundary the proof depends on.
- **Two Deployments of one replica, not one of two.** A node's identity comes from outside the
  document and a Deployment cannot give replicas distinct ids; assigning them is the operator's job.
- Environment defects worked through on the way, recorded because they will recur: the k3d node
  carried a stale `disk-pressure` NoSchedule taint while kubelet's own stats reported 12% free — a
  kubelet restart cleared it. And the image had to be rebuilt after every code change; an imported
  image is a snapshot, and `IfNotPresent` will happily run yesterday's binary.
- Considered for upstream: no. This is this platform's own deployment proof.

### What this deliberately does not prove

A single Service in front of both nodes. Each node record-routes its **own pod IP**, so the route set
names a pod and in-dialog requests come back to the node that forwarded. Put a ClusterIP there and
kube-proxy will spread `BYE` across both, which is precisely the case affinity tokens exist for and
they are specified, not implemented. The scripts say this in their own output so a green run cannot be
read as more than it is.

## Notes

- **Blocked by `DP-8`, `RG-12` and `DP-10`.** `DP-8` reads a document and `RG-12` can act on one;
  `DP-10` is what makes a running node do both. Two nodes with two in-memory stores is not a cluster; it is
  two unrelated proxies that will each answer for whoever happened to register with them.
- Deliberately **not** in scope: affinity tokens, flow ownership, registrar sharding, a load
  balancer in front. Those are the `AF-*` and `RG-5` work. This story is the smallest honest
  multi-node claim, and its value is partly in discovering which of those becomes necessary first.
- Media is expected to flow directly between the phones, as it does today — there is no relay.
- `scripts/e2e-call.sh` is the model for the proof: wait on the `listening on` line rather than
  sleeping, assert on what actually happened, and allow for RFC 3261's 64·T1 absorption window
  before expecting a transaction store to drain.
