---
id: KO-18
title: Give the devspace nodes an address a phone can dial, not only one it can register through
pillar: Cluster
status: in-progress
priority: 2
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, docs]
note: blocks DX-13 — getting-started §4's caller cannot work in any spelling while the AoR is a Service name
---

# Give the devspace nodes an address a phone can dial, not only one it can register through

## Goal
Make the published k3d walkthrough's call step executable, by giving the deployed nodes an address
that is both a valid `domains` entry and something `sipx dial` can send to.

## Acceptance
- [ ] `website/docs/getting-started.md` §4's caller command runs as written and the call completes.
- [ ] The greeting endpoint's address of record, the tenant's `domains` entry, and the address the
      documented `dial` command targets are the same string, and that string is reachable from
      another pod.
- [ ] `scripts/k8s-two-node-call.sh` passes; it fails today for the same reason.
- [ ] The fix does not reintroduce what `FC-5` closed: a ConfigMap written before any pod exists
      still cannot contain a runtime-assigned address, and a `REGISTER` for a domain the node does not
      serve must still be refused.
- [ ] Whatever is chosen is stated where a reader of the walkthrough meets it, rather than working by
      coincidence.

## Progress
- (not started)

## Notes
- Found by `DX-13` while making the walkthrough executable, and it is what stopped that story at
  `PARTIAL`. **The commands are not stale prose — no spelling of them works.** `sipx dial` takes a
  literal address and port; it does not resolve a name, so `sip:hello@sipx-clstr-node-a` exits on a
  usage error before a packet leaves the pod. Dialling the Service's ClusterIP instead answers
  `480 Temporarily Unavailable`, because the location lookup keys on the whole address of record and
  the stored one is `hello@sipx-clstr-node-a`.
- The name is there for a good reason: `FC-5` moved `domains` and the greeting's AoR from a pod IP to
  the Service name *because* `FC-4` made `domains` enforceable and a ConfigMap written before any pod
  exists cannot hold a runtime IP. So this is a genuine tension between what `REGISTER` needs (a name,
  with an explicit `--target`) and what `dial` needs (a literal destination), not an oversight.
- Candidate directions, both outside `DX-13`'s write set: a static `clusterIP` on the per-node
  Service, used verbatim in `domains` and in the greeting's AoR
  (`deploy/devspace/manifests/node.yaml`); or name resolution in `sipx dial`, which is kernel work and
  would be an [upstream ledger](../upstream.md) row rather than a change here.
- Considered for upstream: **the second direction only.** Giving a deployment an addressable name is
  orchestration and stays here; teaching a UA to resolve a name before dialling is protocol-generic
  and belongs to the kernel if that is the route chosen.
- Related: `guides/docker-and-k3d.md`'s `devspace deploy` path has the same gap, and
  `scripts/k8s-two-node-call.sh` needs the same one-line change.
