# sipx-clstr — vision & principles

This document states *why* sipx-clstr exists and the principles that decide how it's built. It is
the **tie-breaker** when a design choice is unclear: prefer the option that best serves the north
star and the principles below.

## What sipx-clstr is

sipx-clstr is a clustered SIP platform in Rust — a proxy-first signalling core, a strongly
consistent registrar, and external media control — built on the [sipx](https://github.com/codewandler/sipx) protocol
kernel. It is for people who operate telephony infrastructure: multi-tenant edges terminating
untrusted clients, carrier interconnects, and registrars that survive node loss. The defining idea
is that **the cluster carries no ambient state**: everything a message needs in order to be routed
rides in the message itself as signed tokens, and everything else — registrations, connections,
media sessions — has exactly one owner.

## North star

**A cluster that is indistinguishable from one correct proxy.** Under any node count, adversarial
timing, and partial failure, the SIP behavior endpoints observe is that of a single correct
RFC 3261 proxy and registrar — and this is proved in deterministic multi-node simulation, not
demonstrated on a lucky network.

## Principles

1. **Proxy-first.** Dialogs are end-to-end between endpoints; the platform forwards, forks and
   records the route — it does not terminate calls. A B2BUA is a separate, optional service for
   features that structurally need one (queues, IVR, conference), never the default path. Settles:
   every "just terminate the dialog, it's easier" shortcut.
2. **State rides the message.** Signed opaque tokens in Record-Route, Route, Path and flow tokens
   carry the tenant, shard, media node and expiry a mid-dialog request needs. Settles: the
   recurring shared-dialog-database temptation — the answer is no.
3. **Every resource has one owner.** A client connection belongs to the edge that accepted it;
   reaching it is an RPC to that edge. A registration belongs to one shard by rendezvous hash.
   Settles: "publish the socket in a shared store."
4. **Media is another cluster.** The SIP process controls media relays over a network protocol
   behind a `MediaRelay` trait; a media engine is never linked into the signalling process.
   Settles: scope creep toward an embedded RTP relay.
5. **Extensions are declared, not patched.** A capability is a module with typed hook phases and
   declared dependencies, conflicts and state needs, backed by a machine-readable RFC registry;
   deployment profiles select provably compatible sets. Settles: the per-customer `if` jungle.
6. **Deterministic before distributed.** Every cluster behavior must reproduce in a seeded
   virtual-time simulation before it touches a socket. Settles: whether a flaky multi-node test is
   acceptable — it is a bug.
7. **Upstream first.** Protocol logic lives in sipx; sipx-clstr adds orchestration. Settles: where
   a header, parser or transaction fix lands ([docs/upstream.md](upstream.md)).

## Non-goals

- **An RFC-complete monolith.** Profiles select compatible RFC sets; conformance is tracked per
  normative requirement, and "not implemented" is a recorded status, not a hidden gap.
- **Call-survival HA in v1.** Service HA — new calls and registrations succeed after a node loss —
  is the guarantee. Established calls surviving the loss of their signalling node is an explicit,
  later, opt-in feature; it is never silently promised.
- **Embedded media.** No RTP forwarding in the SIP process, ever.
- **A class-5 feature server in the core.** Queues, IVR and conference live in the optional B2BUA
  service, built *with* the platform, not inside it.
- **A routing DSL.** Routing policy is composed from typed modules, not a config scripting
  language.
