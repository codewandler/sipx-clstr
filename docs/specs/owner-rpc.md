# Spec: The connection-owner RPC

**Status:** normative · **Crate:** _none of its own — the caller and owner halves are
`crates/sipx-clstr-proxy`'s decisions and `crates/sipx-clstr-node`'s driver (`AF-7`)_ ·
**Stories:** AF-3, AF-7 · **Design:** [cluster-affinity](../designs/cluster-affinity.md)

The platform's **only** cross-node signalling hop. Everything a mid-dialog message needs in order
to be routed rides in the message ([affinity-token](affinity-token.md) §2–§10), and the one thing
that cannot ride in a message — a client's TCP/TLS/WS connection — is named by a reference that
carries its owner (§11–§13 there). This document is what happens after `verify_flow` says
`claims.flow.node` is somebody else: how the request gets to that node, what may come back, and
what each answer costs the request.

[affinity-token](affinity-token.md) §13.1 D5 is the one-line version and is not restated here.
What that spec deliberately left open — "the owner RPC's transport, node-to-node authentication and
queueing (AF-3 — §13.2 fixes only the outcomes it must distinguish)" — is this document, and
[cluster-membership](cluster-membership.md) §1 leaves the same hole from the other side: it fixes
`membership[].rpc`, the endpoint a peer dials, and nothing about what is spoken to it.

**The posture in one sentence, because every rule below follows from it:** this hop is not a new
protocol, it is **one more SIP hop whose next hop is a flow rather than a URI** — so the message
that crosses it is the message, the answer that comes back is a SIP response, and the failure
taxonomy is a mapping rather than a translation.

## 1. Normative references

- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- RFC 3261 §16.6 — request forwarding, which this hop performs unchanged except for F7's next hop;
  §16.7 — response processing, which the caller performs unchanged **after** delivery (§7);
  §16.9 — a transport error on a branch, the input `OwnerUnreachable` is derived from (§7);
  §16.10 — CANCEL, which crosses this hop like any other request (§5 CR6);
  §16.11 — stateless mode, and why neither end of this hop qualifies for it (§3 CH6);
  §18.2.2 — a response is returned over the connection its request arrived on, which is what makes
  the owner's response path stateless;
  §9.1 — a CANCEL is built with the Route values of the request it cancels (§5 CR6);
  §25.1 — the `user` production the reference's text form must satisfy (§5 CR2);
  §26.2.2 — the `sips` scheme's hop-by-hop TLS requirement, satisfied here by §3 CH1 rather than
  by a per-request check.
- **RFC 5626** §5.2 — generating a flow token and placing it in the **user part** of a URI; §5.3 —
  an edge proxy using a flow token to forward a request toward the flow it names; and its
  `430 Flow Failed` response code, which is exactly this hop's dead-flow answer and is adopted in
  M3 (§7 AN8). [affinity-token](affinity-token.md) §11 is the construction, generalized so the
  token names the owning *node* as well as the flow; §5 here is that token in a Route.
- **RFC 3326** — the `Reason` header field, which marks a response as the owner's own answer about
  delivery rather than the client's answer to the request (§7 AN2).
- **RFC 4320** §4.1 — `100` is the only provisional a non-INVITE request may receive. It is what
  lets the owner acknowledge a write with one mechanism for every method (§7 AN1).
- **RFC 8446** — TLS 1.3, the floor for the peer channel (§3 CH2).
- **RFC 9525** — service identity in TLS: the reference identity a client checks and how a
  certificate presents it. §4 fixes what the reference identity is here.
- RFC 5922 — domain certificates in SIP, for the `sip`/`sips` identity forms a deployment may
  already be issuing; §4 AU4 permits them and requires nothing new of them.
- RFC 5923 — connection reuse: what may be sent on a connection is decided by its validated
  identity, not by its 5-tuple. The peer channel is a connection whose validated identity is a
  cluster member, and §4 AU5 is that rule applied to it.
- Our specs consumed by / consuming this one:
  [affinity-token](affinity-token.md) §11.2 (the reference's fields and its 66/60-character text
  form), §11.5 (verification, and that every failure collapses to `Invalid`), §12.1 (the connection
  table row a delivery writes to), §12.2 CT9 (`Draining` still delivers), §12.3 (`T_idle`),
  §13.1 D1–D8 (the chain, and that `Invalid` removes a target before a branch exists), §13.2 (the
  resolution steps RS1–RS5 and **the outcome taxonomy this document carries**), §13.3 BI4 (tenant
  agreement);
  [proxy-behavior](proxy-behavior.md) §2 (the effect set this hop needs no addition to), §3 A1/A3
  (why neither end is stateless), §5 P2 (popping our own Route), §7 F1–F11 (forwarding), §7.1 (the
  target queue a removed target returns to), §8 R8/R10 (`503` from a branch, and a branch transport
  error), §11 (transaction affinity, which this hop is **not**);
  [cluster-membership](cluster-membership.md) §3 (`membership[].rpc`, MB5–MB7), §4 (`keys[]` and
  KY9), §8 UQ1–UQ3 (what may never be derived), §9 DY4 (a node starts with nothing reachable);
  [cluster-config](cluster-config.md) §5 P1/P5/P7 (identity is a start-up input; the identity set;
  advertised-address rules), §7 (the section registry a value here needs a row in), §8 V8/V9/V10
  (declared ceilings, no secret in the document, refusing to start);
  [location-service](location-service.md) §7 L7 (`flow_ref` carried verbatim on a `Target`).

