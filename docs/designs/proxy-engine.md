# Design: Proxy engine

**Status:** proposed · **Pillar:** Signalling · **Epic:** `proxy-engine` ·
**Stories:** PX-1 … PX-8

## Why

The forwarding layer the whole platform stands on: RFC 3261 §16 as a sans-IO engine.

The platform is proxy-first: dialogs stay end-to-end between endpoints, and the cluster forwards.
That keeps the state the cluster must hold — and therefore replicate — to a minimum, and it is
what makes the affinity-token model possible at all. The sipx kernel supplies everything below
this layer (lossless messages with byte-exact passthrough, all four RFC 3261 transaction machines
amended by RFC 6026, transports, resolution) but contains **no forwarding path**: no Via push/pop,
no `Max-Forwards` decrement, no Record-Route insertion, no forking, no response aggregation, no
CANCEL propagation. This epic builds that layer — RFC 3261 §16, amended by RFC 5393 for loop
detection — as a sans-IO engine over the kernel.

## Approach

**Two modes, one engine.** Transaction-stateful proxying (§16.2) is the primary mode: one server
transaction, N client transactions, a response context selecting the best response (§16.7).
Stateless forwarding (§16.11) is a strict subset used where per-message cost matters (e.g.
in-dialog requests already carrying a valid affinity token); anything that forks, recurses, or
must generate its own responses is stateful by definition.

**The engine is pure.** Following the kernel's sans-IO discipline, the proxy core is a state
machine: messages, fired timers and verified-token facts go in; effects come out (send on branch,
create client transaction, cancel branch, respond upstream, set/clear timer). Sockets, clocks and
the location service live in the driver. This is what makes the §16 rules testable under the
deterministic harness (`conformance-harness`) with vectors from the spec (`PX-1`).

**Request processing pipeline** (each step a typed hook phase — see `extension-framework`):
validate (§16.3, including the `Max-Forwards` check and RFC 5393 loop/spiral detection via branch
parameter reuse rather than Via counting alone) → preprocess routing information (§16.4: strip
own Route, handle strict-router predecessors) → target determination (location service, trunk
routing, or the Request-URI) → forward to each target (§16.6: copy, decrement `Max-Forwards`,
push Via with a new branch, optionally insert Record-Route carrying the affinity token, apply
Route) → response processing (§16.7: pop own Via, aggregate, pick best final response, forward
provisionals) → CANCEL propagation on a better final response or upstream CANCEL, with `487`
generation for branches cancelled before a final response. Timer C guards INVITE branches that
stop receiving provisionals.

**The proxy driver lives here, not in sipx** — but it is built on the kernel's endpoint, not
beside it. This paragraph originally argued that `sipx_transport`'s API was "UA-shaped: one
transaction, one target" and that a forking proxy therefore needed its own socket loop over
`sipx_sip::transaction::TransactionLayer`. `PX-2` read the released kernel rather than
remembering it and found that wrong in the way that matters: `Handle::send` creates one client
transaction per call, N calls fan out concurrently over one `TransactionLayer`, and `send`
inserts a `Via` only when one is absent — so a proxy that pushes its own keeps control of the
branch, and the branch it chooses is the transaction key. **Decision: build on
`sipx_transport::Handle`**, and file the two things a proxy needs that a UA does not (unmatched
responses surfaced, requests not dropped silently) as kernel changes. Full rationale, the effect
table and the backpressure analysis: [proxy-transaction-driver](proxy-transaction-driver.md).

Only generic primitives are upstreamed — the `Headers` surgery API (`remove_first`, `insert_at`,
`retain`) needed for Via pop/push and Record-Route insertion is a sipx change
([upstream ledger](../upstream.md), story PX-3).

