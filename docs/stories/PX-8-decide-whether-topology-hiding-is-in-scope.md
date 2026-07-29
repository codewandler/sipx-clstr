---
id: PX-8
title: Decide whether topology hiding is in scope, and how it survives a node change
pillar: Signalling
status: in-progress
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, topology]
note: a real deployment's current implementation is the defect the cluster design exists to fix
---

# Decide whether topology hiding is in scope, and how it survives a node change

## Goal
Decide whether the platform hides internal topology from peers, and if so specify a mechanism whose state does not pin a dialog to one node.

## Acceptance
- [x] A decision, recorded: in scope or not, with the reasoning.
- [ ] If in scope: the mechanism is specified, and mid-dialog requests are servable by any healthy node — state rides the message or is genuinely shared. — the untaken branch; see the note below.
- [x] If not in scope: the consequence for deployments that rely on it today is written down.
- [x] Interaction with affinity tokens and Record-Route is specified either way.

## Progress
- 2026-07-29 — **decided: out of scope for v1**, written into
  [proxy-engine](../designs/proxy-engine.md) § *PX-8: topology hiding — decided out of scope for
  v1*. Zero Rust; this story's whole deliverable is the record.

  The reasoning, in one line each:
  - **Nothing of ours is on the wire to hide.** A surface-by-surface table shows the platform
    contributes one Via (advertised identity, never a bind address or node name — DP-5) and one
    Record-Route pair (cluster-wide service identity — affinity-token §5), with shard/edge/media
    ids living encrypted in the token body (affinity-token §3/§4). The connection-owner RPC is not
    a SIP hop and pushes no Via.
  - **One Via is a north-star property, not a convenience.** A cluster contributing several Via
    entries is distinguishable from one correct proxy by reading the message. So the decision
    rests on a commitment the platform already has.
  - **The residual demand is RFC 3323 header privacy, and it is owned elsewhere.** §5.1 points it
    at a B2BUA (`services-b2bua`, never the proxy path per vision principle 1); its assumed
    construction — "a pretty significant amount of state on a per-dialog basis", restored on every
    later message — is invariant 5's forbidden hot-path lookup and, keyed by node, is exactly the
    non-interchangeable-replica defect in this story's Notes. What is real about it is per-carrier,
    and belongs to RT-5 (egress header allowlist) and RT-7 (`Privacy`/PAI policy).
- 2026-07-29 — the **untaken branch is recorded rather than skipped**, so a reversal inherits it:
  RFC 3323 §5.1 itself sanctions a stateless alternative ("encrypting and persisting the values in
  the signaling"), and the design specifies it — removed Via entries encrypted under the token key
  family into a `via-extension` parameter of our own Via, returned verbatim by RFC 3261 §8.2.6.2,
  restorable by any node with the cluster key. It is **not built in v1**: it enlarges the message
  it is meant to shrink (RFC 3261 §18.1.1), it cannot be a hook module (hook-framework §3 keeps
  Via engine-internal), and Record-Route privacy has no restorable form in the direction usually
  wanted (RFC 3323 §5.1). The second Acceptance box stays unchecked because that is the branch the
  decision did **not** take — not because the mechanism is missing.
- 2026-07-29 — **considered for upstream: no.** The decision and any future mechanism are
  trust-boundary policy over cluster-owned identities and keys; the one protocol-generic piece
  either would need — the `Headers` surgery API — already landed as sipx `S-15` (PX-3).
- Deferred, explicitly, so it is not silently lost:
  - The single-border-Via property is stated in the design as three assertable clauses but has
    **no normative rows and no vectors**. Adding `PB-` rows to
    [proxy-behavior](../specs/proxy-behavior.md) and wiring them into the vector registry
    (`docs/reference/vector-scope.toml`, `docs/reference/conformance.md`) is a follow-up story,
    not part of PX-8.
  - Whether a deployment may chain `edge` and `outbound-proxy` as two *SIP* elements is
    unsettled and owned by DP-1. If it may, the first element's internal-facing Via crosses the
    border on every egress request and this decision reopens. Filed in the design's risks.

## Notes
- One deployment hides topology today by storing every dialog in a database keyed by the pod's own name, which makes replicas non-interchangeable and forces a 15-minute drain on every rollout. One production region runs a single replica as a result.
- This is the concrete instance of the problem the affinity-token design exists to solve, so the answer here shapes AF.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-3**). The evidence sits in that deployment's own reference material.