**Out of scope.** The reference's byte layout, cryptography and verification order
([affinity-token](affinity-token.md) §11); the connection table's schema, lifecycle and timers
(§12 there); what a `Target` is and where `flow_ref` comes from
([location-service](location-service.md)); the endpoint's *place in the configuration document*
([cluster-membership](cluster-membership.md) §3 — this document fixes what is spoken to it, not
where it is declared); which address the owner binds beyond §3 CH4's rule; the spelling of the four
configuration values §8 introduces ([cluster-config](cluster-config.md) §7 owns spellings, and §10
here records the row they need); RFC 5626 outbound semantics, UDP flows and the Path carriage of a
reference (M3, [affinity-token](affinity-token.md) §11.4 FM6 and its §11.4 caveat); and how a
deployment obtains and rotates the peer channel's TLS material, which is transport material a
deployment already has for `sips` listeners and is named by reference like every other
([cluster-config](cluster-config.md) §8 V9).

**Upstream considerations** (AGENTS.md rule 6): **no — this is orchestration.** Which node owns a
client's connection, how that node is addressed, and what a failed delivery costs a request are
facts about *our* membership, *our* connection table and *our* configuration document, none of
which the kernel has a concept of. Everything this hop rides on is protocol-generic and is
**consumed rather than re-implemented**: SIP parsing and lossless re-serialization, the client and
server transaction machines, TLS transport, and RFC 5626's flow-token-in-the-user-part
construction. Even the M3 adoption of `430` needs no kernel change — the kernel's `StatusCode` is a
checked `u16` newtype, so an unlisted code is expressible today. Nothing new joins
[the ledger](../upstream.md).

## 2. What this is, and what it is not

| # | Rule |
|---|---|
| RP1 | **It is a delivery interface, not a control plane.** The only interaction the peer endpoint carries is "deliver this SIP request to the flow this reference names, and tell me whether you did". Configuration, membership, key material, health, statistics and shard handoff are not spoken here and MUST NOT be added to this endpoint; each already has a home ([cluster-membership](cluster-membership.md) §4 KY9 and §8 UQ3 forbid the one that would be most tempting). |
| RP2 | **It is not transaction affinity.** [proxy-behavior](proxy-behavior.md) §11's transaction-scoped messages reach the right edge because the dataplane pins a 5-tuple, and that is a deployment requirement rather than a hop. This hop exists only for a request whose target came from a location-service lookup carrying `flow_ref` ([affinity-token](affinity-token.md) §13.1 D1). |
| RP3 | **It is not a lookup, and it may not become one.** The owner falls out of the reference (D2); nothing here consults a directory, queries membership for *who owns this flow*, or broadcasts. `membership[].rpc` is consulted only to turn an id the reference already carries into an address, which is configuration the node already holds. A design that reintroduced a shared socket registry on this path would be wrong by definition (AGENTS.md rule 5). |
| RP4 | **The taxonomy is not this document's to invent.** [affinity-token](affinity-token.md) §13.2 fixes four outcomes — `Delivered`, `FlowDead`, `FlowRejected`, `OwnerUnreachable` — and what each costs the request. This document fixes how each one is *produced and observed on the wire*, and adds no fifth. Every condition §3–§8 names resolves to one of the four. |
| RP5 | **Refusing is the only failure mode of the channel itself.** A peer that cannot be authenticated is not downgraded, not retried in the clear, and not logged-and-continued; a node whose own peer listener cannot start does not start ([cluster-config](cluster-config.md) §8 V10). |

## 3. The channel

