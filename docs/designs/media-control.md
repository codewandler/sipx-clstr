# Design: Media control

**Status:** proposed · **Pillar:** Media · **Epic:** `media-control` ·
**Stories:** ME-1 … ME-6 · **Spec:** [media-relay](../specs/media-relay.md) (ME-1 §1–§12, ME-6
§13; normative)

**Upstream considerations** (AGENTS.md rule 6): considered for upstream — **no**. The `MediaRelay`
contract and its NG binding stay in sipx-clstr: the trait's vocabulary is cluster orchestration
(media-node ids, pool membership, tenant call classes) and the NG protocol is one integration
target's control protocol rather than SIP protocol semantics. The one piece that would be
protocol-generic — an SDP model — is deliberately not needed, because the trait carries the body
as opaque bytes end to end ([media-relay](../specs/media-relay.md) §3.2 O3). If `ME-4` ends up
having to *read* the body, that parser is protocol-generic and becomes an
[upstream ledger](../upstream.md) row then. **`ME-6` lands on the same side and for a sharper
reason**: per-trunk codec and SRTP policy is about a *trunk*, an object the kernel does not have,
and its entire output is NG control keys for a relay the kernel never speaks to — so it is
orchestration by construction, and it needs no SDP model either, because the policy is declared
rather than inferred from a body.

## Why

The SIP process controls media over a network protocol; it never touches a media packet.

The vision's fourth principle: media is another cluster. Anchoring RTP in the signalling process
couples two workloads with opposite scaling laws (packets-per-second versus transactions-per-
second), drags kernel-level packet forwarding into a portable Rust service, and recreates years
of media engineering (SRTP, ICE, DTLS, transcoding, recording, in-kernel forwarding) that a
dedicated media relay already provides. The SIP process therefore *controls* media relays over a
network protocol and never touches a media packet. rtpengine is the named first integration
target (per the AGENTS.md carve-out): its NG protocol takes the complete SDP and returns
rewritten SDP, which fits a proxy that treats bodies as opaque until a hook asks otherwise.

## Approach

**The `MediaRelay` trait** is the platform's only view of media: offer, answer, update, delete,
query — SDP in, SDP out, plus session statistics. Implementations planned: `NullMediaRelay`
(pass-through; the default for tests and media-direct deployments — M1 runs entirely on it) and
the rtpengine NG adapter (M2). The trait leaves room for an external gRPC-controlled relay or a
future native relay without the platform noticing.

**The NG adapter** (ME-1 spec, ME-2 implementation) speaks the NG protocol — bencode-framed
request/response over UDP with retransmission, cookie-correlated — against a pool of media
nodes. The integration spec pins: command mapping (offer/answer/delete/query), timeout and
retransmission budget, error taxonomy (node down vs command rejected), and health signals.
**ME-1 is written**: [media-relay](../specs/media-relay.md) fixes the five-method port and the
null relay (§3, §4), the framing, cookie rule and canonical bencode (§6), the command mapping
(§7), the 150 ms/1.5 s retransmission budget (§8), the taxonomy (§9), the node health machine
(§10), the tested baseline `mr13.0.1.10` and its drift rules (§11), and 63 vectors under the `MR`
prefix, with twelve byte-exact datagrams pinned in full (§12). Two of its rulings are worth reading before
ME-2 starts: the port's `update` has **no** command on the wire — it is an `offer` with both tags
— and SDP is opaque bytes end to end, so nothing in either repository parses a body.

**Media policy is declared per trunk, not derived from control flow** (ME-6, spec §13). ME-1 §7.4
listed codec manipulation and SRTP among the things version 1 did not send, and said what adding
one would cost: a story that moves §7.1, §7.2 and §12 together. ME-6 is that story, and what it
adds is not the keys but the *shape of the decision*. A trunk declares a `TrunkMediaPolicy` — the
codecs offered toward it, any transcoding, its SRTP mode — and one pure function
`media_keys(policy, command)` turns that declaration into the four NG keys `transport-protocol`,
`DTLS`, `SDES` and `codec`. The signature is the enforcement: no request, URI, domain, source
address, body, clock or randomness is in scope, so "which branch of the routing logic did this
call take" and "does the callee's domain match a pattern" are not merely discouraged inputs, they
are unreachable ones. Four consequences worth carrying into ME-2 and ME-5:

