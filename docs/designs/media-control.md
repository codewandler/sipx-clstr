# Design: Media control

**Status:** proposed · **Pillar:** Media · **Epic:** `media-control` ·
**Stories:** ME-1 … ME-5

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

## Risks & open questions

- NG protocol version drift and feature detection across rtpengine releases: the integration spec
  pins a tested baseline; CI needs a containered rtpengine (CF-3).
- Media-node failure semantics: what the platform promises for in-flight calls when a node dies
  (current answer: re-anchor on next offer/answer; no packet-level continuity claim — consistent
  with the vision's HA non-goal).
- Whether relay state persistence/restore on the media node is relied upon at all in v1, or
  treated purely as operator convenience.
- Reselection propagation: tokens already minted carry the failed node's id, and the
  [affinity-token](../specs/affinity-token.md) spec rules out mid-dialog token refresh (the
  route set is fixed at dialog establishment, RFC 3261 §12.2). The replacement must therefore
  be computable without a new token — e.g., every edge deterministically re-runs the rendezvous
  selection over the current node-set epoch when the token's node is marked failed. Decided in
  ME-3; until then this is the epic's sharpest open question.

## Acceptance / done

The union of ME-1 … ME-5: the trait with `NullMediaRelay` under harness tests; the NG adapter
verified against a real rtpengine in the interop harness; rendezvous selection with the node id
round-tripping through the affinity token; the media-anchoring module implemented (ME-5) with
media-direct bypass asserted in the harness; and the M2 exit criterion — a cross-edge call in
the 3-zone deployment with relayed media surviving re-INVITE from either side.
