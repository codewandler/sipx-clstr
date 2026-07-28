# Design: Cluster affinity & connection ownership

**Status:** proposed · **Pillar:** Cluster · **Epic:** `cluster-affinity` ·
**Stories:** AF-1 … AF-6

## Why

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
id, tenant, home shard, edge affinity, media node id, policy version, expiry, random nonce, and
an authentication tag. Always authenticated; encrypted when contents are sensitive. Never raw
hostnames or internal node identifiers in public tokens. Verification failure behavior is
normative (AF-1): an unverifiable token on a mid-dialog request is a hard reject, not a fallback
to guesswork. Key rotation is built into the format (key id + overlapping validity); keys are
distributed by configuration in v1.

**Processing a mid-dialog request** then needs no lookup: verify token → learn tenant, direction,
media node → execute the transaction → forward along the Route set. Any edge can do it; the dialog
does not belong to a node.

**Flow references.** TCP/TLS/WS connections cannot move between nodes, so they get an owner: the
edge that accepted the connection keeps a local connection table (transport, remote address,
authenticated identity, TLS info, flow generation, last activity). The location binding stores an
opaque `flow_ref = signed(node_id, connection_id, generation)` — never a socket. Delivering a
request to such a binding means: resolve → inspect flow_ref → RPC to the owning edge → the owner
writes to the connection. The generation counter makes references to a dead connection detectable
(bumped on reconnect); if the owner is gone, the caller uses another registered flow or treats
the binding as temporarily unavailable — with RFC 5626 semantics arriving in M3, this is the
`430 Flow Failed` path.

**The connection-owner RPC** (AF-3) is an internal, mutually authenticated interface with
delivery semantics specified up front: at-most-once delivery, bounded queueing, and an explicit
failure answer (owner unreachable ≠ flow dead ≠ flow rejected). It is the only cross-node hop on
the signalling path, and it exists only for requests toward connection-bound clients.

**Membership and keys are config-first.** v1 has no consensus system and no discovery protocol:
the node set, shard map and token keys come from configuration (`deployment` epic), reloadable
without restart. A dynamic membership service is a later, separate decision.

## Alternatives considered

- **Global dialog database.** Rejected by principle: adds a strongly consistent read to every
  mid-dialog message, couples call availability to the store's availability, and replicates state
  the message can carry for free.
- **Sticky routing at the load balancer (L4 affinity).** Rejected as the correctness mechanism:
  helps as an optimization, but a dead edge must not strand its dialogs; tokens keep every edge
  able to route.
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

## Acceptance / done

The union of AF-1 … AF-6: a normative `docs/specs/affinity-token.md` with byte-level vectors; a
mint/verify library passing them; tokens round-tripping through the proxy's Record-Route/Route
path in the harness; flow_ref + connection table + owner RPC delivering to connection-bound
clients across nodes; and the M2 assertion — mid-dialog requests across a 3-node simulated
cluster with the cross-node dialog-lookup counter at zero.