- **The default is a written-down `Transcode::None`**, and restricting a codec list is explicitly
  *not* a licence to transcode — only a `transcode` declaration adds a format that was not
  offered. A `Restrict` list disjoint from what the peer sent fails the call, which is RFC 3264 §6
  working rather than a fault, and is a great deal better than a transcoding bill nobody declared.
- **The policy in force is the receiving peer's, and it flips between `offer` and `answer`.**
  Inverting that is invisible — it encrypts the wrong half of the call — so §13.2 MP4 has a table
  and two vectors in both directions.
- **SDES demands TLS signalling, checked at startup.** RFC 4568 §7.2 requires the description
  carrying a key be protected; a trunk asking for SDES over UDP refuses to start rather than
  logging a warning about a key it has already sent in clear.
- **Nothing shipped before the interop run.** A misspelled option key degrades to clear-text media
  *silently*, because ignore-unknown-keys binds the far end too, so §13.2 MP12 blocks any policy
  beyond the identity until CF-3 has confirmed each key against the §11 baseline. Spec §11 V5
  records it; MR-C-8 discharges it.

The seam with **EX-7**'s carrier quirk profiles is settled from this side: a quirk may *require*
an SRTP mode and may never *assign* one. If a profile could assign, SRTP selection would go back
to being a consequence of which pattern matched — the defect ME-6 exists to remove, relocated —
and profiles compose, which assignments cannot do without a precedence rule that will surprise
somebody. Constraints intersect instead, and a constraint contradicting a trunk's declaration is a
startup error naming both. EX-7 owns the vocabulary; §13.6 owns only the direction.

**Selection is deterministic.** A media node is chosen once per call by rendezvous hashing over
`tenant || Call-ID || initial From-tag`, and the chosen node id rides in the affinity token
(`cluster-affinity`), so every re-INVITE, UPDATE and BYE — arriving at *any* edge — addresses the
same node with no lookup. Reselection happens only on explicit node failure (ME-3), and the
design is honest about the consequence: moving mid-call media is re-anchoring, not transparent
failover.

**SDP rewrite is a hook.** The proxy pipeline stays body-agnostic; a media-anchoring extension
module registers at the offer/answer-bearing phases (ME-4 decides the exact phases and the
latching stance; ME-5 implements the module). The ICE stance is a per-call-class decision, not
pass-through: an anchored call **cannot** leave ICE untouched — per RFC 8445, endpoints send
media to the nominated candidate pair, not to rewritten `c=`/`m=` addresses, so untouched
candidates let ICE-capable endpoints negotiate around the relay. For anchored calls the module
must have the relay participate in ICE or strip it (capabilities rtpengine provides); untouched
pass-through applies only to media-direct calls, which skip the module entirely — the platform
must never *require* anchoring.

**Deployment posture** (with the `deployment` epic): media nodes are dedicated Linux hosts —
host networking, explicit UDP port ranges, private control interface, separate scaling from
signalling — never general-purpose service pods.

## Alternatives considered

- **Embed an RTP relay in the signalling process.** Rejected by non-goal; also forfeits
  in-kernel forwarding and independent media scaling.
- **Build a native Rust media relay first.** Rejected for v1: the sipx kernel has RTP primitives,
  but SRTP/ICE/DTLS/transcoding/recording at carrier quality is its own multi-year product; the
  trait keeps the option open as `FutureNativeRustRelay`.
- **Per-call random node selection with a session→node map.** Rejected: reintroduces a shared
  lookup on the mid-dialog path — exactly what the affinity token exists to remove.