| # | Rule |
|---|---|
| CH1 | **TLS over TCP, mutually authenticated, always.** There is no plaintext mode, no per-deployment switch and no trusted-network exemption. This channel can write arbitrary bytes into any registered client's socket, so an unauthenticated one is a cluster-wide request-injection primitive; and a safety rule that can be switched off from the document is one that gets switched off during the incident it exists for ([cluster-membership](cluster-membership.md) §7.1 RB9's lesson, applied). It is also what makes a `sips` request deliverable across this hop at all (RFC 3261 §26.2.2) without a second check. |
| CH2 | **TLS 1.3 (RFC 8446) is the floor.** Both ends of this channel are this platform's own processes, so there is no legacy peer to accommodate and no reason to negotiate downward. A deployment cannot lower it from the document. |
| CH3 | **The peer channel carries SIP, not a second framing.** The payload is a SIP request and the answer is a SIP response; both are parsed and serialized by the same kernel machinery as every other hop, so the lossless re-serialization guarantee ([proxy-behavior](proxy-behavior.md) §7) holds across the cluster rather than up to its edge. See §9 for what was weighed against this. |
| CH4 | **A dedicated listener, and its bind address is a start-up input.** The peer endpoint is not one of the node's SIP service listeners: it serves no role, admits no tenant, and is projected by nothing. What it **advertises** is exactly this node's own `membership[].rpc` and nothing else ([cluster-membership](cluster-membership.md) MB6). What it **binds** arrives beside `NodeIdentity` as a start-up input ([cluster-config](cluster-config.md) §5 P1; [cluster-membership](cluster-membership.md) MB7 already says the document cannot carry it), defaulting to all interfaces on the port of this node's own `rpc` entry. Defaulting to all interfaces is deliberate: a bind address is not an access control, CH1 is, and a default that guessed an interface would fail in the deployment that cannot know its address at authoring time. |
| CH5 | **The peer connection is never a flow.** It gets no [affinity-token](affinity-token.md) §12.1 row, no slot, no generation and no reference; it does not count against `max_connections` (CT8); and no reference can ever name it. A peer link is a cluster-internal pipe, not a client. |
| CH6 | **Both ends are transaction-stateful, by rules that already exist.** [proxy-behavior](proxy-behavior.md) §3 A1 admits stateless handling only for a request carrying a Route with a valid *affinity token* — a flow reference is the other family (§11.3 DS1 there) — and A3 excludes anything needing a local `100`, which §7 AN1 requires of the owner. No new applicability rule is needed and none is added. |
| CH7 | **One connection per peer, opened on demand, never at start-up.** A node MUST NOT dial any peer while starting: a start-up that waited on the fleet would make every call in the cluster depend on the fleet being up, which is the coupling [cluster-membership](cluster-membership.md) §9 DY4 exists to refuse and which membership being a *declaration* (CM3) already permits — an entry may name a node that has never run. The connection is opened by the first delivery that needs it, kept open across deliveries, and re-dialed after a failure subject to §8's `T_peer_retry`. |
| CH8 | **A changed `rpc` takes effect for deliveries begun after the reload**, which is [cluster-membership](cluster-membership.md) RD5 seen from this side. An established peer connection to the old endpoint is closed once it carries no in-flight delivery; a delivery already in flight completes or fails with §7's answer. |
| CH9 | **The channel is not multiplexed beyond SIP's own concurrency.** Several deliveries share one peer connection, distinguished by their transactions exactly as several requests share any SIP connection. There is no session, no sequence number, no correlation id and no handshake of ours on top of TLS — every one of those would be state to resynchronize after a reconnect, and a reconnect is the case this hop must survive cheaply. |

## 4. Authentication

The question this section answers is narrow and worth stating before the rules: **what does each
end need to know about the other, and what is it not allowed to invent in order to know it?**

| # | Rule |
|---|---|
| AU1 | **Both directions are authenticated by the TLS handshake and by nothing else.** There is no application-layer credential, no bearer token, no challenge and no nonce anywhere in this hop. `CX-5` is why that is written as a prohibition rather than left as an omission: a value that must be unique and is computed from inputs two nodes share is the shape a challenge design falls into by looking clever, and the cheapest way not to have that defect is not to have that mechanism ([cluster-membership](cluster-membership.md) §8 UQ1). |
| AU2 | **The caller's reference identity is the `rpc` host it dialled** (RFC 9525). The caller resolves the owner's id to a `membership[].rpc` entry, dials it, and MUST verify the presented certificate against that host. No new configuration field is needed for this and none is added: `rpc` is already required, already unique in the document (MB6), and already the name the caller is reaching for. |
| AU3 | **The owner's acceptance rule is membership, not a peer list.** A connection is accepted only if the presented client certificate's identity is the `rpc` host of **some** member in the active configuration version. The owner therefore needs no per-peer configuration and no second document: it already holds every member entry, byte-identical to every other node's ([cluster-membership](cluster-membership.md) CM2). A member removed by a reload stops being accepted at the next connection, which is MB9 working rather than an exception to it. |
| AU4 | **The certificates are transport material, referenced the way transport material already is.** They are not a new mechanism: [cluster-config](cluster-config.md) §7 already registers `tls` on a listener and §8 V9 already names TLS private keys by reference and refuses inline values. A deployment issuing RFC 5922 domain certificates for its `sips` listeners may use the same issuance path here; this document requires nothing of the issuer beyond AU2's and AU3's identity check. |
| AU5 | **`keys[]` is not used to authenticate this channel, and this is a decision rather than an oversight.** Three reasons, each sufficient. *Key separation*: a secret that both mints wire records and keys a channel has two rotation calendars and one value, and [affinity-token](affinity-token.md) §6 K4's calendar is derived from record circulation (`max(L, E_max) + S`), which has nothing to do with how long a TCP connection should be trusted. *It cannot name a peer*: `keys[]` is a **group** secret held byte-identically by every node (CM2), so possession proves "some cluster node" and can never prove "the node this reference names" — and AU2 needs exactly the second. *It would create the handshake the subsystem is built not to have*: using a `keys[]` entry as a pre-shared key makes the record-minting secret an input to a node-to-node key agreement, which is one restatement away from the key-exchange protocol [cluster-membership](cluster-membership.md) §10 and §4 KY9 forbid outright. |
| AU6 | **What TLS establishes here is a channel key, not key material, and the distinction is load-bearing.** [cluster-membership](cluster-membership.md) §8 UQ3 forbids nodes to *agree* on a value that must be unique; a TLS handshake agrees on an ephemeral session key that protects one connection, is never a `keys[]` entry, is never persisted, and mints nothing. No node becomes a source of key material (KY9), nothing is derived from inputs two nodes share (UQ2), and §8's uniqueness table gains no row. |
| AU7 | **Peer authentication is an integrity and availability control; it is not what keeps a request out of a stranger's socket.** That is the reference's own `node` field: a reference presented to the wrong node resolves `FlowDead` at [affinity-token](affinity-token.md) §13.2 RS1 before anything is written, so even a caller that reached the wrong peer mis-delivers nothing. Stated positively so that the two mechanisms are not confused and neither is asked to carry the other's weight — MB6 notes that nothing re-checks that the answer came from the endpoint the reference named, and RS1 is the reason that is a reachability property rather than a safety one. |
| AU8 | **A request carrying a flow-reference Route is refused on every listener except the peer listener**, with `403 Forbidden` — [proxy-behavior](proxy-behavior.md) §5 P3's shape and status, for the same reason. This is the SIP-layer half of CH1: a client that presents cluster-internal routing is not making a routing request, and the refusal does not depend on the TLS check having been reached. §5 CR5 is the identity-set narrowing that makes it mechanical. |

