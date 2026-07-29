---
title: "Migrating from Asterisk"
description: "An honest concept map, starting with the mismatch — this is a proxy, and it never terminates a dialog."
---

# Migrating from Asterisk

**Lead with the mismatch: this is a proxy-first signalling platform and it never terminates a
dialog, so the things a PBX exists for — queues, IVR, conference, voicemail, a dialplan — are not
in the core and are not scheduled.**

That is not a gap waiting to be filled. It is the decision the rest of the architecture is built
on, and if those features are why you run a PBX today, this does not replace it. Read
[Does this fit?](../guides/does-this-fit.md) for the same answer at greater length.

## What "never terminates a dialog" rules out

A queue, an announcement, an IVR menu and a conference focus all need the same thing: the server
has to be one end of a dialog, hold the media, and usually create a second dialog of its own and
bridge the two. Two dialogs means two offer/answer state machines, `CSeq` translation, leg
correlation, and failure translation per leg.

That machinery is deliberately outside this platform. A design record for it exists and is a
**deferred placeholder with no stories against it** — written down so that the layers underneath
it (hook phases, media control, affinity tokens) are designed with a dialog-terminating consumer
in mind, and deliberately not scheduled, because building services on an unproven platform is how
a platform acquires workarounds it can never remove.

Media is the same boundary seen from the other side. RTP never enters the signalling process — not
today, not later. Audio flows directly between endpoints, or through an external relay this
platform controls over a network protocol. There is nowhere in this architecture to play a prompt
from.

## Maps today / not yet

| In your deployment | Goes to | Status |
|---|---|---|
| Devices registering to the server | The registrar: bindings, address-of-record canonicalisation, a compare-and-swap location store | today |
| Calls between two registered devices | Proxy forwarding under RFC 3261 §16, with forking, `CANCEL`, Timer C | today |
| Audio between those two devices | Nothing to configure — media flows endpoint to endpoint and the platform never sees a packet | today |
| Short extension numbers resolved to users | Declarative number normalisation, bound at fixed points in the request path | specified, not shipped |
| A SIP trunk to a carrier | The trunk model: egress selection, asserted identity, privacy | specified, not shipped |
| Dialplan logic — the per-call routing decision | Typed modules with declared hook phases, dependencies and conflicts | specified, not shipped |
| The dialplan as something you write in a language | Nothing. A routing configuration language is a stated non-goal | not planned |
| RTP through the server: NAT fixup, SRTP, transcoding | An external relay under this platform's control — it receives policy, never packets | specified, not shipped |
| Queues, IVR, conference, voicemail, announcements | An optional B2BUA service built *with* the platform; its design record is a deferred placeholder | not planned |
| An API that originates and controls call legs | The same B2BUA service, and unscheduled with it | not planned |
| The whole thing under Kubernetes | An operator, a Helm chart, drained scale-in, autoscaling on SIP-shaped signals | designed |

Statuses use the site's closed vocabulary: `today` · `today, partly` · `specified, not shipped` ·
`designed` · `not planned`. The `not planned` rows are the honest ones — they are non-goals in the
project's vision, not a backlog nobody has reached.

## What a proxy-first layer is actually for

The shape this platform is designed for is an edge in front of the things that terminate dialogs:
registration, authentication, routing policy, NAT handling and carrier interconnect scale
horizontally at the edge, while a small number of stateful feature servers sit behind it. Splitting
those two jobs is what makes the front half clusterable at all — the edge can hold no call state
precisely because it is never a party to a call.

Be clear about the timing, though: putting this in front of an existing feature server means
routing to a static peer, and that is trunk work. Trunks are specified and not shipped. Today the
node forwards between users registered to it, and nothing else.

## What does not carry over

- **The dialplan.** Not the syntax and not the model. Per-call routing becomes module selection
  and module configuration, that module set is unwritten, and no scripting language is coming.
- **Anything that needs the server in the media path.** Prompts, music on hold, announcements,
  recording, tone generation. The specified relay control surface carries codec and transport
  policy; it deliberately does not carry recording keys.
- **Anything that needs two dialogs.** Queues, IVR, conference, application-originated legs, and
  topology hiding done by terminating the call. Those wait on a service that is not scheduled.
- **Voicemail and the mailbox notifications around it.** No mailbox exists in the core, and none
  is planned for it.
- **Feature parity as a way of evaluating this.** No parity is claimed anywhere on this site.
  Coverage is selected by deployment profile and tracked per normative requirement, with a
  generated report that is allowed to answer "no" — see
  [Conformance](../reference/conformance.md). The comparison that survives contact is
  architectural: whether a signalling layer that holds no call state, and never becomes a party to
  the call, is the layer you want.
