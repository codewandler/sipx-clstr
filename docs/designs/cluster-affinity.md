# Design: Cluster affinity & connection ownership

**Status:** proposed · **Pillar:** Cluster · **Epic:** `cluster-affinity` ·
**Stories:** AF-1 … AF-7

## Why

What makes N nodes one proxy: routing state rides in the message, and every resource has one owner.

This is the subsystem that makes sipx-clstr a cluster rather than N independent proxies, and it
embodies the vision's second principle: **state rides the message**. Mid-dialog requests must be
routable by *any* healthy edge with zero global dialog lookups — the alternative, a shared dialog
database on the hot path, adds a consensus round-trip to every in-call message and a new failure
mode to every call. The mechanism: signed opaque tokens carried where SIP already carries routing
state (Record-Route → Route, Path, flow tokens), plus explicit ownership for the one resource
that genuinely cannot ride in a message — a client's TCP/TLS/WS connection. Because this
subsystem is the most novel, it is specified tightest: the token is a byte-level format with
security obligations, and AF-1 writes the normative spec before any consumer hardens.

## Approach

**The affinity token.** Minted by the edge that handles dialog-forming requests and encoded into
the Record-Route URI it inserts (and, for registrations, into Path). Fields: format version, key
id, tenant, home shard, edge affinity, direction (which side of the dialog this Route position
faces — a mid-dialog request at a foreign edge must be able to tell upstream from downstream),
media node id, policy version, expiry, a random nonce, and an authentication tag. The nonce
provides mint uniqueness and unlinkability — it is deliberately **not** a replay defense:
re-presenting the same token on every mid-dialog request is the mechanism, and cross-context
abuse is bounded by expiry and the scope fields. Always authenticated; encrypted when contents
are sensitive. Never raw hostnames or internal node identifiers in public tokens. Verification failure behavior is
normative (AF-1): an unverifiable token on a mid-dialog request is a hard reject, not a fallback
to guesswork. Key rotation is built into the format (key id + overlapping validity); keys are
distributed by configuration in v1.

**Processing a mid-dialog request** then needs no lookup: verify token → learn tenant, direction,
media node → execute the transaction → forward along the Route set. Any edge can do it; the dialog
does not belong to a node.

**Transaction affinity is the dataplane's job.** The token solves *dialog* routing; messages
scoped to a single transaction — retransmissions, CANCEL, ACK to a non-2xx — carry no Route set
and must reach the edge holding the server transaction. That affinity is provided by the L4
dataplane's same-flow stickiness (same 5-tuple → same edge for UDP; connections are pinned by
nature), which is therefore a **documented, load-bearing deployment requirement** (DP-2), not an
optimization. A stickiness miss must degrade per RFC 3261, not corrupt: the proxy spec (PX-1)
specifies the required behavior when a transaction-scoped message arrives at an edge without the
transaction (statelessly forwarded CANCEL, retransmission handling), and quantifying that path is
a named risk below.

**Flow references.** TCP/TLS/WS connections cannot move between nodes, so they get an owner: the
edge that accepted the connection keeps a local connection table (transport, remote address,
authenticated identity, TLS info, flow generation, last activity). The location binding stores an
opaque `flow_ref = signed(node_id, connection_id, generation)` — never a socket. Delivering a
request to such a binding means: resolve → inspect flow_ref → RPC to the owning edge → the owner
writes to the connection. The generation counter makes references to a dead connection detectable
(bumped on reconnect); if the owner is gone, the caller uses another registered flow or treats
the binding as temporarily unavailable — with RFC 5626 semantics arriving in M3, this is the
`430 Flow Failed` path.

**Settled in AF-2** ([affinity-token](../specs/affinity-token.md) §11–§14 — the flow reference
lives in the token's spec because the two share one key set, one construction and one parser
prologue). Three decisions the sketch above left open:

- *The reference is `signed(tenant, node, **incarnation**, connection, generation, transport)`, not
  the three-field sketch.* `incarnation` — the node's boot second — is what stops a restarted node
  from re-issuing an identity a live reference already names; without it the generation counter
  only protects within one run of one process. `transport` lets a caller refuse a `sips` target on
  a plaintext flow (RFC 3261 §26.2.2) without asking the owner. The whole record is 49 bytes.
- *No expiry.* The token needs one because a dialog exists nowhere; a reference names a row that
  either exists or does not, and that is a tighter bound than a clock and needs no clock. So
  `verify_flow` is the only record verifier in this epic that takes no `now`, and the invariant is
  stated positively: **a reference resolves to the connection it was minted for, or to nothing.**
- *The two families are domain-separated cryptographically*, by a version byte inside the
  authenticated input — not by convention — because they share keys and, in M3, will share
  carriage.

