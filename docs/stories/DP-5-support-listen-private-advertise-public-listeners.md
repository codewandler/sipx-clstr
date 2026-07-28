---
id: DP-5
title: Support listen-private / advertise-public listeners
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, transport]
note: blocks babelforce M1
---

# Support listen-private / advertise-public listeners

## Goal
Let a listener bind one address and advertise another, so a node on a private address is reachable at its public one.

## Acceptance
- [ ] A listener declares `bind` and `advertise` independently.
- [ ] The advertised address is what appears in Via, Contact and Record-Route.
- [ ] It works for UDP, TCP and TLS.
- [ ] A failing-first test proves a request received on the bound address is answerable at the advertised one.

## Progress
- (not started)

## Notes
- Every babelforce environment runs this way — the proxy binds the node's private address and advertises its public one. It is load-bearing on host-networked cloud nodes.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-8** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