## 5. Carriage: the request on the wire

The caller forwards the request through [proxy-behavior](proxy-behavior.md) §7 F1–F11 **unchanged**,
with one addition at F4/F5 and one substitution at F7.

| # | Rule |
|---|---|
| CR1 | **The reference travels in a Route header, in the user part of a platform URI** — RFC 5626 §5.2's own construction, which [affinity-token](affinity-token.md) §11 already generalizes. The caller pushes `Route: <sips:REF@HOST:PORT;lr>` where `REF` is the reference's canonical text form ([affinity-token](affinity-token.md) §11.2 — base64url, unpadded, 66 characters encrypted or 60 authenticated-only) and `HOST:PORT` is the owner's `membership[].rpc`. No new header, no new parameter and no wrapper: a Route is what SIP already uses to say "go via here", and a loose route is what the owner already knows how to pop ([proxy-behavior](proxy-behavior.md) §5 P2). |
| CR2 | **The text form needs no escaping and that is not a coincidence.** base64url's alphabet is `A–Z a–z 0–9 - _`, and `-` and `_` are `mark` characters — and therefore `unreserved` — in RFC 3261 §25.1's `user` production, so the reference is a legal user part verbatim, unescaped. A form that needed escaping would make "the same reference" a question about encodings, and [affinity-token](affinity-token.md) §11.2's identity comparison (D6) exists precisely to keep that question from arising. |
| CR3 | **The Request-URI is the target, unchanged.** F2 puts the location service's contact there and this hop does not touch it: the owner is a *route*, not the destination, and a request that arrived at the wrong owner must still be a well-formed request toward that contact. Nothing about the target is inferred from the reference and nothing about the reference is inferred from the target. |
| CR4 | **The caller Record-Routes; the owner does not.** The dialog's Record-Route pair is minted at the edge that ran F4 — the caller, which did the lookup — and the owner adds nothing to any route set. An owner that Record-Routed would pin every mid-dialog request of that dialog to itself, which is a shared-fate dependency on the one node the design went to trouble to make replaceable. The owner does push a Via (F8) and does decrement `Max-Forwards` (F3), because it is a proxy hop and RFC 3261 §16.6 admits no third kind; the Via is also what returns the response with no state at the owner (RFC 3261 §18.2.2). |
| CR5 | **The peer endpoint is a separate identity, recognized only on the peer listener.** [cluster-config](cluster-config.md) §5 P5's identity set — the union of the advertised addresses of a node's projected listeners, which is what lets any edge pop any edge's Route — MUST NOT include the peer endpoint. Otherwise a flow-reference Route arriving from a client on an ordinary listener would be popped and honoured, and CH1's whole point would be reachable around it. The peer endpoint is matched only by the node that owns it, only on the listener that received it. |
| CR6 | **CANCEL crosses this hop like any other request.** RFC 3261 §9.1 builds a CANCEL with the Route values of the request it cancels, so the flow-reference Route is present without the caller doing anything special, and §16.10 applies at both ends unchanged. A CANCEL is a second delivery to the same flow; §6 OW7's at-most-once is per request, not per flow. |
| CR7 | **Two platform hops, and the cost is named rather than hidden.** A cross-node delivery consumes two `Max-Forwards` decrements and leaves two Via entries instead of one. Neither end may compensate — inflating `Max-Forwards` to hide a hop forges the loop bound RFC 3261 §16.6 step 3 exists to enforce, and the extra Via is what makes the hop visible in a capture, which is worth more than the byte it costs. |

## 6. Delivery at the owner

The owner's work is a fixed sequence, and the first failing step wins.

| # | Step | On failure |
|---|---|---|
| OW1 | The request arrived on the peer listener over a channel accepted by §4 | not reached — an unaccepted channel carries nothing |
| OW2 | The first Route resolves to **this node's** peer endpoint and carries a user part; pop it ([proxy-behavior](proxy-behavior.md) §5 P2) | no such Route: this is an ordinary request on a listener that admits none — `403`, as AU8 |
| OW3 | `verify_flow` the user part against the verify-valid key set ([affinity-token](affinity-token.md) §11.5), pinning no tenant — the owner performed no lookup, so it has no tenant to pin (§13.3 BI4 binds the caller) | `Invalid` → the answer is `FlowDead` (§7 AN4), and telemetry records it separately: an owner that cannot verify a reference a peer could mint is the observable of a key-set disagreement, which [affinity-token](affinity-token.md) §6 K1 and [cluster-membership](cluster-membership.md) §4 KY2 both exist to prevent, and it is an alarm rather than a call failure |
| OW4 | `resolve(claims.flow, table)` — [affinity-token](affinity-token.md) §13.2 RS1–RS5, a pure function of this node's own table and the only lookup on this path | `FlowDead` |
| OW5 | The resolved row's `tenant` equals `claims.tenant` | `FlowDead`. A valid reference can never trip this — FM2 takes both from the same row — so it is a construction check, not a policy one, and it is here because the owner is the one node that verifies a reference it did not mint under a pin it does not have |
| OW6 | Admission: the delivery counts against `admission.maxInFlightTransactions` like every other request, and its per-flow pending writes are within §8's `max_pending_per_flow` | `FlowRejected` |
| OW7 | **Write, once.** The request is serialized to the row's connection exactly once. `Draining` rows still deliver ([affinity-token](affinity-token.md) §12.2 CT9); `Closed` rows resolved at OW4 | the write not completing within §8's `T_write` is `FlowRejected` |

