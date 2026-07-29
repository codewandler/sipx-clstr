---
title: "What sipx-clstr is"
description: "A clustered SIP proxy and registrar in Rust that behaves like one correct proxy — with an honest account of what runs today and what is still specification."
slug: /
---

# What sipx-clstr is

**A clustered SIP proxy and registrar.** Many nodes, one observable behaviour. It forwards
requests under RFC 3261 §16, holds registrations in a strongly consistent location service, and
is proved in deterministic simulation rather than on a lucky network.

It is for people who operate telephony infrastructure: multi-tenant edges terminating untrusted
clients, carrier interconnects, and registrars that have to survive losing a node.

## The problem it exists for

Running one SIP proxy gives you RFC 3261 behaviour. Running five introduces failures no RFC
describes:

- a mid-dialog `BYE` arrives at a node that never saw the `INVITE`;
- a re-`REGISTER` lands on a different shard than the one holding the binding;
- a `CANCEL` races the fork it was meant to stop;
- a NAT'd client is reachable only through the one edge holding its connection.

The usual answers — a shared dialog database, a socket registry, a sticky load balancer — all
make the cluster the source of truth, and then the cluster's availability becomes the call's
availability.

**This platform's answer is that the cluster carries no shared call state at all.** What a node
needs in order to route the next request travels in the message itself, signed, in
`Record-Route`, `Route` and `Path`. A node that has never seen a dialog can still route its
`BYE` correctly, because everything required is in the request.

## Where this actually is

One node registers users and proxies calls between them. That is real and it is tested — two
independent phones register, call each other through the node, and hang up, with audio flowing
directly between them. **The cluster is not built yet.**

| | | |
|---|---|---|
| **Proxy** | RFC 3261 §16 forwarding, forking, `CANCEL`, Timer C, loop detection (RFC 5393) | today |
| **Registrar** | `REGISTER`, AoR canonicalisation, bindings, compare-and-swap location store | today |
| **Transports** | UDP and TCP on one listener | today |
| **Media** | Flows directly between endpoints; the platform never touches RTP | today |
| **Configuration** | One cluster-scoped document in YAML, JSON or TOML; refuses to start rather than apply half of it | today |
| **Two nodes, one registrar** | A shared PostgreSQL location service: register through one node, be called through another | today |
| **Digest authentication** | Implemented and vector-proved — the document accepts `tenant[].auth` and nothing applies it | today, partly |
| **One address in front of the cluster** | Needs affinity tokens: each node record-routes its own address, so in-dialog requests must return to it | specified, not shipped |
| **Registrar shards, flow ownership** | Rendezvous hashing, connection ownership, the owner RPC | specified, not shipped |
| **Trunks** | Carrier interconnect, number normalisation, asserted identity and privacy | specified, not shipped |
| **Media control** | External relay over the NG protocol; no RTP in the signalling process, ever | specified, not shipped |
| **Kubernetes** | Operator, Helm chart, drained scale-in, SIP-shaped autoscaling | designed |

## The honest version

Three things will bite you, and none is obvious from the outside.

**It is an open registrar.** Digest authentication is implemented and proved against the RFCs'
own test vectors, but there is no command-line or configuration path that turns it on. The
binary you build today accepts any `REGISTER` for any address-of-record, from anyone who can
reach the port. Do not put it on a public address.

**Registrations are only durable if you ask for them to be.** With `backend: memory` a restart loses
every binding. `backend: postgres` shares them across nodes and survives a restart, and it needs the
`postgres` cargo feature compiled in — the container image has it.

**Two nodes are a cluster; one address in front of two nodes is not.** Each node writes its *own*
address into `Record-Route`, so in-dialog requests come back to the node that forwarded them. Put a
single Service or VIP in front and a `BYE` will land on whichever node the load balancer picks, which
has no idea about the dialog. That is what affinity tokens are for, and they are not implemented.

The first two are configuration; see [Configuration](reference/configuration.md). The third is
architecture, and it is the next thing that has to be built.

## How it is built

The protocol kernel is a separate project, [sipx](https://github.com/codewandler/sipx) — message
parsing, the four RFC 3261 transaction state machines, transports, digest, SDP. This repository
pins it at a tag and adds orchestration only. Protocol logic that belongs in the kernel goes
upstream rather than being reimplemented here.

Inside, decision logic is sans-IO: the proxy and registrar crates are state machines with no
sockets and no clock, which is what makes it possible to run a whole multi-node cluster in
deterministic simulation with virtual time and seeded faults. Correctness claims are measured
against numbered test vectors — see [Conformance](reference/conformance.md).

## Where to go from here

- **[Getting started](getting-started.md)** — a node and a real call, in about five minutes.
- **[Does this fit?](guides/does-this-fit.md)** — the qualification page. Read this before you
  invest anything.
- **[Migrating from Kamailio](migrate/from-kamailio.md)** — the concept map, and what does not
  carry over.
- **[CLI reference](reference/cli.md)** — every flag and exit code.

## Public docs vs project docs

This site is the documentation for people who want to *use* sipx-clstr. The repository also
carries internal contributor material — the
[normative specifications](https://github.com/codewandler/sipx-clstr/tree/main/docs/specs),
[design records](https://github.com/codewandler/sipx-clstr/tree/main/docs/designs), the
[roadmap](https://github.com/codewandler/sipx-clstr/blob/main/docs/roadmap.md). None of it is
published here: it is more detailed, more volatile, and written for people
with the repository open.

The specs are worth reading if you are integrating against this platform — they are normative,
they cite RFCs by section, and they carry byte-level test vector tables. **When this site and the
specs disagree, the code and its tests win.**