**What the proxy never does:** own dialog state, rewrite bodies (that is `media-control`'s hook),
or terminate calls. A dialog-terminating feature belongs in `services-b2bua`.

### PX-8: topology hiding — decided out of scope for v1

**Decision: the platform ships no topology-hiding mechanism in v1.** Not deferred for cost. What
such a mechanism conceals is already absent from the wire here, by decisions this platform has
made for other reasons; and the residual demand it usually answers is a per-trunk privacy
obligation that belongs to `routing-trunks`, not a cluster feature.

**What a peer actually sees of this platform.** Take any request the platform forwards across its
border — to a carrier, to an untrusted client — and enumerate every place internal structure could
surface:

| Surface | What the peer sees | Why |
|---|---|---|
| The Via we push (§16.6 step 8, F8) | one entry, carrying the listener's **advertised** identity — never its bound address, never a node name | DP-5 (bind ≠ advertise); one entry because the cluster is one SIP hop, below |
| The Record-Route pair we insert (F4) | two entries, both naming a **cluster-wide service identity** that any edge recognises and pops | [affinity-token](../specs/affinity-token.md) §5 |
| The routing state those entries carry | ciphertext — home shard, edge affinity and media node are confidential body fields, encrypted by default | affinity-token §3/§4 |
| The hop toward a connection-bound client | nothing: the connection-owner RPC is not a SIP hop and pushes no Via | [cluster-affinity](cluster-affinity.md) |
| Media addresses in SDP | the relay's, wherever media is anchored | [media-control](media-control.md), ME-4/ME-5 |
| Via entries below ours, and Contact | the calling endpoint's own address — the *subscriber's* topology, not the platform's | RT-5 / RT-7, below |

Read down the middle column: **there is no internal topology of ours on the wire to hide.** The
one row that leaks an address leaks the *caller's*, which is a different problem with a different
owner.

**Why one Via and not several.** The north star is a cluster indistinguishable from one correct
proxy, and one correct proxy contributes exactly one Via and one Record-Route position per
traversal. A cluster that contributed several would be distinguishable from a single proxy by
reading the message — a north-star violation before it is ever a topology leak. So the property
that makes hiding unnecessary is one the platform is committed to for an independent reason,
which is what makes this decision cheap to hold. The one deployment shape that would break the
property is named under *Risks & open questions*, not assumed away.

**The obligation the decision creates.** Because the argument rests on a property rather than a
mechanism, the property is stated so it can be asserted rather than believed. For a request
leaving the platform toward an external peer:

1. exactly one Via entry is attributable to the platform;
2. every host in that entry, and in every Record-Route entry the platform inserted, is a
   configured **advertised** identity;
3. no bind address, node name, shard id, edge id or media-node id appears in cleartext anywhere
   in the message — those live in the token body only.

**The residual demand, and why it is not this.** What deployments usually mean by topology hiding
is RFC 3323 header privacy: the `header` priv-value asks a privacy service to obscure "those
headers which cannot be completely expunged of identifying information without the assistance of
intermediaries (such as Via and Contact)" (§4.2). It is about the **originator's** exposure, not
the intermediary's, and v1 declines it for three reasons:

- **RFC 3323 §5.1 points it at a B2BUA**: "In order to provide these functions the privacy service
  must frequently act as a transparent back-to-back user agent (B2BUA)." Vision principle 1 makes
  the B2BUA an optional service (`services-b2bua`), never the proxy path.
- **Its assumed construction is the defect this platform exists to remove.** §5.1 assumes "the
  privacy service has locally persisted the values of any of the above headers that are so
  removed, which requires the privacy service to keep a pretty significant amount of state on a
  per-dialog basis", and that on every further request or response of the dialog it "MUST restore
  values for the Via, Record-Route/Route or Contact headers that it has previously removed". That
  is a per-dialog store on the mid-dialog hot path: wrong by invariant 5 before it is slow. Key
  that store by the node that minted the entry and replicas stop being interchangeable — only the
  node holding a dialog's removed headers can serve that dialog's mid-dialog requests, so rollouts
  need a drain window sized to the longest call. That failure mode is the reason the affinity
  token exists.
- **The part that is real is already owned, at a finer grain.** Which headers a given carrier
  receives is a per-carrier compliance obligation, not a cluster property:
  [RT-5](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/RT-5-implement-per-trunk-egress-header-allowlist.md)
  owns the per-trunk egress header allowlist and
  [RT-7](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/RT-7-specify-per-trunk-asserted-identity-and-privacy.md)
  owns `Privacy`/P-Asserted-Identity policy including anonymous callers. This decision does not
  pre-empt either.

**The mechanism, if it is ever needed.** Recorded so the next reader inherits it instead of
re-deriving it — and because writing it down is what makes "out of scope" a decision rather than
an omission. RFC 3323 §5.1 sanctions the stateless alternative in its own words: "There may be
alternative ways (outside the scope of this document) to perform this function that do not require
keeping state in the privacy service (usually means that involve encrypting and persisting the
values in the signaling somehow)." That is state riding the message, and the construction is
forced: the stripped Via entries are encrypted under the token key family into a `via-extension`
parameter (RFC 3261 §20.42: `via-extension = generic-param`) of the platform's **own** Via, and
RFC 3261 §8.2.6.2 requires that "the Via header field values in the response MUST equal the Via
header field values in the request and MUST maintain the same ordering" — so the parameter returns
verbatim, and any node restores the stack with the cluster-wide key. Zero stored state, no node
affinity, and it degrades exactly like a token: unverifiable in, `403` out.

It is not built in v1, for reasons that will not change with the schedule:

- **Hiding by displacement costs more bytes than it removes.** `N` stripped bytes come back as an
  authenticated, base64url-encoded parameter of roughly `4/3·N + 30` bytes. The hidden message is
  *larger* than the exposed one, against RFC 3261 §18.1.1 — "if it is larger than 1300 bytes and
  the path MTU is unknown, the request MUST be sent using an RFC 2914 congestion controlled
  transport protocol, such as TCP". Buying concealment by pushing UDP callers onto TCP is a trade
  a deployment makes per trunk, deliberately; it is not a cluster default.
- **It cannot be a hook module.** [hook-framework](../specs/hook-framework.md) §3 keeps Via
  engine-internal — "no module can cancel a branch, time out a call, or touch a Via" — and fires
  `BeforeForward` (H9) at F5, before the Via exists at F8. The `topology-hide` id appearing in the
  hook-framework §9 vectors is an **ordering fixture**, not a shipped module and not a commitment
  to one.
- **In the direction usually wanted, Record-Route privacy does not work at all.** RFC 3323 §5.1
  recommends stripping Record-Route only on the originator's behalf, and says of the other
  direction: "This document recommends no way of statefully restoring those headers if they are
  stripped." The route set is learned once (RFC 3261 §12.1.1/§12.1.2) and is not recomputed by a
  target refresh (§12.2.1.2, §12.2.2) — the same rule affinity-token M6 leans on for expiry. An
  entry stripped from a dialog-forming request is gone from that endpoint's route set for the life
  of the dialog.

**Interaction with the affinity token and Record-Route** — normative for any future mechanism:

- **The token already is the platform's topology hiding.** What another design would strip and
  store — which shard, which edge, which media node — this platform writes into a token body that
  is ciphertext by default under a cluster-wide key (affinity-token §4). Concealment with no
  concealment state, which is the whole trick.
- **The Record-Route pair inserted at F4 is exempt from any hiding, always.** Both entries are
  load-bearing routing state: strip them and the endpoints' route sets lose the platform, so
  mid-dialog requests either bypass it or arrive with no `aft` parameter and are answered `403`
  (affinity-token §8; vectors PB-P-4/PB-P-5). A hiding mechanism may only strip entries *other*
  elements added — never the pair, and never our own Via.
- **Different URIs per side is already solved without response-time surgery.** RFC 3261 §16.7
  step 8 lets a proxy rewrite its own Record-Route value in a response so upstream and downstream
  see different URIs, and warns that locating the right value on a spiral is tricky enough to need
  "sufficiently distinct URIs". The two-entry ORIG/TERM pair (affinity-token M2) hands each side
  its own entry in the request itself, so this platform needs neither the rewrite nor the
  workaround.
- **Via surgery would not perturb loop detection.** The RFC 5393 cookie (§6 above) is computed over
  the routing-relevant fields — Request-URI, To/From tags, Call-ID, CSeq number, Route values — and
  deliberately excludes the Via stack, so stripping or restoring entries cannot turn a spiral into
  a loop or hide one.

**Considered for upstream** (AGENTS.md rule 6): **no** — the decision and any future mechanism are
trust-boundary policy over cluster-owned identities and keys, which is orchestration by
definition; the only protocol-generic piece either would need is the `Headers` surgery API, which
already landed upstream as sipx `S-15` ([upstream ledger](../upstream.md), PX-3).

**What a deployment that hides topology today does instead.** A deployment arriving with a
per-dialog header store gives it up rather than porting it. It is not merely unnecessary here, it
is incompatible: a mid-dialog request may land on any edge — that is the point — the platform
holds no dialog store to key into, and the dialog events it does expose (hook-framework H12/H13)
are explicitly observational and "MUST never be load-bearing for routing or admission". The three
things such a store actually buys are each replaced somewhere else:

| What the store bought | What replaces it here |
|---|---|
| internal node addresses off the wire | structural: one Via naming an advertised identity, one Record-Route pair naming a cluster identity (DP-5; affinity-token §5) |
| internal routing state off the wire | the token body, encrypted by default (affinity-token §4) |
| the caller's headers stripped toward a given carrier | the per-trunk egress header allowlist (RT-5) and per-trunk privacy/PAI policy (RT-7) — per carrier, which is the grain the obligation actually has |

What such a deployment gains: replicas become interchangeable, so a rollout no longer needs a
drain window sized to the longest dialog, and no region needs to run a single replica to keep a
dialog store coherent.

What it gives up, stated plainly rather than papered over: `Privacy: header`-grade **Via
stripping** on the originator's behalf. RT-5 removes headers per trunk, but Via is not among them
— a proxy that removes Via entries has thrown away the response path (RFC 3261 §16.7 routes
responses by the stack it did not touch), and restoring them is precisely the mechanism above. A
deployment under a contractual Via-stripping requirement needs either that mechanism or a B2BUA
leg (`services-b2bua`), which is where RFC 3323 §5.1 puts it. Nothing in v1 closes that gap.

## Alternatives considered

- **Extend sipx-transport's `Driver` to 1→N fan-out.** Rejected (with the user): it destabilizes
  a shipped, UA-shaped endpoint loop and couples both roadmaps; the kernel's transaction layer is
  the right reuse boundary, not the UA driver above it.
- **B2BUA-first (terminate every call).** Rejected by the vision: it multiplies replicated state
  (two dialogs, two offer/answer machines, CSeq translation per call) and forfeits end-to-end
  transparency for the common case.
- **Stateless-only edge.** Rejected: forking, authentication challenges, CANCEL and Timer C all
  require transaction state; RFC 3263 §4.4 failover itself recommends becoming stateful after the
  first failed attempt.
- **Topology hiding with a per-dialog header store** — strip Via/Record-Route on egress, persist
  the removed values, restore them on every later message of the dialog. Rejected (PX-8): this is
  RFC 3323 §5.1's own assumed construction, and it is forbidden by invariant 5 — a lookup on the
  mid-dialog hot path. Keyed by the node that minted the entry, it also makes replicas
  non-interchangeable, which is the failure the affinity token exists to remove.
- **Topology hiding with the removed stack encrypted into our own Via parameter** — RFC 3323
  §5.1's stateless alternative, state riding the message. Admissible under the invariants and
  specified above, but **not built in v1** (PX-8): it enlarges the message it is meant to shrink
  (RFC 3261 §18.1.1), and every deployment requirement it would serve is already per-trunk work in
  `routing-trunks` (RT-5, RT-7).

## Risks & open questions

- ~~The exact crate boundary against sipx-transport.~~ **Settled by PX-2:** the driver is built on
  `sipx_transport::Handle`; `Pool`, `Resolver`, `resolve`, `Target` and `TransportKind` are all
  public and imported as they are; nothing is extracted; two gaps are filed upstream as sipx
  `T-18` and `T-19`.
- Record-Route token size: RFC 3261 puts no hard limit on URI length, but UDP fragmentation is
  real; the affinity token spec (AF-1) must budget bytes with this epic in the review.
- ~~Whether stateless mode ships in M1 at all.~~ **Decided:** stateless mode is defined in the
  spec (PX-1) but implemented only in M2, when the token path gives it a real consumer; M1 ships
  transaction-stateful forwarding only. The roadmap's M1 entry says the same.
- ~~Whether the platform hides internal topology from peers.~~ **Decided by PX-8:** out of scope
  for v1 — there is no internal topology on the wire to hide, and the residual demand is per-trunk
  privacy policy (RT-5, RT-7). Two things the decision leaves open:
  - **The single-border-Via property depends on the platform presenting one SIP hop at its
    border.** The reference topology does: routing/policy is consulted rather than hopped, and the
    only cross-node hop is the connection-owner RPC, which pushes no Via. Whether a deployment may
    chain `edge` and `outbound-proxy` as two *SIP* elements is not settled —
    [DP-1](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/DP-1-design-roles-and-the-config-schema.md)
    owns the role model. If it may,
    the first element's internal-facing Via crosses the border on every egress request and PX-8
    reopens. Named here so that answer is deliberate rather than incidental.
  - **The property is prose until it is a vector.** Normative rows for it in
    [proxy-behavior](../specs/proxy-behavior.md) and their conformance-registry wiring are a
    deferred follow-up, not part of PX-8.

## Acceptance / done

The union of PX-1 … PX-8: a normative `docs/specs/proxy-behavior.md` with vector tables; a
sans-IO proxy core passing those vectors under the deterministic harness, including forking,
CANCEL, Timer C, loop detection and best-response selection; an M1 node through which two
sipx CLI phones register and call each other with media direct; and the topology-hiding question
answered in writing (PX-8) rather than left to whoever meets it first in a deployment.
