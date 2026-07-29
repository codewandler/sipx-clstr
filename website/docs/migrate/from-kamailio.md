---
title: "Migrating from Kamailio"
description: "An honest concept map: what moves across today, what is still specification, and what is different by design."
---

# Migrating from Kamailio

**The role is the same — this is a SIP proxy and registrar, so it replaces one — but the floor
today is a single node, and the reason to look at it is architectural rather than featural.**

You already run a proxy. This page is the concept map: which of the things in front of you have a
home here, which of those homes are built, and which of your assumptions this platform declines to
carry. Read [Does this fit?](../guides/does-this-fit.md) first if you have not; it is the
qualification page and it is blunter than this one.

## The floor, stated before the map

Nothing below is hedged, so start here.

- **One node.** More than one node cooperating is specified and normative, and not shipped. There
  is no clustering in the binary you can build today.
- **No trunks.** There is no carrier egress: a call routes between users registered to this node,
  or it does not route.
- **An open registrar.** Digest authentication is implemented and proved against the RFCs' own
  test vectors, and no command-line or configuration path turns it on. The binary accepts any
  `REGISTER` for any address-of-record from anyone who can reach the port.
- **Bindings live in memory.** A restart loses every registration, and there is no second node
  that kept them.
- **UDP and TCP, one listener.** No TLS listener, no WebSocket listener.

That is a small floor and it is the true one. The rest of this page is what the map looks like
anyway.

## Maps today / not yet

| In your deployment | Goes to | Status |
|---|---|---|
| Request forwarding, stateful and stateless | The proxy core: RFC 3261 §16 forwarding, forking, `CANCEL`, Timer C, loop detection (RFC 5393) | today |
| A registrar and its location table | The registrar: `REGISTER`, address-of-record canonicalisation, bindings, a compare-and-swap location store | today |
| That location table kept in a database | A PostgreSQL location store — real code with real tests, behind a cargo feature the shipped binary does not reach | today, partly |
| A digest challenge on `REGISTER` | The registrar-auth rules, vector-proved, with no flag that switches them on | today, partly |
| UDP and TCP listeners | One listener carrying both | today |
| The request-routing script | Typed modules with declared hook phases, dependencies and conflicts; a deployment profile selects a provably compatible set | specified, not shipped |
| Per-customer branches inside that script | Nothing. Routing policy is composed from modules, and a routing configuration language is a stated non-goal | not planned |
| A gateway list with failover and distribution | The trunk model: carrier interconnect, egress selection, asserted identity, privacy | specified, not shipped |
| Number rewriting on the way out | Declarative number normalisation, fenced to exactly two points in the request path and to the number position alone | specified, not shipped |
| A state store every node reads on the hot path | Nothing, deliberately — routing state rides in the message instead. This is the section below | specified, not shipped |
| Per-edge tracking of NAT'd client connections | Flow references: a signed reference naming both the flow and the node that owns it, plus an RPC to that owner | specified, not shipped |
| Registrations spread across nodes | Registrar shards by rendezvous hash, compare-and-swap per binding | specified, not shipped |
| An RTP relay the proxy controls | An external relay driven over a network control protocol; RTP never enters the signalling process | specified, not shipped |
| The whole thing under Kubernetes | An operator, a Helm chart, drained scale-in, autoscaling on SIP-shaped signals | designed |
| Queues, IVR and conference alongside the proxy | An optional B2BUA service built *with* the platform rather than inside it | not planned |

Statuses use the site's closed vocabulary: `today` · `today, partly` · `specified, not shipped` ·
`designed` · `not planned`. "Specified, not shipped" means a normative document with numbered
rules and byte-level test vectors exists and no code implements it — not that a direction has been
sketched.

## The argument, which is architectural

A feature-by-feature comparison is not the reason to move, and this page will not offer one. The
difference that decides everything else is where the information needed to route the *next*
request lives.

Running one proxy gives you RFC 3261 behaviour. Running five introduces failures no RFC describes:
a mid-dialog `BYE` arrives at a node that never saw the `INVITE`; a re-`REGISTER` lands somewhere
that does not hold the binding; a `CANCEL` races the fork it was meant to stop; a NAT'd client is
reachable only through the one edge holding its connection. Any answer that puts the answer in a
store the cluster shares makes that store's availability the call's availability.

**This platform carries no shared call state.** When a node forwards a dialog-forming request it
records itself in `Record-Route` (RFC 3261 §16.6 step 4), and the URI it records carries a signed,
opaque token holding the tenant, shard, media node, policy version and expiry. Endpoints echo the
route set back on every in-dialog request — RFC 3261 §12.1.1 and §12.1.2 require exactly that, and
§12.2.1.2 fixes that a target refresh does not recompute it — so a node that has never seen the
dialog routes its `BYE` correctly from the request alone. The one resource that cannot ride in a
message is a client's connection: that gets a flow reference naming its owning node, on the
unforgeable construction RFC 5626 §5.2 requires of a flow token.

The trade is stated rather than hidden. You pay a signature verification on every in-dialog
request. You get no cross-node lookup on the signalling hot path, a node that can be lost without
losing routing information that was never on it, and a node that can be added without a cache to
warm. Scale-in becomes a drain rather than a migration, because nothing has to be handed over.

What it does not buy is call survival. A client's TCP connection dies with the node holding it,
and established calls surviving the loss of their signalling node is an explicit later opt-in,
never a silent promise.

The normative version is the
[affinity token specification](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md);
the principle it comes from is the second in
[the project's vision](https://github.com/codewandler/sipx-clstr/blob/main/docs/vision.md). The
readable version is [How the cluster works](../clustering/how-it-works.md).

## What does not carry over

- **The routing script.** There is no language to port it into, and there will not be one.
  Routing policy is composed from typed modules with declared dependencies, conflicts and state
  needs; a configuration scripting language is a stated non-goal rather than a missing feature.
  Whatever your script encodes has to be re-expressed as module selection and module
  configuration, and most of that module set is unwritten.
- **A module-for-module mapping.** None is claimed anywhere on this site. RFC coverage is selected
  by deployment profile and tracked per normative requirement, and the generated conformance
  report is allowed to answer "no" — see [Conformance](../reference/conformance.md).
- **The location table itself.** Bindings here are shaped by this project's own location-service
  rules, with compare-and-swap per binding rather than last-write-wins. There is no import path
  and no migration tool; bringing an existing credential store across is a known requirement with
  work filed against it and nothing built.
- **Anything that puts the signalling process in the media path.** RTP never enters it — not
  today, not later. Media flows endpoint to endpoint, or through a relay this platform controls
  over a network protocol.
- **Any design that needs a shared lookup on the hot path.** This is the one assumption the
  platform will not trade, and a request to reintroduce it is a request for a different system.