The reachability question the sketch did not raise: a client that reconnects and *retransmits* the
same REGISTER leaves a binding pointing at the dead connection until its next refresh. That is a
latency defect, never a mis-delivery, and closing it means touching the registrar's idempotency
comparison — flagged to `RG-8` rather than decided in AF-2 (spec §13.3 BI5).

**Considered for upstream: no** — a flow reference names this platform's node set and this node's
table slots, and RFC 5626 §5.2 leaves flow-token construction to the edge proxy that mints it;
the connection table is orchestration over the handle the kernel's transport layer already
returns, not a second transport.

**The connection-owner RPC** (AF-3, implemented by AF-7) is an internal, mutually authenticated
interface with delivery semantics specified up front: at-most-once delivery, bounded queueing,
and an explicit failure answer (owner unreachable ≠ flow dead ≠ flow rejected). It is the only
cross-node signalling hop the platform itself performs — transaction affinity above is provided
by the dataplane, not by a hop — and it exists only for requests toward connection-bound clients.

**Membership and keys are config-first.** v1 has no consensus system and no discovery protocol:
the node set, shard map and token keys come from configuration (`deployment` epic), reloadable
without restart. A dynamic membership service is a later, separate decision.

## Alternatives considered

- **Global dialog database.** Rejected by principle: adds a strongly consistent read to every
  mid-dialog message, couples call availability to the store's availability, and replicates state
  the message can carry for free.
- **Sticky routing at the load balancer (L4 affinity) as the dialog-routing mechanism.**
  Rejected for dialogs: a dead edge must not strand its calls; tokens keep every edge able to
  route mid-dialog requests. Retained deliberately for *transaction* affinity (see Approach),
  where it is the correctness mechanism.
- **Unsigned/cleartext route cookies.** Rejected: Record-Route contents echo back from untrusted
  endpoints; an unauthenticated token is an open redirect into the cluster.

## Risks & open questions

- Token size vs UDP MTU: Record-Route appears in every dialog-forming request/response and the
  Route set in every mid-dialog request; AF-1 must budget bytes (target: token URI parameter well
  under 200 bytes) and the proxy spec review enforces it.
- Clock skew and token expiry for long calls: expiry must outlive plausible dialog lifetimes or
  be refreshable on target refresh; decided in AF-1.
- RPC transport choice (likely the workspace's own framing over TLS; no external broker) —
  settled in AF-3.
- Key compromise blast radius and rotation cadence; whether tenant ids in tokens require
  encryption by default.
- **Node-id uniqueness is now a correctness input, not a convention** (AF-2 §12.2 CT1): two nodes
  sharing a logical id give two different connections one flow identity, which is the only way to
  break the reference invariant from outside its spec. AF-6/DP-1 must reject it at config-validation
  time rather than warn.
- Key rotation now has to outlast registrations, not only tokens: a reference leaves circulation
  when the binding holding it is refreshed, so the overlap window is `max(token lifetime,
  maximum registration expiry)` (AF-2 wrote that term into the rotation rule and the mint-window
  rule both). A deployment that raises registration expiry above the token lifetime lengthens
  every rotation.
- **That rotation bound rests on a location binding being a reference's only carrier**, which is
  true in M1/M2 and is what makes registration expiry a bound at all. M3's Path-URI carriage would
  break it: a route set an endpoint has learned is never recomputed (RFC 3261 §12.2.1.2), so a
  reference sitting in one is refreshed by nothing. M3 must therefore either re-bound rotation or
  give the reference an expiry after all — it cannot inherit AF-2's argument unexamined.
- Two lifetimes now have to be kept in step, and the platform owns both ends: a connection's idle
  timeout and the registration expiry granted over that connection. AF-2 clamps the grant to the
  idle timeout rather than trusting them to be configured compatibly, because the failure is
  silent — the binding outlives the flow and every call toward that client is answered `480` until
  the binding expires. Worth revisiting once RFC 5626 keepalives (M3) give flows a liveness signal
  that does not depend on the refresh cadence.
- The stickiness-miss path: how often the L4 dataplane delivers a transaction-scoped message to
  the wrong edge in practice, and whether PX-1's degraded behavior (stateless CANCEL forwarding,
  retransmission absorption) is enough — measured under the harness's fault schedules.

## Acceptance / done

The union of AF-1 … AF-7: a normative `docs/specs/affinity-token.md` with byte-level vectors; a
mint/verify library passing them (AF-4); tokens round-tripping through the proxy's
Record-Route/Route path in the harness (AF-5); the connection table, flow_ref plumbing and owner
RPC implemented (AF-7) and delivering to connection-bound clients across nodes; and the M2
assertion — mid-dialog requests across a 3-node simulated cluster with the cross-node
dialog-lookup counter at zero.

`PX-15` supplies the immediate per-process secure-randomness seam for loop-cookie keys. `AF-6`
retains ownership of how keys are distributed, rotated and versioned across a cluster; the proxy
story must not invent a competing cluster key schema.
