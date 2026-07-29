---
id: DP-9
title: Prove registration and a call across two nodes in a local cluster
pillar: Cluster
status: backlog
priority:
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, k8s, registrar, proxy]
note: the first multi-node proof — register through one node, be called through another
---

# Prove registration and a call across two nodes in a local cluster

## Goal

Stand up more than one node in the local k3d/devspace environment, sharing one location store, and
prove with independent `sipx` CLI phones that a user who registered through one node can be called
through another. This is the first moment the word "cluster" is earned.

## Acceptance

- [ ] The devspace profile runs **at least two** node replicas, each advertising its own pod
      address, against one shared location store.
- [ ] **Failing-first**: a scripted run registers `alice` through node A and places a call to her
      from `bob` via node B, and it fails on the current single-node-per-store arrangement because
      node B cannot resolve a binding it never saw.
- [ ] The call completes with audio, driven by two independent `sipx` CLI phones — the same
      standard of proof `scripts/e2e-call.sh` already meets for one node.
- [ ] An in-dialog `BYE` is routed correctly. Record what makes it work: each node record-routes
      its own advertised address, so the route set names a specific node rather than a service VIP.
      **If a load balancer or a shared VIP is placed in front, say so and say what breaks** —
      that case is what affinity tokens exist for and it is not in this story.
- [ ] Registrations survive losing a node: kill the node that accepted a `REGISTER` and the binding
      is still resolvable through the other. This is the service-HA claim the docs make, tested for
      the first time.
- [ ] The result is a script that exits non-zero on failure, not a manual procedure.
- [ ] The published pages that say clustering does not exist are updated in the same change — the
      site's capability matrix, `clustering/how-it-works`, and `operate/high-availability`.

## Progress

- (running log)

## Notes

- **Blocked by `DP-8` and `RG-12`.** Two nodes with two in-memory stores is not a cluster; it is
  two unrelated proxies that will each answer for whoever happened to register with them.
- Deliberately **not** in scope: affinity tokens, flow ownership, registrar sharding, a load
  balancer in front. Those are the `AF-*` and `RG-5` work. This story is the smallest honest
  multi-node claim, and its value is partly in discovering which of those becomes necessary first.
- Media is expected to flow directly between the phones, as it does today — there is no relay.
- `scripts/e2e-call.sh` is the model for the proof: wait on the `listening on` line rather than
  sleeping, assert on what actually happened, and allow for RFC 3261's 64·T1 absorption window
  before expecting a transaction store to drain.