- **Codec and SRTP policy as a call-class or domain rule** (ME-6's alternative). Rejected: it is
  the shape of the defect the story was filed against. A domain pattern deciding encryption cannot
  be changed for one carrier without touching every carrier that shares the expression, and it
  answers "is this leg encrypted?" only by simulating the match. A trunk field answers it by being
  read. Call class survives for what it is actually about — whether a call is anchored at all, and
  its ICE stance (ME-4) — which is a property of the call rather than of the peer.
- **Opportunistic SRTP, and codec policy conditioned on what the peer offered** (ME-6). Rejected
  for v1, on the same constraint that keeps an SDP parser out of both repositories: both need to
  *read* the description that came back, which spec §3.2 O3 forbids. A trunk needing either is two
  trunks until the story that gives the platform an SDP model exists — and that story is the
  upstream row ME-4 already reserves, not a second one.

## Risks & open questions

- ~~NG protocol version drift and feature detection across rtpengine releases~~ — **answered by
  ME-1**: [media-relay](../specs/media-relay.md) §11 pins the tested baseline `mr13.0.1.10` and
  rules out version detection entirely; compatibility is the unknown-key, spelling-tolerance and
  `supports`/`result` contract, so a newer node needs no adapter change. What remains open is
  operational: CI still needs a containered rtpengine (CF-3), and raising the baseline is a story
  that moves the chart, the container and §12's bytes together.
- Media-node failure semantics: what the platform promises for in-flight calls when a node dies
  (current answer: re-anchor on next offer/answer; no packet-level continuity claim — consistent
  with the vision's HA non-goal).
- Whether relay state persistence/restore on the media node is relied upon at all in v1, or
  treated purely as operator convenience.
- **The sharpest risk ME-6 leaves open: a media-policy key that the node does not recognise is
  ignored, and ignoring an SRTP key means clear text.** The failure is silent by construction and
  the platform cannot detect it from the control channel, because verifying would mean parsing the
  returned SDP (spec §13.2 MP10). The mitigations are structural rather than defensive — spec §11
  V5 marks the spellings unconfirmed, MP12 refuses to start on any policy beyond the identity
  until CF-3 is green, and MR-C-8 asserts against the media rather than against the reply. What
  remains genuinely open is what happens *after* a baseline moves: V4 already re-verifies the
  bytes, but re-verifying that a key is still **honoured** needs the interop run, so raising the
  baseline is not a paperwork exercise for this family.
- **Codec-mismatch observability is poor** (spec §13.4). The platform sees `Rejected` plus a reason
  string it will not interpret, so a call that fails because a `Restrict` list had nothing in
  common with the peer looks like any other rejection. The sanctioned remedy is §9 X3's per-tenant
  mapping of specific reasons to `488`, which is configuration and never a default. Whether that is
  enough in practice is a question for the first deployment that runs a restrictive list.
- Reselection propagation: tokens already minted carry the failed node's id, and the
  [affinity-token](../specs/affinity-token.md) spec rules out mid-dialog token refresh (the
  route set is fixed at dialog establishment, RFC 3261 §12.2). The replacement must therefore
  be computable without a new token — e.g., every edge deterministically re-runs the rendezvous
  selection over the current node-set epoch when the token's node is marked failed. Decided in
  ME-3; until then this is the epic's sharpest open question.

## Acceptance / done

The union of ME-1 … ME-6: the trait with `NullMediaRelay` under harness tests; the NG adapter
verified against a real rtpengine in the interop harness; rendezvous selection with the node id
round-tripping through the affinity token; the media-anchoring module implemented (ME-5) with
media-direct bypass asserted in the harness; per-trunk codec and SRTP policy derived by a pure
function whose byte output is pinned (ME-6, MR-P and MR-C); and the M2 exit criterion — a
cross-edge call in the 3-zone deployment with relayed media surviving re-INVITE from either side.