| # | Rule |
|---|---|
| OW8 | **The owner never chooses a different destination.** There is no fallback to the Request-URI, no re-resolution, no second flow and no next hop — a reference that does not resolve produces an answer, never a delivery somewhere else. This is [affinity-token](affinity-token.md) §11's invariant ("resolves to the connection it was minted for, or to nothing at all") at the one place where the temptation to be helpful exists. |
| OW9 | **After the write, the taxonomy is spent.** The owner is an ordinary stateful proxy hop for everything that follows: responses from the client take [proxy-behavior](proxy-behavior.md) §8, a transport error on the client connection is RFC 3261 §16.9 and therefore R10, and none of it is re-labelled as a delivery outcome. §7's answers describe **whether the request reached the client's socket**, and nothing else. |
| OW10 | **The owner strips this document's `Reason` marker from anything it relays**, in both directions. The marker means "the owner is speaking about delivery" (§7 AN2) and a message that merely passed through the owner is not the owner speaking. Without the strip, a client could put the marker in a response and have the caller read it as the owner's answer. |

## 7. The answer, and the failure taxonomy

[affinity-token](affinity-token.md) §13.2 fixes the four outcomes and what each costs the request.
This section fixes how the caller learns which one it got. The three failures are distinct because
they are three different facts — **a dead flow is final and is about the connection; a rejection is
about the owner's load right now; unreachability is about this caller's view of the network and may
heal with nothing about the flow having changed** — and a hop that collapsed them would answer a
temporarily busy node the way it answers a client that hung up.

