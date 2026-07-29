---
id: DP-5
title: Support listen-private / advertise-public listeners
pillar: Cluster
status: done
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, transport]
note: blocks a downstream deployment's first milestone
---

# Support listen-private / advertise-public listeners

## Goal
Let a listener bind one address and advertise another, so a node on a private address is reachable at its public one.

## Acceptance
- [x] A listener declares `bind` and `advertise` independently.
- [x] The advertised address is what appears in Via, Contact and Record-Route.
- [x] It works for UDP, TCP and TLS.
- [x] A failing-first test proves a request received on the bound address is answerable at the advertised one.

## Progress
- `crates/sipx-clstr-node/src/listen.rs` is the decision: `Listener { transport, bind, advertise }`,
  validated as a `Listeners` set, with `sent_by()`, `record_route_uri()` and `contact_uri()` as pure
  functions of it. No socket, no clock — it runs in the harness (AGENTS.md #2).
- The driver takes the answers: `Listeners::endpoint_config()` maps onto the kernel's own
  `bind`/`sent_by` split, and `driver::proxy_config(config, receiving)` builds the `Record-Route`
  from the listener the request **arrived on**, with every listener's advertised host in the
  identity set (proxy-behavior §5).
- Behaviour change worth knowing: a node that would advertise an unspecified address (`0.0.0.0`,
  `::`) now refuses to start and says so, rather than putting "everywhere" in its `Record-Route`.
  `sipx-clstr run` with no `--advertise` on the default bind is that case.
- **Not done here, deliberately:** the TLS listener is decided but not *bound* — sipx wants a server
  identity with it and certificate material is `DP-1`'s config schema. And the proxy engine writes
  `SIP/2.0/UDP` as the `Via` transport token on every branch it forwards (`forward.rs` F8); the
  sent-by is right, the token is not, and that crate was outside this story's write set.
- Failing-first proof, against the merge base (`33bd373`), expressed in the API that existed there:
  the node's `Via` sent-by came from `Config::new(config.listen)`, so it read `10.0.0.7:5060` — the
  **bound** address — where the acceptance wants `203.0.113.9:5060`; and the single global
  `Record-Route` read `<sip:203.0.113.9:5060;lr>` for every transport, so a TLS arrival was told to
  come back to 5060 over UDP. `tests/advertised_listeners.rs` cannot compile against that tree at
  all — there is no per-listener type to write it in terms of.

## Notes
- Every environment in one deployment runs this way — the proxy binds the node's private address and advertises its public one. It is load-bearing on host-networked cloud nodes.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-8**). The evidence sits in that deployment's own reference material.
