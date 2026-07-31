---
id: AF-3
title: Design the connection-owner RPC
pillar: Cluster
status: in-progress
priority: 2
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity]
note: 
---

# Design the connection-owner RPC

## Goal
Design the one cross-node hop on the signalling path: delivering a request to the edge that owns the target client's connection.

## Acceptance
- [x] Delivery semantics are specified: at-most-once, bounded queueing, and an explicit failure taxonomy (owner unreachable ≠ flow dead ≠ flow rejected — the future `430 Flow Failed` mapping).
- [x] Node-to-node authentication and the transport choice are decided with rationale.
- [ ] The failure taxonomy is exercised as harness scenarios. **Named, not written** — see the
2026-07-30 note below.

## Progress
- 2026-07-30: Written as a **new spec, [owner-rpc](../specs/owner-rpc.md)**, rather than as more
sections of `affinity-token`. That spec's §1 already lists "the owner RPC's transport, node-to-node
authentication and queueing" as out of scope and defers it here, and `cluster-membership` §1 leaves
the same hole from the other side; the two documents own record formats and a configuration
schema respectively, and neither is a home for a hop's behaviour under faults. The design record
carries a **Settled in AF-3** block in the shape AF-2 and AF-6 established.
- 2026-07-30: **The transport decision reverses the design's own guess, and that is the story's
biggest claim.** The sketch said "likely the workspace's own framing over TLS". It is instead
**SIP over mutually authenticated TLS** — the hop is one more SIP hop whose next hop is a *flow*
rather than a URI. Four reasons, in the order they weigh: the payload is a SIP request and the
answer a SIP response, so a second framing carries SIP inside it and puts the kernel's lossless
re-serialization guarantee at risk for nothing; the failure answers already have SIP spellings
(`503` now, `430 Flow Failed` in M3), so a bespoke code space is a mapping table to keep in step;
it adds no dependency and no variant to `proxy-behavior` §2's effect set; and the deterministic
harness executes it as it stands, because a simulated node already exchanges `sipx_sip::Message`.
That last one settled it — a framing the harness cannot carry makes the taxonomy untestable until a
socket exists, which is AGENTS.md rule 2's definition of a wrong design.
- 2026-07-30: **Carriage is RFC 5626 §5.2's own construction** — the reference in the *user part*
of a Route URI, `Route: <sips:REF@HOST:PORT;lr>`. §11.2's canonical text form is base64url and
`-`/`_` are `mark` characters in RFC 3261 §25.1's `user` production, so it is legal verbatim with
no escaping, and the owner pops it with the loose-routing rule `proxy-behavior` §5 P2 already has.
It also makes M3 continuous rather than a replacement: a Path carriage would use the same shape.
- 2026-07-30: **Authentication is mutual TLS, and `keys[]` is deliberately not it** — with the
reason stated rather than the choice asserted. `keys[]` structurally cannot do this job: it is a
*group* secret held byte-identically by every node (CM2), so possession proves "some cluster node"
and never "the node this reference names"; its rotation calendar is derived from record circulation
(`max(L, E_max) + S`) and says nothing about how long a channel should be trusted; and using it as
a pre-shared key makes the record-minting secret an input to a node-to-node key agreement, one
restatement away from the key exchange `cluster-membership` §4 KY9 and §10 forbid. What replaces it
is not a second mechanism either — TLS material is what a `sips` listener already has, named by
reference under `cluster-config` §8 V9 — and it needs **no new configuration field**: the caller's
reference identity is the `rpc` host it dialled (RFC 9525), and the owner accepts a peer whose
certificate identity is the `rpc` host of *some* member, checked against a document every node
already holds. `AU6` records why a TLS handshake is not the key exchange UQ3 forbids: it agrees a
channel key, mints nothing, and adds no row to §8's uniqueness table.
- 2026-07-30: **Peer authentication is not what keeps a request out of a stranger's socket** (AU7).
That is the reference's own `node` field — a reference presented to the wrong node resolves
`FlowDead` at §13.2 RS1 before anything is written. Stated positively because MB6 records that
nothing re-checks the answer came from the endpoint the reference named; RS1 is why that is a
reachability property rather than a safety one.
- 2026-07-30: **At-most-once falls out of the transport plus one rule** (§9). The channel is a
reliable stream, so no transaction timer retransmits across it (UDP between nodes is refused for
exactly that reason); the rule that does the work is that the caller **never retries**, because it
cannot distinguish "it never arrived" from "it arrived, was written, and the answer was lost". The
cost is recorded rather than argued away in `AM5`: under a failure this hop *loses* requests, and a
delivered request may be reported undelivered and answered `480` while the device rings. Chosen
over a retry that rings a device twice.
- 2026-07-30: **Bounded queueing means the caller keeps no queue at all** (§8). The only queue is
the owner's, per flow, bounded by `max_pending_per_flow` (8). Inbound admission reuses the node's
existing `admission.maxInFlightTransactions` rather than adding a second overload knob — only the
*answer* is specific (`FlowRejected`, so the caller's taxonomy stays closed). Two timers: `T_write`
2 s at the owner, `T_owner` 4 s at the caller, plus `T_peer_retry` 1 s so a dead peer does not turn
every call attempt into a dial. `T_owner` bounds **delivery, not the call**, which needed a positive
delivery signal to exist — `100 Trying` on a completed write, for every method (RFC 4320 §4.1).
Without it a device that takes five seconds to ring times the caller out, the target is removed, the
next contact is tried, and the first device is still ringing.
- 2026-07-30: **The taxonomy is carried, not re-invented.** AF-2 §13.2 fixed four outcomes and what
each costs; this spec fixes only how each is produced and observed, and adds no fifth (RP4, AN9).
Two mechanisms: `100 Trying` means the bytes are in the socket, and an RFC 3326 `Reason` marker
means "the owner is speaking about delivery". The marker rather than the status code is the
discriminator, because M2's dead-flow answer is `480` and a client may legitimately send `480`
itself. `430` is mapped now so it is not re-litigated in M3 (AN8): the owner's answer becomes `430`,
the marker becomes redundant for that cause, the caller-side consequence is unchanged, and `430` is
never forwarded upstream.
- 2026-07-30: **One rule elsewhere is narrowed and it is named rather than absorbed** (AN6).
`proxy-behavior` R10 makes a branch transport error behave as `503` from that branch; on a
peer-channel branch it is `OwnerUnreachable` and the target is removed, because AF-2 already fixed
that consequence and a caller that could not reach the owner has learned nothing about the flow. A
`500` upstream where a `480` belongs is what the harness row is written to catch. §12 lists this and
six other consequences for documents this story does not own — none performed here.
- 2026-07-30: **`MB5` is an over-approximation and the precise rule is not expressible where it
lives.** The property that needs an endpoint is "this node may own a flow", which by FM6 is "this
node accepts a connection-oriented transport" — a *listener* fact, so a UDP-only proxy declares an
endpoint nothing will dial. The document cannot check it: cluster-scoped, carries no member's
listener set (CM2, MB7), and deriving identity from it is forbidden (`cluster-config` §5 P1). The
same rule *is* checkable at the node that knows its own listeners — a connection-oriented listener
with no `rpc` in its own member entry refuses to start, MB2's cross-check shape applied to a field
MB2 does not cover. Recorded in §12 and in the design's open questions; `cluster-membership` was
not edited.
- 2026-07-30: **Flagged, not decided — mid-dialog reachability.** The hop serves only a request
whose target came from a lookup carrying `flow_ref` (D1); a mid-dialog request has no lookup (T1),
so it carries no reference and cannot use the hop. RFC 5626's answer is to carry the reference in
the route set, and §11.4's caveat is why AF-3 could not just do it: a route set is never recomputed
(RFC 3261 §12.2.1.2), so a reference in one is refreshed by nothing and `E_max` stops bounding key
rotation. M3 must re-bound rotation or give the reference an expiry first. This bounds what AF-7's
M2 assertion can honestly claim, and it is now the epic's largest open question.
- 2026-07-30: **Third acceptance item deliberately not ticked — PARTIAL.** "The failure taxonomy is
exercised as harness scenarios" needs an implementation to exercise, and that is `AF-7`'s
("Implement connection ownership and the owner RPC"), whose own acceptance already reads "…surfaces
as distinct outcomes in a harness scenario". Everything under `crates/` was out of scope for this
story, so no scenario could be written or run. What is delivered instead is §10: **thirteen named
scenarios** with their fault schedules and the claim each fixes, named as test functions so a
missing one is missing by name. The spec registers **no vector prefix**, on `cluster-membership`
§11's precedent and for the same reason `CF-8`/`EX-12` paid for — a row and the test that executes
it arrive in the same commit, and a row written first lands in the deferral ledger with a story
attached.
- 2026-07-30: Zero Rust changed. Full gate green.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