| # | Rule |
|---|---|
| AN1 | **A successful write is acknowledged with `100 Trying`, for every method.** RFC 4320 §4.1 makes `100` the only provisional a non-INVITE may receive, which is what lets one mechanism cover every method; for INVITE it is the `100` a stateful proxy sends anyway. It is emitted **after** OW7's write completes and not before, because its entire meaning is "the bytes are in the client's socket". It is what cancels §8's `T_owner`. |
| AN2 | **The owner's own answers carry `Reason` (RFC 3326) with the protocol token `SIPX-CLSTR-FLOW`**, compared as a SIP token (case-insensitively), and a `cause` from AN3. A response on the peer channel **without** that marker is the client's answer to the request and takes [proxy-behavior](proxy-behavior.md) §8 unchanged. The marker is the discriminator rather than the status code, because M2's dead-flow answer is `480` and a client may legitimately send `480` itself; and it is not emitted outside the cluster (OW10 strips it inbound, AN7 forbids relaying it upward), so it needs no registration to be unambiguous where it is read. |
| AN3 | **The causes, exhaustively:** `cause=1` the flow is dead (OW4, OW5); `cause=2` the owner refuses now (OW6, OW7's `T_write`); `cause=3` the reference did not verify at the owner (OW3). Cause 3 maps to the same outcome as cause 1 and exists so that a key-set disagreement is visible as itself rather than as a wave of dead flows. A `cause` this document does not define MUST be treated as `FlowDead`: an owner speaking about delivery in a dialect this caller does not know has, by construction, not told it the request was delivered. |
| AN4 | **`FlowDead`** is the owner's answer with cause 1 or 3, and in M2 its status code is **`480 Temporarily Unavailable`** — D9 defers `430` to M3 and this hop emits none before then. The caller does **not** treat it as a candidate response: per [affinity-token](affinity-token.md) §13.2 the target is **removed**, exactly as a target removed by D3, and the branch contributes nothing to [proxy-behavior](proxy-behavior.md) §8's best-response selection. If targets remain queued, §7.1's next group is forwarded; if the set empties, the context concludes `480` by D8 — the same three digits, produced by the caller's own rule rather than relayed from the owner. |
| AN5 | **`FlowRejected`** is the owner's answer with cause 2, status **`503 Service Unavailable`**. It **is** a branch response: [proxy-behavior](proxy-behavior.md) R10's branch-failure treatment applies, and therefore R8 — it becomes `500` upstream if it ends up the best response, and is never forwarded as `503`. An owner that is up and saying "not now" is a server condition, not an unavailable user, and that distinction is the whole reason this outcome is not folded into AN4. |
| AN6 | **`OwnerUnreachable`** is produced by the **caller**, from exactly three conditions and no others: a transport error on the peer channel (RFC 3261 §16.9) at any point before AN1's `100`; a dial that does not complete, or is suppressed by §8's `T_peer_retry`; and `T_owner` elapsing with no response of any class. Its consequence is the caller's, not a response: the target is removed as in AN4. **A transport error on a peer-channel branch is `OwnerUnreachable`, not R10's `503`** — this is the one place this document narrows [proxy-behavior](proxy-behavior.md) R10, and it narrows it because [affinity-token](affinity-token.md) §13.2 already fixed the consequence: a caller that could not reach the owner has learned nothing about the flow and must not report a server error on its behalf. |
| AN7 | **No answer of this hop is forwarded upstream.** The owner's `100`, its `480` with cause 1/3, and (in M3) its `430` are hop-scoped: the caller consumes them and the upstream context sees only what D8 and [proxy-behavior](proxy-behavior.md) §8 produce. `503` is the exception that proves it — R8 already forbids forwarding it as `503`, so it too is transformed rather than relayed. |
| AN8 | **The M3 mapping, decided now so it is not re-litigated then.** RFC 5626's `430 Flow Failed` is exactly AN4's fact — the flow the token names is no longer usable, said by the node that owns it to the node that routed to it — so in M3 the owner answers `430` in place of `480` and the `Reason` marker becomes redundant for that one cause (`430` is unambiguous; `480` is not). **The caller-side mapping does not change:** `430` ⇒ `FlowDead` ⇒ the target is removed ⇒ D8's `480` if the set empties. `430` MUST NOT be forwarded upstream, then or ever. What M3 additionally unlocks is the registrar half — an upstream registrar can drop a dead binding instead of waiting for it to expire — and that is M3's to specify with the rest of outbound, not this document's to anticipate. |
| AN9 | **The outcomes are total and mutually exclusive**, and the order that makes them so is: a response carrying the marker is the owner's answer (AN2 → AN4/AN5); any other response after AN1's `100` is the client's (OW9); a transport error or a timeout is the caller's (AN6); anything else cannot occur, because a response without a marker and before `100` would mean the owner answered without either delivering or speaking about delivery, which OW1–OW7 leaves no path to. An implementation that finds a fourth case has found a defect in this section, not a case to invent an outcome for. |

## 8. Bounds, queues and timers

The bound this section is really about is stated first: **the only queue on this path is the
owner's, it is per flow, and it is short.** A caller-side queue would be a place a request waits
for a node that may never come back, and a retry loop is a queue with better manners.

| # | Rule |
|---|---|
| BQ1 | **The caller keeps no queue of its own.** A delivery is attempted now or it fails now. There is no pending-delivery buffer, no store-and-forward, no deferral to a healthier moment, and no persistence anywhere on this path. The peer channel's write buffer is bounded by the driver, and exhausting it is a transport error — AN6, not a wait. |
| BQ2 | **The owner's queue is per flow and bounded by `max_pending_per_flow`.** Overflow is `FlowRejected` immediately, never a block: a client that is not reading its socket must not be able to make the owner hold requests on its behalf, because the memory that holds them is shared with every other flow on that node. |
| BQ3 | **Inbound admission is the node's existing bound, deliberately.** A delivery counts against `admission.maxInFlightTransactions` ([cluster-config](cluster-config.md) §8 V8) like any other request; the peer listener gets no separate allowance and no priority. A second overload knob would have to be tuned against the first, and a fleet with two overload settings has one it has forgotten. What is specific is only the *answer*: an overload refusal on the peer listener is `FlowRejected` (AN5), so the caller's taxonomy stays closed. |
| BQ4 | **`T_peer_retry` bounds dialling, not delivering.** After a failed dial or a dropped peer channel, a node MUST NOT dial that peer again for `T_peer_retry`; deliveries arriving in that interval fail `OwnerUnreachable` immediately rather than each starting a dial. Without it, a node whose peer is down converts every call attempt into a connection attempt, and the load a dead node causes grows with the traffic it is not carrying. |
| BQ5 | **Every timer is a fired-timer input, never a clock read** (AGENTS.md rule 2), and `now` arrives with the timer. The values below are configuration, and the two ordering constraints are validation rules rather than conventions: a `T_write` that outlived `T_owner` would make the caller give up before the owner's own deadline, and a `T_owner` at or above the caller's Timer B would let a delivery answer arrive after the transaction it belongs to had already timed out. |

| Value | Default | Bound | What it is |
|---|---|---|---|
| `T_write` | 2 s | must be less than `T_owner` | Owner-side: a write that has not left for the client's socket in this long is `FlowRejected`. Far above any healthy write and far below the caller's patience, so a stalled client produces a specific answer rather than a timeout |
| `T_owner` | 4 s | greater than `T_write`, less than Timer B | Caller-side: armed when the delivery is issued, cancelled by the first response of any class (AN1's `100` is the one that always arrives), and on expiry `OwnerUnreachable`. It bounds **delivery, not the call** — after `100` the ordinary transaction timers own the request, so a client that rings for a minute is not this timer's business |
| `T_peer_retry` | 1 s | greater than zero | Caller-side: the minimum interval between dial attempts to one peer (BQ4) |
| `max_pending_per_flow` | 8 | at least 1 | Owner-side: pending delivery writes held for one connection before BQ2 refuses |

**Why `T_owner` needs `100` to exist.** Without a positive delivery signal the caller would have to
guess, and the guess fails in the direction that breaks at-most-once: a device that takes five
seconds to ring would time the caller out, the target would be removed, the next contact would be
tried — and the first device would still be ringing, delivered. `100` costs one message on a channel
that is already open and converts that guess into a fact.

## 9. At-most-once, and what it costs

| # | Rule |
|---|---|
| AM1 | **The property, stated exactly:** one delivery causes the request to be written to the named connection **at most once**. Not at least once, and not exactly once — exactly-once delivery over a network that can lose the acknowledgement is not available at any price, and a design that claims it has hidden a retry somewhere. |
| AM2 | **The caller never retries a delivery.** Not after `OwnerUnreachable`, not after `FlowRejected`, not after a dropped channel, and not with different bytes. A caller that failed cannot distinguish "it never arrived" from "it arrived, was written to the client, and the answer was lost", and a retry is precisely the second case turned into a duplicate. What the caller does instead is what [affinity-token](affinity-token.md) §13.2 already says: remove the target and let [proxy-behavior](proxy-behavior.md) §7.1's queue offer the next one, or conclude `480` by D8. |
| AM3 | **The channel does not retransmit either.** CH1's TLS-over-TCP is a reliable stream, so the client transaction toward the owner runs no RFC 3261 Timer A or E: at-most-once is a property of the transport choice rather than a rule layered on top of it. **UDP between nodes is not permitted**, and this is the sharpest reason why. |
| AM4 | **The owner writes at most once per received request** (OW7). Every other path through §6 writes nothing at all and answers instead; there is no partial-write retry and no second attempt on a different row. |
| AM5 | **The honest cost, recorded rather than argued away.** Under a failure this hop *loses* requests: a delivery that was written but whose answer was lost is reported to the caller as undelivered, the target is removed, and the request may be answered `480` while the client is ringing. That is a real, observable defect, and it is chosen over the alternative — a retry that rings a device twice, forks a call the caller already answered, or writes a stale request into a socket the client has reused. SIP is built to survive a lost request; nothing in it is built to survive a request it never sent arriving twice. The residual is bounded by `T_owner` and is measured under the harness's fault schedules (§10). |
| AM6 | **At-most-once is per request, not per flow or per dialog.** A CANCEL (CR6), an in-dialog request and a retransmission each cross the hop as their own delivery with their own answer. Nothing here deduplicates across deliveries, and nothing here is idempotent in the sense that would let it. |

## 10. What this specifies and does not execute

**This document registers no vector prefix, and the reason is sequencing rather than modesty.** Its
rules are not message-in / effects-out transformations of a record — they are the behaviour of two
nodes, a channel and a connection table under faults, so what executes them is a **harness
scenario** rather than a vector row. The implementation those scenarios run against is `AF-7`'s, and
this repository's rule is that a row and the test that executes it arrive in the same commit: a row
written earlier lands in the deferral ledger with a story attached, which is the shape `CF-8` and
`EX-12` both paid for. The scenarios below are therefore named — named, so that a scenario missing
from `AF-7` is missing by name rather than by omission — and they are named as test functions
because a test name is a claim that deleting the test deletes.

| Scenario | Fault schedule | The claim it fixes |
|---|---|---|
| `owner_rpc_delivers_cross_node` | none | Three nodes: a caller edge, an owner edge, a client on a stream link to the owner. The request reaches the client exactly once, the caller observes `100` then the client's response, and the cross-node dialog-lookup counter is zero |
| `owner_rpc_flow_dead_on_reconnect` | client reconnects before the delivery | The slot is reused with a bumped generation (RS3); the owner answers cause 1; the caller removes the target and the context concludes `480` — not `503`, not `408`, and not the owner's `480` relayed |
| `owner_rpc_flow_dead_on_owner_restart` | owner restarts with a new incarnation | RS1. Same caller-side consequence as above, reached by a different fact — this is the row that would pass if `incarnation` were dropped from the reference |
| `owner_rpc_flow_dead_leaves_binding_alone` | as the reconnect row | The location binding is **not** invalidated by a dead flow ([affinity-token](affinity-token.md) §13.2); the next refresh replaces the reference |
| `owner_rpc_rejected_when_flow_queue_full` | client stops reading | `max_pending_per_flow` overflows; the owner answers cause 2; the caller treats it as a branch failure and R8 turns a best-response `503` into `500` |
| `owner_rpc_rejected_when_write_stalls` | client stops reading, no overflow | `T_write` elapses; cause 2 again, reached by the timer rather than by the bound — the two paths must not be one row, because a bound that is never hit and a timer that never fires look identical in a report |
| `owner_rpc_unreachable_under_partition` | partition caller ↔ owner | `T_owner` fires; `OwnerUnreachable`; the target is removed and the context concludes `480` **and terminates** — the no-hang half of `AF-7`'s acceptance |
| `owner_rpc_unreachable_on_transport_error` | peer link breaks mid-delivery | The §16.9 error on a peer-channel branch is `OwnerUnreachable`, **not** R10's `503` (AN6). This is the row that fails if the narrowing is not implemented, and it fails by producing a `500` upstream |
| `owner_rpc_at_most_once_when_answer_is_lost` | link breaks after the write, before the answer | The client received the request **exactly once**; the caller reported it undelivered; no re-dial and no second write occurred. AM5's cost, asserted rather than described |
| `owner_rpc_outcomes_are_distinct_in_the_trace` | all three, in one run | The three failures are separately observable — the taxonomy exists to be told apart, and a report that counted them together would satisfy every row above |
| `owner_rpc_dial_is_suppressed_after_failure` | owner down, repeated attempts | `T_peer_retry` holds; N call attempts produce one dial, not N (BQ4) |
| `owner_rpc_flow_route_refused_from_a_client` | none | A flow-reference Route arriving on an ordinary listener is `403` (AU8), and the peer endpoint is absent from the node's identity set (CR5) |
| `owner_rpc_cancel_crosses_the_hop` | none | A CANCEL carries the same Route (CR6), reaches the flow, and the INVITE branch settles `487` |

The channel's own properties — the TLS floor (CH2), the certificate identity checks (AU2, AU3) —
are **not** in that list and are not harness work: the harness executes decisions, not handshakes
(AGENTS.md rule 2), and a simulated TLS stack would prove a simulation. They are proved where a
real socket exists, in the end-to-end path.

## 11. What is deliberately not expressible

| Not expressible | Why, and where it belongs |
|---|---|
| A retry, a redelivery or a pending-delivery queue at the caller | AM2, BQ1. The one mechanism that turns at-most-once into at-least-once |
| Store-and-forward, or any persistence on this path | BQ1. A request that outlives the transaction that wanted it is a request nobody is waiting for |
| A plaintext or optionally-authenticated peer channel | CH1, RP5. The switch would be found in the incident it exists for |
| An application-layer credential, challenge or nonce on this hop | AU1, and `CX-5` for what that shape costs |
| A `keys[]` entry used as a channel credential | AU5. Key separation, and a group secret cannot name a peer |
| A key request, push or exchange between nodes | [cluster-membership](cluster-membership.md) §4 KY9, §8 UQ3, §10. Unchanged by this hop and not weakened by AU6 |
| A directory, registry or membership query that answers *who owns this flow* | RP3, AGENTS.md rule 5. The reference carries its owner |
| A fallback destination when a reference does not resolve | OW8. "Or to nothing at all" is the invariant, and helpfulness is how it would be lost |
| A Record-Route inserted by the owner | CR4. It would pin the dialog to the one node this design keeps replaceable |
| A fifth delivery outcome | RP4, AN9. The taxonomy is [affinity-token](affinity-token.md) §13.2's and this document carries it |
| `430 Flow Failed` before M3 | AN4, AN8, [affinity-token](affinity-token.md) §13.2 D9 |
| Anything on this endpoint that is not a delivery | RP1. Configuration, health, statistics and shard handoff each already have a home |

## 12. Consequences for documents this spec does not own

Named so they are tracked rather than discovered. **None is performed by this story**, which writes
this document and its design record only.

| Where | What must change, and why |
|---|---|
| [affinity-token](affinity-token.md) §1 "Out of scope" | It defers "the owner RPC's transport, node-to-node authentication and queueing" to `AF-3`. The pointer moves to this document; no rule moves, and §13.2's taxonomy is carried here unchanged |
| [affinity-token](affinity-token.md) §13.1 D5 | It names `AF-3` as where the hop is specified and can name this document instead. D5 itself is unchanged and is quoted here rather than restated |
| [proxy-behavior](proxy-behavior.md) §8 R10 | AN6 narrows it: a transport error on a **peer-channel** branch is `OwnerUnreachable` and removes the target, rather than behaving as `503` from that branch. R10 is unchanged for every other branch, and the narrowing belongs beside it as one clause so that a reader of R10 is not surprised by this document |
| [proxy-behavior](proxy-behavior.md) §5 P2 | P2 pops a first Route that resolves to this platform and verifies an *affinity token*. The flow-reference case is OW2, and its recognition is narrower than P2's on purpose (CR5, AU8): only the owning node, only on the peer listener |
| [cluster-config](cluster-config.md) §5 P5 | The identity set is the union of the advertised addresses of projected listeners, which is what lets any edge pop any edge's Route. CR5 requires the peer endpoint to be **excluded** from it; without that exclusion AU8's refusal is reachable around |
| [cluster-config](cluster-config.md) §7 | §8's four values (`T_write`, `T_owner`, `T_peer_retry`, `max_pending_per_flow`) need a registry row with an owner and a reload class — `reloadable`, since none of them binds a listener or invalidates a record. The spelling is that spec's; the values and their bounds are fixed here |
| [cluster-config](cluster-config.md) §5, listeners | CH4's peer listener is not a `listener[]` entry and its bind address is a start-up input beside `NodeIdentity`. That is a schema statement about a schema this document does not own, and it is [cluster-membership](cluster-membership.md) MB7's "the bind side is not here" landing somewhere at last |
| [cluster-membership](cluster-membership.md) §3 MB5 | MB5 requires `rpc` whenever `roles` intersects `{edge, registrar, inbound-proxy, outbound-proxy}`. The property that actually needs an endpoint is "this node may own a flow", which by [affinity-token](affinity-token.md) §11.4 FM6 means "this node accepts a connection-oriented transport" — a **listener** fact, so a UDP-only proxy is made to declare an endpoint nothing will ever dial. The document structurally cannot check the precise property: it is cluster-scoped and carries no member's listener set (CM2, MB7), and deriving identity from it is forbidden ([cluster-config](cluster-config.md) §5 P1). What **is** checkable is the same rule moved to the node that knows: a node whose own projected listeners include a connection-oriented transport and whose own member entry declares no `rpc` refuses to start, which is MB2's cross-check shape applied to a field MB2 does not currently cover. Recorded here because this document defines the endpoint; the amendment is that spec's |
| `deploy/helm/` | A chart that renders `membership[].rpc` must also render the peer listener's bind input (CH4) and the peer channel's TLS material (AU4), or a node with an `rpc` entry has an endpoint nobody can reach |
