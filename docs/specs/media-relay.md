# Spec: Media relay control

**Status:** normative · **Crate:** _future — the trait lands with `ME-2`_ · **Stories:** ME-1
(this spec), ME-2 … ME-5, KO-7 · **Design:**
[media-control](../designs/media-control.md) · **Vector prefix:** `MR`

The platform's only view of media is a five-method port, `MediaRelay`. This spec fixes that port's
semantics, the pass-through implementation every M1 test runs on, and the byte-level contract of
the first real implementation — the NG control protocol spoken to an rtpengine media node.

## 1. Normative references

- **RFC 3264** — the offer/answer model with SDP. Fixes what `offer`, `answer` and a re-offer
  mean, and (§3) that an offer belongs to the agent that generated it — the rule behind O4. §6: a
  stream whose offered formats are unacceptable is rejected by setting its port to zero — the
  outcome MP3 makes visible rather than papers over.
- **RFC 8866** — SDP (obsoletes RFC 4566). The body this port carries; its tolerance for
  attributes an intermediary does not understand is why §3.2 O3 forbids re-serializing one.
- **RFC 6337** — SIP usage of the offer/answer model: where in a dialog an offer may legally
  appear. Fixes the preconditions of `update` (§3.2 U2).
- **RFC 3261** — §12.1.1 (a remote tag does not exist before the first tagged response; O2),
  §13.3.1.4 (a UAS retransmits the 2xx until the ACK arrives; A3), §14.1 (re-INVITE), §16.10 and
  §16.8 (CANCEL and Timer C; D3), §17.1.1.2 (Timer A/B — the doubling retransmission schedule and
  `64·T1`; §8), §17.2.1 (`100 Trying` within 200 ms; O6), §19.1.4 and §7.3.1 (Call-ID and tags are
  compared as opaque byte tokens; E2), §21.4.26 and §21.5.1 (the statuses §9 maps onto).
- **RFC 6141** — a failed re-INVITE leaves the session as it was. Behind U3 and state row S9.
- **RFC 3311** — the UPDATE method: a mid-dialog offer that is not a re-INVITE (U2).
- **RFC 8445** §12 — ICE nominates the highest-priority working candidate pair, which is why an
  anchored call may not offer the relay as a low-priority candidate (§7.3).
- **RFC 5761** — multiplexing RTP and RTCP on one port (the `rtcp-mux` key, §7.2).
- **RFC 3711** — SRTP, and the `RTP/SAVP` profile name. What "this leg is encrypted" means, and
  the subject of §13's `SrtpPolicy`. §9.5 — the authentication tag length, which is the whole
  difference between §13.1's two crypto-suite sets.
- **RFC 4568** — SDES: keying material carried in the description itself (`a=crypto`), and §6.2,
  the two SRTP crypto suites §13.1 admits. §7.2 requires that a description carrying a key be
  protected in transit, which is why MP7 is a startup error rather than a warning.
- **RFC 5764** and **RFC 5763** — DTLS-SRTP: keys negotiated in the media path, with only a
  certificate fingerprint in the description, and its SIP/SDP framing. RFC 5764 §8 names the
  `UDP/TLS/RTP/SAVP` profile §13.3 sends.
- **RFC 8122** — the `a=fingerprint` attribute DTLS-SRTP's integrity rests on, and RFC 4145 §4.1 —
  the `setup` attribute, whose passive and active roles are what §13.1's `DtlsRole` selects
  between.
- **RFC 3551** — the `RTP/AVP` profile name, the third and last member of §13.3's closed
  `transport-protocol` set. The feedback profiles (`RTP/AVPF`, `RTP/SAVPF`) are deliberately not
  in it: nothing in version 1 decides whether a leg carries RTCP feedback, and a policy that
  cannot decide something must not assert it.
- **RFC 3550** §6.4 — RTCP reception statistics; the shape of what `query` reports (§3.2 Q).
- **RFC 768** — UDP. Unreliable, unordered, undelivered without notice, and bounded by one
  datagram: the three facts §6.3 D10, §8 and §9 are built on.
- **RFC 2606** §2, **RFC 5737** and **RFC 3849** — the reserved name and address blocks §12 uses,
  so no vector names a real host.
- **BEP 3** — the bencode grammar (`i…e`, `l…e`, `d…e`, length-prefixed byte strings) and its
  requirement that dictionary keys appear in lexicographic order, which §6.3 E3 adopts for this
  platform's encoder.
- **Our specs consumed by / consuming this one:** [affinity-token](affinity-token.md) §3 (the
  `media node` and `edge affinity` fields — where a chosen node id rides, and where §6.2's cookie
  prefix comes from) and §7 (the route set is fixed at dialog establishment, so a token cannot be
  refreshed mid-dialog — behind §10's selection rule);
  [hook-framework](hook-framework.md) §3 (the phases a media module registers at; `ME-4` picks
  which); [proxy-behavior](proxy-behavior.md) §7 and §9 (forwarding, CANCEL and Timer C — the
  events that drive `delete`, D3).
- **Integration target:** the rtpengine NG control protocol, at the baseline version §11 names.
  Per the AGENTS.md integration carve-out it is named here as a system this platform *talks to*.
  No rule in this spec is justified by how it or any other system behaves; every rationale below
  cites an RFC, a spec in this directory, or a property of UDP.

## 2. Scope, and the upstream boundary

| Here | Not here |
|---|---|
| the `MediaRelay` port and its five methods (§3) | which calls are anchored at all (`ME-4`) |
| the media-session state machine (§3.3) | which node is chosen, and reselection (`ME-3`) |
| `NullMediaRelay` (§4) | the hook phases a media module registers at (hook-framework §3) |
| the NG wire contract: framing, cookies, encoding (§6) | how the node id is carried (affinity-token §3) |
| the command mapping and the keys sent (§7) | pool membership, host networking, port ranges (`KO-7`) |
| retransmission budget and timers (§8) | media transport itself — the platform never touches a packet |
| the error taxonomy and health model (§9, §10) | SDP semantics: nothing here reads the body |
| the per-trunk codec and SRTP policy, and the keys it derives (§13) | where a trunk object lives, and how it is configured and versioned (`RT-2`) |
| that a carrier quirk profile may *constrain* media policy and never set it (§13.6) | the quirk-profile vocabulary itself (`EX-7`) |

**Upstream considerations** (AGENTS.md rule 6): considered for upstream — **no**. `MediaRelay`
and its NG binding stay in sipx-clstr, because both are platform orchestration rather than SIP
protocol: the port's vocabulary is cluster media anchoring (media-node ids, pool membership,
tenant call classes) and the NG protocol is one integration target's control protocol, so a
kernel carrying it would ship a client for a relay it never talks to. The one piece that *would*
be protocol-generic — an SDP model — is deliberately not required: O3 keeps the body opaque bytes
end to end, so neither repository needs an SDP parser for `ME-2`. If a later story must *read* the
body (`ME-4`'s latching decision may), that parser is protocol-generic and becomes a row in the
[upstream ledger](../upstream.md) at that point, not now.

**Upstream considerations for §13** (`ME-6`, same rule): considered for upstream — **no**.
Per-trunk codec and SRTP policy is orchestration in the strict sense: its subject is a *trunk*, an
object the kernel does not have and should not grow, and its whole output is a set of NG control
keys for a relay the kernel never talks to. What is protocol-generic nearby — an SDP model, and
RFC 3264 offer/answer bookkeeping — is again not required, because MP1 makes the derivation a
function of declared policy alone and MP10 forbids reading the body to decide *or* to verify. The
day a policy must be chosen by inspecting what a peer offered, that inspection needs a parser, and
that parser is a row in the [upstream ledger](../upstream.md) — the same row §2 already reserves
for `ME-4`, not a second one.

## 3. The `MediaRelay` port

### 3.1 Types

`MediaRelay` is a **driver-layer port**: a call to it crosses a network. That does not weaken
AGENTS.md rule 2, because the parts that decide anything are pure — the anchoring decision belongs
to the module (`ME-4`/`ME-5`), and the adapter's own decisions (encode, decode, retransmit, judge
a node) are the sans-IO core of §5. What the driver owns is a socket.

```rust
pub struct MediaSessionKey {
    pub tenant: TenantId,
    pub call_id: Bytes,          // byte-exact, case-sensitive (RFC 3261 §19.1.4)
    pub from_tag: Bytes,         // the tag of the party whose SDP this is — O4
    pub to_tag: Option<Bytes>,   // absent on the initial offer only — O2
    pub via_branch: Option<Bytes>,
}

pub struct OfferRequest {
    pub key: MediaSessionKey,
    pub node: MediaNodeId,       // chosen by ME-3; rides in the affinity token
    pub sdp: Bytes,              // opaque, verbatim — O3
    pub source: SourceAddr,      // the observed source of the SIP message
    pub interfaces: Option<(InterfaceName, InterfaceName)>,  // initial offer only — §7.2
    pub ice: IcePolicy,
    pub class: CallClass,        // the tenant's per-call-class media policy (ME-4)
    pub toward: TrunkMediaPolicy,// the declared policy of the peer that receives the returned
                                 // SDP — §13, MP4. Not the caller's, not the call's
}

pub struct AnswerRequest { /* as OfferRequest, `to_tag` required, no `interfaces` */ }
pub struct DeleteRequest { pub key: MediaSessionKey, pub node: MediaNodeId }
pub struct QueryRequest  { pub key: MediaSessionKey, pub node: MediaNodeId }

pub enum IcePolicy { Anchored, Stripped }   // §7.3; there is no pass-through option

pub struct RelayedSdp { pub sdp: Bytes, pub warning: Option<Bytes> }

pub struct SessionStats {
    pub created: u32,            // UNIX seconds, as reported
    pub last_signal: u32,
    pub totals: RtpTotals,       // RFC 3550 §6.4 shape: packets, bytes, errors, per RTP and RTCP
    pub parties: Vec<PartyStats>,
}

pub enum MediaError {
    /// Nothing answered inside the §8 budget, or the path said the port is closed.
    NodeUnreachable { node: MediaNodeId, attempts: u8, waited: Duration },
    /// The node answered, and answered that it is full — §9, the `load limit` contract.
    NodeOverloaded  { node: MediaNodeId, message: Option<Bytes> },
    /// The node answered, and answered "no" to *this command*. Portable across nodes.
    Rejected        { node: MediaNodeId, reason: Bytes },
    /// A reply arrived and could not be believed — §6.3.
    Malformed       { node: MediaNodeId, fault: DecodeFault },
    /// `delete`/`query` for a session the node does not hold. Not a failure — D2, Q3.
    NoSuchSession   { node: MediaNodeId },
}

pub trait MediaRelay {
    fn offer(&self,  req: OfferRequest,  now: Instant) -> Result<RelayedSdp, MediaError>;
    fn answer(&self, req: AnswerRequest, now: Instant) -> Result<RelayedSdp, MediaError>;
    fn update(&self, req: OfferRequest,  now: Instant) -> Result<RelayedSdp, MediaError>;
    fn delete(&self, req: DeleteRequest, now: Instant) -> Result<Option<SessionStats>, MediaError>;
    fn query(&self,  req: QueryRequest,  now: Instant) -> Result<SessionStats, MediaError>;
}
```

Time is an argument, never a clock read, so the harness drives every method in virtual time. The
signatures are written synchronously for readability; `ME-2` may make them `async` without
changing a rule here.

### 3.2 Method semantics

Normative. Rule ids are referenced from the state table (§3.3), the mapping (§7) and the vectors
(§12).

**`offer` — O.**

- **O1 — identity is the key, not a handle.** A session is named by
  `(tenant, call-id, from-tag, to-tag?)`; the platform MUST NOT mint a session identifier of its
  own. Two `offer`s bearing the same key are the same session: the second re-confirms the existing
  anchor and MUST NOT be counted, billed or ported as a new one.
- **O2 — the initial offer carries no `to_tag`.** A dialog has no remote tag until the first
  tagged response (RFC 3261 §12.1.1); supplying a placeholder splits one session into two on the
  node.
- **O3 — SDP is opaque, in both directions.** Between reading the body off the wire and handing it
  to `offer`, the platform MUST NOT parse, normalize, reorder, re-indent or re-serialize it, and
  the bytes returned in `RelayedSdp::sdp` replace the body verbatim. RFC 8866 lets a session
  description carry lines an intermediary does not understand, and the relay's rewrite is
  self-consistent over the exact bytes it produced — a round trip through a parser can reorder
  attributes or drop an unknown one and invalidate a fingerprint or key the relay just computed.
  This rule is also what keeps an SDP model out of both repositories (§2).
- **O4 — `from_tag` is the tag of the party whose SDP this is**, not the dialog's initiator. When
  the callee re-offers, the two differ; supplying the dialog's original From tag makes the node
  rewrite the wrong side of the call, which fails as one-way media rather than as an error.
- **O5 — every successful `offer` is followed by exactly one `answer` or one `delete`.** An offer
  that is neither answered nor deleted is a port set held until the node's own timeout; the
  node's timeout is a backstop, not the design (D3 lists the triggers).
- **O6 — `100 Trying` first.** For an INVITE, the proxy MUST have emitted the provisional response
  before the first NG request goes out (RFC 3261 §17.2.1). This is what lets §8's budget exceed
  `T1` without provoking a retransmitted INVITE, and it costs nothing: the response does not
  depend on the media outcome.

**`answer` — A.**

- **A1** `to_tag` is REQUIRED.
- **A2** `answer` is the only method that completes a pending offer; the returned SDP replaces the
  body travelling back toward the offerer.
- **A3 — re-answering is safe and MUST NOT be suppressed.** A UAS retransmits its 2xx until the
  ACK arrives (RFC 3261 §13.3.1.4), and a proxy forwards those retransmissions. Issuing `answer`
  again with the same key and the same body is idempotent at the node and returns the same
  allocation; the adapter does not deduplicate, because a deduplication cache is state on the
  mid-dialog path (AGENTS.md rule 5) bought to save a datagram on a private network.

**`update` — U.**

- **U1 — there is no `update` command on the wire.** The method exists because callers must
  distinguish a dialog's first offer from a mid-dialog re-offer — their preconditions differ (U2)
  and their failure handling differs sharply (U3) — but §7 maps it onto `offer`. Pinning the
  asymmetry is the point of writing it down: an adapter that invented an `update` command would be
  talking to nothing.
- **U2** `update` requires the session `Established` (§3.3) and both tags present. It covers a
  re-INVITE (RFC 3261 §14.1), an UPDATE (RFC 3311), and an offer carried in a reliable provisional
  exchange (RFC 6337 §3.2).
- **U3 — a failed re-offer MUST NOT tear media down.** On any `MediaError` from `update`, or from
  the `answer` that completes it, the previous negotiation stands and the platform MUST NOT issue
  `delete`. A failed re-INVITE leaves the session as it was (RFC 6141); deleting the anchor would
  convert a rejected media change into a dropped call.
- **U4** `interfaces` is not sent on a re-offer — the node holds the pair chosen at the initial
  offer (§7.2), and re-sending it invites a mid-call interface change nobody asked for.

**`delete` — D.**

- **D1** Exactly once per session, at dialog termination.
- **D2 — idempotent and non-fatal.** Deleting a session the node does not hold is
  `Ok(None)` — not an error. The adapter MUST NOT ask for fatal handling of that case: a
  retransmitted BYE, a `delete` racing the node's own expiry, and a re-anchored call all produce
  it, and none of them is a fault. A recorded warning is the whole response.
- **D3 — the triggers.** BYE in either direction; any non-2xx final response to an anchored
  INVITE; CANCEL; Timer C expiry (proxy-behavior §9); and abandonment of the transaction that
  carried the offer. Missing one of these leaks ports at exactly the rate the deployment forgets
  about them.
- **D4 — no per-call teardown delay is sent**, so the node's configured value applies. Media and
  BYE retransmissions arrive after the last signalling message, and a zero-delay teardown
  discards them; the delay is a deployment property (`KO-7`), not a per-call one.
- **D5** Whatever statistics the reply carries are returned; a reply carrying none is `Ok(None)`.
  Statistics are telemetry and MUST NOT feed any control decision — §10's health model reads
  outcomes, never counters.

**`query` — Q.**

- **Q1** Read-only: `query` never changes a session.
- **Q2 — never on the signalling path.** `query` MUST NOT be called while handling a request or
  response. It exists for operations: `KO-7`'s drain check, `ME-3`'s reselection audit, and
  support. A mid-dialog lookup against a media node is precisely the shared dependency the
  affinity token exists to remove (AGENTS.md rule 5).
- **Q3** A `query` for a session the node does not hold is `NoSuchSession`, and a drain check reads
  that as zero rather than as a failure.
- **Q4** The count of sessions anchored on a node — what draining waits for — comes from the
  platform's own ledger, not from `query`. An external pool may serve controllers this platform
  knows nothing about, and draining *our* sessions is the only thing we can honestly promise.

### 3.3 The media-session state table

Normative. States: `Absent`, `Offered`, `Established`, `Reoffered`, `Gone`.

| # | State | Event | Effect | Next |
|---|---|---|---|---|
| S1 | Absent | `offer` → ok | the node id is recorded and minted into the affinity token's `media node` field (affinity-token §3) | Offered |
| S2 | Absent | `offer` → `Rejected` or `Malformed` | no session exists; `ME-4`'s policy decides between failing the call and proceeding media-direct | Absent |
| S3 | Absent | `offer` → `NodeUnreachable` or `NodeOverloaded` | `ME-3` reselects within the §8 `B_media` budget; on exhaustion, S2's policy | Absent |
| S4 | Offered | `answer` → ok | the anchor is complete in both directions | Established |
| S5 | Offered | `answer` → any error | the offer's allocation is still held; `delete` it (D3) | Gone |
| S6 | Offered | non-2xx final, CANCEL, or Timer C | `delete` (D3) | Gone |
| S7 | Established | `update` → ok | | Reoffered |
| S8 | Reoffered | `answer` → ok | | Established |
| S9 | Reoffered | `update` or `answer` → any error | the previous negotiation stands; **no** `delete` (U3) | Established |
| S10 | Established or Reoffered | BYE in either direction | `delete` | Gone |
| S11 | Gone | `delete` again — a retransmitted BYE | idempotent; `Ok(None)` (D2) | Gone |
| S12 | any | the session's node becomes `Down` (§10) | media re-anchors on the **next** offer or answer only. A call sitting in `Established` has no media until then, and none is restored retroactively: this is re-anchoring, not failover, and no packet-level continuity is claimed (design, *Risks*) | Absent |

## 4. `NullMediaRelay`

The default. M1 runs entirely on it, and so does every deployment whose media is direct.

- **N1** `offer`, `answer` and `update` return `RelayedSdp` whose `sdp` is **byte-identical** to the
  input and whose `warning` is `None`. Not "equivalent" — identical: N1 is O3 with the relay
  removed, and a null implementation that normalizes a body would hide the class of defect O3
  exists to prevent.
- **N2** `delete` returns `Ok(None)`. `query` returns `Err(NoSuchSession)`, because a relay that
  anchors nothing holds no session and reporting zeroed statistics would be an invented fact; Q3
  already requires the drain check to read that as zero.
- **N3** The null relay allocates no node: the affinity token's `media node` field stays `0`
  (affinity-token §3). A token minted on a media-direct call MUST NOT name a node.
- **N4** It is indistinguishable at the SIP layer from having no media control at all — no header,
  no body change, no timer armed, no retransmission, no NG datagram, and no failure mode. Its only
  observable is a pass-through counter.
- **N5** It is a pure function, so it is the harness default and every M1 vector in every other
  spec holds under it unchanged.
- **N6** It never returns `NodeUnreachable`, `NodeOverloaded`, `Rejected` or `Malformed`. A caller
  written against `MediaRelay` must still handle those, and `ME-2`'s scripted fake — not the null
  relay — is what exercises them under the harness.

## 5. The sans-IO core

Everything in §6 through §10 is a pure function or a state machine driven by fired timers, so the
whole contract runs under the deterministic harness with no media node present, and `CF-3` then
replays the same vectors against a real one.

```rust
enum NgInput  { Command(NgRequest, Instant), Datagram(Bytes, Instant), Timer(TimerId, Instant) }
enum NgOutput { Transmit(MediaNodeId, Bytes), SetTimer(TimerId, Instant), ClearTimer(TimerId),
                Complete(ExchangeId, Result<NgReply, MediaError>) }

fn encode(req: &NgRequest, cookie: &Cookie) -> Bytes;                  // §6.3, total
fn decode(datagram: &[u8]) -> Result<(Cookie, NgReply), DecodeFault>;  // §6.3, total

struct NgExchange;   // one command, its retransmissions, its outcome
impl NgExchange { fn step(&mut self, input: NgInput) -> Vec<NgOutput>; }

struct PoolHealth;   // §10; inputs are exchange outcomes and fired probe timers
```

`decode` is total: every malformed input yields a `DecodeFault`. It MUST NOT panic, index rawly,
or reserve memory from a declared length before that many bytes have been received (AGENTS.md
rule 3) — a datagram of forty bytes announcing a string of nine hundred million must cost forty
bytes of work.

## 6. NG transport binding

### 6.1 Framing

```
message     = cookie SP body
cookie      = 1*32 cookie-char
cookie-char = %x21-7E                 ; printable ASCII, excluding SP
body        = <one bencode dictionary, §6.3>
```

- **F1** One message per UDP datagram. There is no length prefix and no terminator; the
  dictionary's closing `e` ends the message, and any byte after it is a `DecodeFault`.
- **F2** A reply carries the cookie of the request it answers, and correlation is by cookie
  alone (§6.2).
- **F3** A well-formed reply whose cookie matches no in-flight exchange is discarded silently. It
  is not an error and MUST NOT move node health (§10) — a late reply to an exchange that already
  gave up is the expected consequence of §8's budget.
- **F4** The transport is UDP; §8's budget exists because of RFC 768. The stream transports the
  integration target also offers are out of scope for version 1; adopting one amends §8 and
  nothing else.
- **F5** The control endpoint is `(host, UDP port)` from configuration. This spec fixes no default
  port: the node's listener is a deployment property (`KO-7`).

### 6.2 The cookie

```
cookie = 4HEXDIG "_" 1*16HEXDIG       ; lowercase hex
```

The first field is the edge's logical id — the same value the affinity token carries at offset 20
(affinity-token §3). The second is a per-process counter, **seeded** from the injected randomness
source at startup and incremented by one per command.

- **C1 — one cookie per command, not per transmission.** All four transmissions of §8's schedule
  carry byte-identical bytes, cookie included. Correlation and duplicate detection both key on the
  cookie, so reusing it is what makes a retransmitted `offer` return the first reply instead of
  allocating a second port set (MR-X-2). A fresh cookie per transmission would turn every lost
  reply into a leaked allocation.
- **C2 — unique across the cluster**, for at least as long as a node remembers a cookie. Two edges
  MUST NOT be able to produce the same cookie; the edge-id prefix is what guarantees it, and it is
  why the cookie is not simply a counter.
- **C3 — seeded, not zero-based.** An edge that restarted and counted from zero would re-spend
  cookies its previous life already used, and the node would answer a *new* command from its cache
  for an old one. Drawing the start from the injected source makes that vanishingly unlikely in
  production and exactly reproducible under the harness.
- **C4** The cookie is not a secret and conveys no authority. The control interface is a private
  network, asserted by `KO-7`, and nothing in this spec treats a cookie as authentication.

### 6.3 Canonical bencode

**Encoder — E.**

- **E1** BEP 3 grammar: `i<int>e`, `<len>:<bytes>`, `l…e`, `d…e`. Integers and lengths are decimal
  ASCII with no leading zeros and no sign except a leading `-` on a negative integer; `i-0e` is
  invalid.
- **E2** Strings are **raw bytes**. Call-ID, tags, branch and SDP go on the wire byte-exact — no
  UTF-8 validation, no escaping, no case folding, no trimming. RFC 3261 §7.3.1 and §19.1.4 make
  these values byte-compared tokens, and the body is a body.
- **E3 — dictionary keys are emitted in ascending raw-byte order.** BEP 3 requires it. This
  platform's encoder does it unconditionally, because sorted keys make the encoding a *function*
  of the value — which is what lets §12 pin bytes and lets `ME-2` assert byte equality rather than
  semantic equivalence. The order is over raw bytes, so SP (`0x20`) sorts before `-` (`0x2D`) and
  uppercase before lowercase: `ICE` precedes `call-id`, and `received from` precedes `replace`.
- **E4** No key appears twice. An option that is not being exercised is **absent**, never present
  and empty.
- **E5** The encoder emits only the keys §7 lists. Sending anything else requires amending §7
  first — a spec change, not a configuration flag, because §12's bytes are the test suite.

**Decoder — D.**

- **D6** Keys may arrive in any order and MUST NOT be assumed sorted. E3 binds this platform's
  encoder; it binds nothing on the far end.
- **D7 — unknown keys are ignored.** This is the whole version-drift contract (§11): a newer node
  that adds reply keys must not break an older adapter, and an adapter that rejected them would
  make every upgrade a coordinated outage.
- **D8** A `DecodeFault` is raised, and the datagram discarded, on: a non-dictionary top level; a
  duplicate key; a non-canonical integer or length (E1); a declared length exceeding the remaining
  datagram; a truncated value; trailing bytes after the top-level dictionary (F1); or nesting
  deeper than **16** levels. The deepest structure the baseline produces is a `query` reply at
  seven, so 16 is headroom rather than a limit anyone meets.
- **D9** The decoder allocates only from bytes already received (§5).
- **D10** A reply that does not fit in one datagram cannot be received at all — UDP offers no
  reassembly of its own. Version 1 therefore issues no command whose reply is unbounded (§7.5),
  and this is a second reason `query` is kept off the signalling path (Q2): its reply grows with
  the call.
- **D11** The timestamp key of a `query` reply is accepted spelled either `last signal` or
  `last_signal`. The integration target's reference documentation uses both spellings, so a
  decoder that insisted on one would fail against a node that is behaving correctly.

## 7. Command mapping

### 7.1 The command set

Version 1 sends these commands and no others.

| Port method | NG `command` | Required keys | Optional keys sent | Success reply |
|---|---|---|---|---|
| `offer` (initial) | `offer` | `command`, `call-id`, `from-tag`, `sdp`, `transport-protocol`, `DTLS` (M4) | `ICE`, `SDES`, `codec`, `direction`, `received from`, `replace`, `supports`, `via-branch` | `result` = `ok`, plus `sdp` |
| `answer` | `answer` | `command`, `call-id`, `from-tag`, `to-tag`, `sdp`, `transport-protocol`, `DTLS` (M4) | `ICE`, `SDES`, `codec`, `received from`, `replace`, `supports`, `via-branch` | `result` = `ok`, plus `sdp` |
| `update` (re-offer) | `offer` | `command`, `call-id`, `from-tag`, `to-tag`, `sdp`, `transport-protocol`, `DTLS` (M4) | as `answer` — no `direction` (U4) | `result` = `ok`, plus `sdp` |
| `delete` | `delete` | `command`, `call-id`, `from-tag` | `to-tag`, `via-branch` | `result` = `ok`, optionally statistics and `warning` |
| `query` | `query` | `command`, `call-id` | `from-tag`, `to-tag` | `result` = `ok`, plus `created`, `last signal`, `tags`, `totals` |
| health probe (§10) | `ping` | `command` | — | `result` = `pong` |

- **M1** `update` maps to `offer` (U1). The three offer rows differ only in `to-tag` and
  `direction`, and that difference is the entire mapping.
- **M2** A success reply to `offer`, `answer` or `update` that carries no `sdp` is `Malformed`
  (§9), not a silent pass-through: proceeding with the endpoint's own address on a call believed
  to be anchored sends media somewhere nobody is listening.
- **M3** `result` values recognised: `ok`, `error`, `pong`, `load limit`. Any other value is
  `Malformed`.
- **M4 — four keys are derived from the receiving peer's declared trunk policy** (§13), and only
  from it: `transport-protocol`, `DTLS`, `SDES` and `codec`. The first two are sent on every
  `offer`, `answer` and re-offer without exception, including where the policy is "no SRTP", so
  that no leg's security depends on a node default (MP8a). The other two are sent when they
  restrict something and are absent otherwise, because an empty restriction has no encoding but
  absence (MP8b, E4). `delete`, `query` and `ping` carry none of the four — they negotiate
  nothing.

### 7.2 The keys, and why each is sent

| Key | Value | Rule |
|---|---|---|
| `call-id`, `from-tag`, `to-tag` | the session key's bytes | O1, O2, O4. Byte-exact per E2 |
| `sdp` | the body, verbatim | O3 |
| `via-branch` | the branch of the transaction carrying the body | sent when known; it lets the node distinguish forked branches of one INVITE, which the key alone cannot (proxy-behavior §7 forks) |
| `received from` | `["IP4", <source address>]` or `["IP6", …]` | the observed source of the SIP message. Sent because the address in the body may be behind a NAT and is not to be trusted; this is the platform's own observation, not the endpoint's claim |
| `direction` | the configured interface pair, initial `offer` only | U4. Names come from the node's configuration (`KO-7`/`DP-1`) |
| `replace` | `["origin", "SDP-version"]` | origin-address rewrite keeps the endpoint's address out of the body that leaves the platform; version control keeps the SDP version monotone across a rewrite, which RFC 8866 §5.2 requires a modified description to observe |
| `supports` | `["load limit"]` | REQUIRED on every `offer`, `answer` and `update`. This is what makes overload a distinguishable outcome instead of a rejection (§9); without it a full node is indistinguishable from a node that refused the request, and the reselection decision inverts |
| `ICE` | §7.3 | |
| `transport-protocol` | one of `RTP/AVP`, `RTP/SAVP`, `UDP/TLS/RTP/SAVP` | M4, MP8a. The receiving peer's declared SRTP mode, and nothing else (§13.3). The set is closed: RFC 3551, RFC 3711 and RFC 5764 §8 name all three |
| `SDES` | `["off"]`, or a one-entry suite restriction; absent when the policy neither forbids SDES nor excludes a suite | M4, MP8b, §13.3 |
| `DTLS` | `["off"]`, `["passive"]` or `["active"]` | M4, MP8a, §13.3. RFC 4145 §4.1 is what the two non-`off` values mean |
| `codec` | a dictionary over `strip`, `offer` and `transcode` | M4, MP8b. Sent only when the policy manipulates codecs; `offer`/`strip` filter, `transcode` adds, and only `transcode` adds (MP3) |

### 7.3 The ICE stance

An anchored call may not leave ICE untouched. Per RFC 8445 §12 the endpoints send media to the
nominated candidate pair, not to the address in `c=`/`m=`, so candidates that survive a rewrite
let ICE-capable endpoints negotiate around the relay — and an anchor the media does not traverse
is worse than no anchor, because the platform believes it is in the path.

- **I1** `IcePolicy::Anchored` sends `ICE: ["default"]`: existing ICE is replaced so the relay is
  the only candidate, and an offer that had no ICE gains none. Both halves are correct for an
  anchored call — there is nothing to negotiate around when the body names only the relay.
- **I2** `IcePolicy::Stripped` sends `ICE: ["remove"]`, for call classes that must not carry ICE
  at all.
- **I3** The value that leaves the relay as one candidate among the endpoints' own MUST NOT be
  sent for an anchored call: it is RFC 8445 §12 nomination working exactly as specified, against
  us. `IcePolicy` has no such variant, so this is unrepresentable rather than merely forbidden.
- **I4** The key is always sent explicitly, even where omitting it would mean the same thing
  today. An omitted key means whatever the node's default means at the version it happens to be
  running (§11); an explicit one means what this spec says.
- **I5** Media-direct calls never reach this adapter at all — they run on `NullMediaRelay` (§4),
  where I1–I4 are vacuous. The platform never *requires* anchoring (design, *Approach*).

### 7.4 What version 1 does not send

`ME-6` took up the invitation this section used to extend, and did it the way the section asked:
DTLS, SDES, transcoding, codec manipulation and transport-protocol rewriting **are** sent, as the
derivation of a declared per-trunk policy and only ever that (§13), and §7.1, §7.2 and §12 moved
with the bytes. The reason the original list existed is preserved rather than discarded — those
are decisions this spec is not entitled to make on a tenant's behalf, so it does not make them: it
fixes how a tenant's own declaration reaches the wire, and MP2 keeps every one of them off by
default.

Still unsent, and for the same original reason: recording, call-metadata and label keys, and any
`transport-protocol` value outside the closed set of §13.3. Adding one is a story that amends
§7.1, §7.2 and §12, so the bytes and the tests move together.

### 7.5 Commands version 1 does not use

Recording, DTMF and media blocking, forwarding, playback, publish/subscribe, statistics, call
listing and inter-node meshing are out of scope. Two are worth naming for a reason beyond scope:
call listing has an unbounded reply (D10), and statistics is not a control input (D5).

## 8. Timers and the retransmission budget

| Symbol | Default | What it is |
|---|---|---|
| `T_ng` | 150 ms | base retransmission interval for one NG request |
| `B_ng` | 1500 ms | budget for one exchange; expiry is `NodeUnreachable` |
| `B_media` | 3000 ms | total media budget for one INVITE, across any reselection (`ME-3`) |
| `T_ping` | 1000 ms | interval of the idle health probe, per node |
| `N_down` | 3 | consecutive unanswered probes before a node is `Down` |
| `N_up` | 2 | consecutive `pong`s before a `Down` node is `Up` again |
| `T_load` | 5000 ms | how long `Loaded` persists without a further signal |

The schedule for one exchange, from the moment the command is issued:

| Transmission | at | note |
|---|---|---|
| 1 | 0 | |
| 2 | `T_ng` = 150 ms | |
| 3 | `3·T_ng` = 450 ms | the interval doubled |
| 4 | `7·T_ng` = 1050 ms | doubled again |
| — | `B_ng` = 1500 ms | give up: `NodeUnreachable` with `attempts` = 4 |

- **K1** The doubling shape is RFC 3261 §17.1.1.2's unreliable-transport schedule, adopted because
  the failure it defends against here is the same one it defends against there — a lost datagram
  with no acknowledgement — and because a doubling schedule stops a node's slow patch from
  becoming the cluster's retransmission storm.
- **K2** Retransmissions are byte-identical, cookie included (C1).
- **K3 — the budget fits inside call setup.** O6 puts `100 Trying` on the wire before the first NG
  request, which ends the caller's INVITE retransmissions (RFC 3261 §17.1.1.2), so `B_ng` may
  exceed `T1` = 500 ms without provoking one. `B_media` = 3 s is under a tenth of Timer B =
  `64·T1` = 32 s, so exhausting the media budget still leaves the transaction ample room to fail
  or succeed on its own terms.
- **K4** An ICMP port-unreachable naming an in-flight exchange ends that exchange immediately as
  `NodeUnreachable` rather than waiting out `B_ng`. It counts as one failed exchange toward
  `Suspect` and MUST NOT by itself take a node to `Down`: ICMP is an unauthenticated hint, and one
  forged datagram must not be able to remove a media node from the pool.
- **K5** Every timer above is a fired-timer input (AGENTS.md rule 2). Nothing here reads a clock,
  so `MR-X-*` runs in virtual time.
- **K6** All seven values are configuration with these defaults. `B_ng` MUST be at least
  `7·T_ng` + one `T_ng`, or the fourth transmission is issued and immediately abandoned.

## 9. Error taxonomy

| Observation | `MediaError` | Node health (§10) | What the caller does |
|---|---|---|---|
| nothing arrived within `B_ng` | `NodeUnreachable` | one failed exchange | reselect (`ME-3`) within `B_media`; on exhaustion, `ME-4`'s policy: fail the call or proceed media-direct |
| ICMP port-unreachable (K4) | `NodeUnreachable`, immediately | one failed exchange | as above |
| `result` = `error`, with `error-reason` | `Rejected`, reason kept verbatim | **none** | fail the call. **Never reselect** |
| `result` = `load limit` | `NodeOverloaded` | node → `Loaded` | reselect: a different node may have room |
| `result` = `ok` but no `sdp` on an offer/answer (M2) | `Malformed` | one failed exchange | fail the call |
| unrecognised `result` (M3) | `Malformed` | one failed exchange | fail the call |
| undecodable datagram (D8) | discarded; the exchange keeps waiting | **none** | nothing until `B_ng` |
| a reply whose cookie matches nothing (F3) | discarded | **none** | nothing |
| `result` = `ok` with `warning` on `delete` of an unknown session | `Ok(None)`, warning recorded | none | nothing (D2) |

**Node down versus command rejected** — the distinction the whole taxonomy exists for. A rejection
is information about the *request*: the node parsed it, understood it and said no, and it said so
deterministically, so every other node in the pool will say the same. Retrying elsewhere converts
one failed call into `N` failed calls plus `N` allocations to clean up, and spends `B_media`
learning nothing. Unreachability is information about the *node* and says nothing about the
request, so it is the only class that reselects. Overload is a third thing — the node is healthy
and the request is fine, there is simply no room — and it reselects for a different reason: another
node has room. The `supports` key of §7.2 is what keeps the third class from collapsing into the
first, which is why sending it is REQUIRED rather than advisory.

- **X1** `Rejected` MUST NOT reselect, retry, or be retried by an upper layer.
- **X2** The reason bytes are recorded verbatim and MUST NOT be copied into a SIP response. They
  describe the platform's internals to an endpoint that did nothing wrong.
- **X3** Default status mapping: `Rejected` and `Malformed` become `500 Server Internal Error`
  (RFC 3261 §21.5.1); exhausted `NodeUnreachable`/`NodeOverloaded` becomes `503 Service
  Unavailable` where the policy is to fail rather than proceed media-direct. A tenant policy may
  map specific rejection reasons to `488 Not Acceptable Here` (RFC 3261 §21.4.26) — that mapping is
  `ME-4`'s and is configuration, never a default.
- **X4** No `MediaError` may panic a node or abort a transaction abruptly; each resolves to a
  response or to a media-direct continuation (AGENTS.md rule 3).

## 10. Health signals and node state

Node states: `Up`, `Loaded`, `Suspect`, `Down`.

| # | From | Event | To |
|---|---|---|---|
| H1 | `Up` | one exchange fails (`NodeUnreachable` or `Malformed`) | `Suspect` |
| H2 | `Up` | `result` = `load limit` | `Loaded` |
| H3 | `Suspect` | any `ok` or `pong` | `Up` |
| H4 | `Suspect` | `N_down` consecutive unanswered probes | `Down` |
| H5 | `Loaded` | a successful `offer`, or `T_load` elapses with no further `load limit` | `Up` |
| H6 | `Loaded` | `N_down` consecutive unanswered probes | `Down` |
| H7 | `Down` | `N_up` consecutive `pong`s | `Up` |
| H8 | any | pool membership withdrawn (`KO-7`) | removed |

- **P1 — only `Up` nodes take new sessions.** `Loaded`, `Suspect` and `Down` are excluded from the
  rendezvous set `ME-3` selects over.
- **P2 — existing sessions keep addressing their node regardless of its state.** The node id is
  fixed in a token that cannot be refreshed mid-dialog: the route set does not change after dialog
  establishment (affinity-token §7, RFC 3261 §12.2). Health governs *selection*, and the only
  remedy for an established session on a dead node is re-anchoring at the next offer or answer
  (S12).
- **P3 — the probe is `ping`.** A node that answers `ping` is Ready; that is the readiness signal
  `KO-7` gates managed pods on, and it is deliberately the same signal both places, because a
  readiness check that tests something other than the control plane tests the wrong thing.
- **P4** Probing is per node and only when idle: a node that answered an exchange within `T_ping`
  needs no probe, since a completed exchange is a stronger signal than a `pong`.
- **P5 — exported per node:** state, the platform's own count of anchored sessions (Q4), exchanges
  counted by outcome class (§9), retransmissions, budget expiries, decode faults, and time in
  state. These are observability, not control inputs (D5).
- **P6** `Suspect` costs at most `T_ping` of exclusion, because one answered probe restores `Up`
  (H3). That asymmetry is deliberate: excluding a healthy node briefly costs a rendezvous
  reshuffle, while including a dead one costs `B_media` on every call that picks it.

## 11. The tested baseline, and version drift

**Baseline: rtpengine `mr13.0.1.10`** (series `mr13.0`). This is the version the interop harness
(`CF-3`) runs and the series `deploy/helm/values.yaml` pins for the managed pool, so the version
under test, the version in the chart, and the version §12's bytes were written against are one
version. rtpengine is named here as an **integration target** under the AGENTS.md carve-out — a
system this platform talks to and is tested against. Nothing above is justified by its behaviour:
every rule cites an RFC, another spec in this directory, or a property of UDP.

- **V1 — "baseline" means tested, not minimum.** No claim is made about earlier releases. An
  adapter MUST NOT be pointed at one until the `CF-3` suite has been run against it and this
  section amended.
- **V2 — drift is handled by contract, never by version detection.** The adapter MUST NOT parse,
  compare or branch on a version string. D7 (ignore unknown keys), D11 (accept both spellings of
  the timestamp key) and M3 (reject unrecognised `result` values) are the entire compatibility
  surface, and capability discovery is the `supports`/`result` contract of §7.2 and §9 — the node
  tells us what it can do by how it answers.
- **V3 — a node without the `load limit` extension degrades, it does not break.** Such a node
  answers overload as an ordinary error, so the platform sees `Rejected` where `NodeOverloaded`
  was true. The consequence is bounded and known: that call fails instead of reselecting. Nothing
  is unsafe, and this is why §7.2 sends the key unconditionally rather than gating it on a version.
- **V4 — raising the baseline** is a story, not a config change: run `CF-3` green against the
  candidate, re-verify every `MR-E` vector byte-for-byte, amend this section and §12 with anything
  that moved, and update the chart in the same change. Later series exist and are untested here;
  running against one is an explicit, recorded decision.
- **V5 — §13.3's option spellings are unconfirmed until `CF-3` has run** (`ME-6`). The four keys
  §13 adds are *this spec's* contract for what the encoder produces, and §12.2 pins those bytes
  exactly. What they are not yet is confirmed to be the spelling the §11 baseline honours. That
  gap is not symmetric with the rest of §7: a node that does not recognise `ICE` gives a worse
  anchor, and a visible one, whereas a node that does not recognise `transport-protocol` or `SDES`
  gives **clear-text media on a leg whose policy said encrypted** — silently, because D7's
  ignore-unknown-keys discipline binds the far end as much as it binds us. MP12 is therefore a
  hard precondition on `ME-2` rather than a note, and MR-C-8 is the vector that discharges it.

## 12. Test vectors

Normative. `ME-2`'s tests derive from these; `CF-3` replays the `MR-E` and `MR-X` families against
a real node at the §11 baseline. Eight families: `MR-T` port semantics, `MR-N` the null relay,
`MR-E` encoding (byte-exact), `MR-X` exchange and timers, `MR-F` faults, `MR-H` health, and —
added by `ME-6` — `MR-P` per-trunk media policy and `MR-C` its configuration validation (§12.5).

### 12.1 Port semantics (MR-T) and the null relay (MR-N)

| # | Given | Expect |
|---|---|---|
| MR-T-1 | `offer` with a key carrying no `to_tag` | `offer` command, no `to-tag` on the wire (O2, MR-E-2) |
| MR-T-2 | `offer` twice with the same key and body | one session; the second returns the first allocation and is not counted as a new anchor (O1) |
| MR-T-3 | An SDP body with an attribute the platform does not model, and unusual line spacing | returned bytes are byte-identical through a null relay, and unmodified except by the node through the NG adapter (O3) |
| MR-T-4 | The callee re-offers mid-dialog | `from-tag` is the **callee's** tag (O4, MR-E-5) |
| MR-T-5 | `answer` issued twice for a retransmitted 2xx, same body | both succeed, same allocation, no deduplication state retained (A3) |
| MR-T-6 | `update` on a session in `Absent` | rejected by the caller's own precondition; no datagram is sent (U2) |
| MR-T-7 | `update` returns `Rejected` | session stays `Established`; **no** `delete` is issued (U3, S9) |
| MR-T-8 | `update` request built | carries `to-tag` and no `direction` (U4, MR-E-5) |
| MR-T-9 | `delete` for a session the node does not hold | `Ok(None)`; no fatal flag on the wire (D2, MR-E-10) |
| MR-T-10 | Anchored INVITE answered `486` | exactly one `delete` (D3, S6) |
| MR-T-11 | BYE retransmitted after teardown | second `delete` is idempotent (D2, S11) |
| MR-T-12 | `query` called while handling a forwarded request | forbidden: the test asserts no `query` is issued on any signalling path (Q2) |
| MR-T-13 | INVITE arrives; the adapter is about to send its first NG request | `100 Trying` has already been emitted (O6) |
| MR-N-1 | `NullMediaRelay::offer` with any body, including one with trailing whitespace and an unknown attribute | returned bytes compare byte-identical to the input; `warning` is `None` (N1) |
| MR-N-2 | `NullMediaRelay::delete` | `Ok(None)` (N2) |
| MR-N-3 | `NullMediaRelay::query` | `Err(NoSuchSession)`; the drain check reads it as zero (N2, Q3) |
| MR-N-4 | A call anchored through the null relay | the affinity token's `media node` field is `0` (N3) |
| MR-N-5 | A full call under the harness on the null relay | no NG datagram, no timer armed, no counter beyond the pass-through counter (N4) |

### 12.2 Encoding (MR-E) — byte-exact

Each vector is the complete datagram payload, cookie included. Requests are what this platform's
encoder MUST produce for the given value (E3); replies are inputs the decoder MUST accept. Values
use the reserved documentation identifiers of RFC 2606 and RFC 5737 and the tag and branch
examples of RFC 3261 §24, so no vector names a real host.

| # | Given | Expect |
|---|---|---|
| MR-E-1 | The health probe, and its reply | the two blocks below, byte for byte |
| MR-E-2 | Initial `offer`: call-id `a84b4c76e66710@invalid`, from-tag `1928301774`, branch `z9hG4bK776asdhds`, source `198.51.100.7`, interfaces `priv`/`pub`, `IcePolicy::Anchored`, the 116-byte body shown | the block below. Note key order: `ICE` before `call-id`, `received from` before `replace` (E3) |
| MR-E-3 | The node's success reply with a rewritten 112-byte body | decodes to `result` = `ok` and the `sdp` bytes; the body is forwarded verbatim (O3) |
| MR-E-4 | `answer` for the same session, to-tag `a6c85cf`, source `203.0.113.9` | the block below; `to-tag` present, `direction` absent |
| MR-E-5 | `update` — a re-offer on the established session | the block below: `command` is `offer`, `to-tag` present, `direction` absent (U1, U4) |
| MR-E-6 | `delete` at BYE | the block below; no fatal flag, no teardown delay (D2, D4) |
| MR-E-7 | `query` for the same session | the block below |
| MR-E-8 | A rejection | decodes to `Rejected` with the reason bytes kept verbatim (§9) |
| MR-E-9 | A `load limit` reply | decodes to `NodeOverloaded`, **not** `Rejected` (§9) |
| MR-E-10 | `delete` reply for a session the node does not hold | `Ok(None)`, warning recorded (D2) |
| MR-E-11 | A reply with keys out of order and one key this spec does not define | decodes successfully; the unknown key is ignored (D6, D7) |
| MR-E-12 | The MR-E-2 bytes with the final `e` removed | `DecodeFault`, no panic, no allocation from the declared lengths (D8, D9) |
| MR-E-13 | A datagram declaring a 900 000 000-byte string in 40 bytes | `DecodeFault`; work and memory bounded by the 40 bytes received (D9) |
| MR-E-14 | A dictionary carrying `call-id` twice | `DecodeFault` (D8) |
| MR-E-15 | The MR-E-3 bytes followed by one extra byte | `DecodeFault` — trailing bytes (F1, D8) |
| MR-E-16 | A reply spelling the timestamp key `last_signal` | decodes identically to one spelling it `last signal` (D11) |
| MR-E-17 | A cookie of 33 characters, or one containing a space | rejected before decoding (§6.1, §6.2) |
| MR-E-18 | Initial `offer` as MR-E-2, toward a trunk declaring `Restrict([PCMA, PCMU])`, `Transcode::None`, `Sdes { Sha1_80Only }` | the block below. `codec` carries `offer` and `strip` and **no** `transcode` (MP3); `SDES` carries the one suite exclusion; `transport-protocol` is `RTP/SAVP` (§13.3) |
| MR-E-19 | `answer` as MR-E-4, toward a trunk declaring the identity policy `{ AsReceived, None, Disabled }` | the block below. **No** `codec` key at all (MP8b); `transport-protocol`, `SDES` and `DTLS` all present and all saying "no SRTP" (MP8a) |
| MR-E-20 | Initial `offer` toward a trunk declaring `Restrict([PCMA])`, `To([PCMA])`, `DtlsSrtp { Passive }` | the block below. `codec` carries all three subkeys; `DTLS` is `["passive"]`; `transport-protocol` is `UDP/TLS/RTP/SAVP` (RFC 5764 §8) |

```rust
// MR-E-1 — ping request; 25 bytes on the wire
b"0007_2f d7:command4:pinge"
// MR-E-1 — ping reply; 24 bytes on the wire
b"0007_2f d6:result4:ponge"
```

```rust
// MR-E-2 — initial offer request; 376 bytes on the wire
b"0007_30 d3:ICEl7:defaulte7:call-id22:a84b4c76e66710@invalid7:command5:offer9:directionl4:priv3:pube8:from-tag10:192830177413:received froml3:IP412:198.51.100.7e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite10:via-branch16:z9hG4bK776asdhdse"
```

```rust
// MR-E-3 — offer reply, ok; 143 bytes on the wire
b"0007_30 d6:result2:ok3:sdp112:v=0\r\no=alice 2890844526 2890844527 IN IP4 192.0.2.50\r\ns=-\r\nc=IN IP4 192.0.2.50\r\nt=0 0\r\nm=audio 30000 RTP/AVP 0\r\ne"
```

```rust
// MR-E-4 — answer request; 369 bytes on the wire
b"0007_31 d3:ICEl7:defaulte7:call-id22:a84b4c76e66710@invalid7:command6:answer8:from-tag10:192830177413:received froml3:IP411:203.0.113.9e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite6:to-tag7:a6c85cf10:via-branch16:z9hG4bK776asdhdse"
```

```rust
// MR-E-5 — re-offer (the port's `update`); 369 bytes on the wire
b"0007_32 d3:ICEl7:defaulte7:call-id22:a84b4c76e66710@invalid7:command5:offer8:from-tag10:192830177413:received froml3:IP412:198.51.100.7e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite6:to-tag7:a6c85cf10:via-branch16:z9hG4bK776asdhdse"
```

```rust
// MR-E-6 — delete request; 101 bytes on the wire
b"0007_33 d7:call-id22:a84b4c76e66710@invalid7:command6:delete8:from-tag10:19283017746:to-tag7:a6c85cfe"
```

```rust
// MR-E-7 — query request; 100 bytes on the wire
b"0007_34 d7:call-id22:a84b4c76e66710@invalid7:command5:query8:from-tag10:19283017746:to-tag7:a6c85cfe"
```

```rust
// MR-E-8 — error reply; 58 bytes on the wire
b"0007_35 d12:error-reason15:Unknown call-id6:result5:errore"
```

```rust
// MR-E-9 — load-limit reply; 73 bytes on the wire
b"0007_36 d7:message30:Parallel session limit reached6:result10:load limite"
```

```rust
// MR-E-10 — delete reply for a session the node does not hold; 72 bytes on the wire
b"0007_37 d6:result2:ok7:warning38:Call-ID not found or tags didn't matche"
```

```rust
// MR-E-11 — reply with keys out of order and one unknown key; 159 bytes on the wire
b"0007_38 d3:sdp112:v=0\r\no=alice 2890844526 2890844527 IN IP4 192.0.2.50\r\ns=-\r\nc=IN IP4 192.0.2.50\r\nt=0 0\r\nm=audio 30000 RTP/AVP 0\r\n6:result2:ok10:future-keyi1ee"
```

The three blocks below are `ME-6`'s, and each is MR-E-2 or MR-E-4 with one thing changed — the
`toward` policy — so a diff against those blocks is exactly the policy's effect on the wire and
nothing else. Note where E3's raw-byte order puts the new keys: `DTLS`, `ICE` and `SDES` lead the
dictionary because uppercase sorts before lowercase, `codec` falls between `call-id` and
`command`, and `transport-protocol` between `to-tag` and `via-branch`.

```rust
// MR-E-18 — initial offer, codec restriction and SDES; 501 bytes on the wire
b"0007_39 d4:DTLSl3:offe3:ICEl7:defaulte4:SDESl26:no-AES_CM_128_HMAC_SHA1_32e7:call-id22:a84b4c76e66710@invalid5:codecd5:offerl4:PCMA4:PCMUe5:stripl3:allee7:command5:offer9:directionl4:priv3:pube8:from-tag10:192830177413:received froml3:IP412:198.51.100.7e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite18:transport-protocol8:RTP/SAVP10:via-branch16:z9hG4bK776asdhdse"
```

```rust
// MR-E-19 — answer, the identity policy; 425 bytes on the wire
b"0007_3a d4:DTLSl3:offe3:ICEl7:defaulte4:SDESl3:offe7:call-id22:a84b4c76e66710@invalid7:command6:answer8:from-tag10:192830177413:received froml3:IP411:203.0.113.9e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite6:to-tag7:a6c85cf18:transport-protocol7:RTP/AVP10:via-branch16:z9hG4bK776asdhdse"
```

```rust
// MR-E-20 — initial offer, transcoding and DTLS-SRTP; 503 bytes on the wire
b"0007_3b d4:DTLSl7:passivee3:ICEl7:defaulte4:SDESl3:offe7:call-id22:a84b4c76e66710@invalid5:codecd5:offerl4:PCMAe5:stripl3:alle9:transcodel4:PCMAee7:command5:offer9:directionl4:priv3:pube8:from-tag10:192830177413:received froml3:IP412:198.51.100.7e7:replacel6:origin11:SDP-versione3:sdp116:v=0\r\no=alice 2890844526 2890844526 IN IP4 198.51.100.7\r\ns=-\r\nc=IN IP4 198.51.100.7\r\nt=0 0\r\nm=audio 49170 RTP/AVP 0\r\n8:supportsl10:load limite18:transport-protocol16:UDP/TLS/RTP/SAVP10:via-branch16:z9hG4bK776asdhdse"
```

### 12.3 Exchange and timers (MR-X)

Virtual time throughout; `t` is milliseconds from the moment the command is issued.

| # | Given | Expect |
|---|---|---|
| MR-X-1 | Reply at `t` = 10 | one transmission; no timer left armed |
| MR-X-2 | No reply until `t` = 500 | transmissions at 0, 150, 450 — byte-identical, same cookie (C1, K2). The `t` = 500 reply completes the exchange with one allocation, not three |
| MR-X-3 | No reply at all | transmissions at 0, 150, 450, 1050; at 1500, `NodeUnreachable` with `attempts` = 4 (§8) |
| MR-X-4 | A reply arriving at `t` = 1600, after MR-X-3 gave up | discarded; node health unchanged (F3) |
| MR-X-5 | A reply whose cookie belongs to a different in-flight exchange | routed to that exchange, not to this one; neither is failed |
| MR-X-6 | ICMP port-unreachable at `t` = 30 | `NodeUnreachable` at `t` = 30, not at 1500; one failed exchange, node not `Down` (K4) |
| MR-X-7 | `ME-3` reselects after MR-X-3 and the second node also times out | the second exchange is cut off at `B_media` = 3000 ms total, not 3000 ms each (§8) |
| MR-X-8 | Two edges configured with the same edge id | cookies collide; a startup check rejects the configuration (C2) |
| MR-X-9 | An edge restarts and issues its first command | the counter does not restart at zero (C3) |
| MR-X-10 | `B_ng` configured below `8·T_ng` | rejected at startup (K6) |

### 12.4 Faults (MR-F) and health (MR-H)

| # | Given | Expect |
|---|---|---|
| MR-F-1 | MR-E-8 in reply to an `offer` | `Rejected`; **no** reselection, no second node contacted (X1) |
| MR-F-2 | MR-E-9 in reply to an `offer` | `NodeOverloaded`; node → `Loaded`; reselection proceeds (H2, §9) |
| MR-F-3 | `result` = `ok` with no `sdp` in reply to an `offer` | `Malformed`, and the call fails rather than proceeding with the endpoint's own address (M2) |
| MR-F-4 | `result` = `maybe` | `Malformed` (M3) |
| MR-F-5 | A `Rejected` reason containing an internal hostname | recorded verbatim in logs; the SIP response carries `500` and none of the reason text (X2, X3) |
| MR-F-6 | `NodeUnreachable` exhausted, tenant policy "fail" | `503` (X3) |
| MR-F-7 | `NodeUnreachable` exhausted, tenant policy "media-direct" | the call proceeds unanchored; the affinity token's `media node` is `0` (N3, S2) |
| MR-F-8 | Every `MediaError` variant, in every state of §3.3 | a response or a media-direct continuation; never a panic, never an abandoned transaction (X4) |
| MR-H-1 | One `B_ng` expiry against an `Up` node | `Suspect`; excluded from new sessions (H1, P1) |
| MR-H-2 | A `pong` after MR-H-1 | `Up`; exclusion lasted at most `T_ping` (H3, P6) |
| MR-H-3 | Three consecutive unanswered probes | `Down` (H4) |
| MR-H-4 | Two consecutive `pong`s against a `Down` node | `Up` (H7) |
| MR-H-5 | One `pong` against a `Down` node | still `Down` (H7) |
| MR-H-6 | `Loaded`, then `T_load` elapses with no further `load limit` | `Up` (H5) |
| MR-H-7 | A node goes `Down` while holding established sessions | those sessions still address it; no token is re-minted; media re-anchors only at the next offer or answer (P2, S12) |
| MR-H-8 | A node answering exchanges within `T_ping` | no probe is sent (P4) |
| MR-H-9 | Pool membership withdrawn for a `Down` node | removed from the pool; no further probes (H8) |
| MR-H-10 | A decode fault on a stray datagram | node health unchanged (§9) |

### 12.5 Per-trunk media policy (MR-P) and its configuration (MR-C)

`ME-6`'s. `MR-P` runs against the pure derivation of §13.2 MP1 — no node, no socket, no clock —
so the whole family is a table-driven unit test. `MR-C` runs against configuration loading, and
every row of it asserts a **refusal to start** rather than a runtime behaviour.

MR-P-1 is the row the story's fourth acceptance item names: *the offer sent to a trunk matches its
declared policy*. It is asserted as bytes, against MR-E-18, because bytes are the only form of
"the offer sent to a trunk" that this platform ever holds — O3 and MP10 mean it never holds a
parsed one.

| # | Given | Expect |
|---|---|---|
| MR-P-1 | A trunk declaring `Restrict([PCMA, PCMU])`, `Transcode::None`, `Sdes { Sha1_80Only }`; an `offer` toward it | the keys of MR-E-18, byte for byte (MP1) |
| MR-P-2 | That policy, ten times, varying Call-ID, tags, branch, source address and body | ten byte-identical key sets: the derivation reads the policy and nothing else (MP1) |
| MR-P-3 | Two trunks, one `Sdes` and one `Disabled`, and a call routed to each | `RTP/SAVP` toward one and `RTP/AVP` toward the other, with no domain, URI, host or pattern anywhere in either derivation (MP1, MP6) |
| MR-P-4 | A trunk configuration that omits `transcode` | `Transcode::None`, and no `transcode` subkey on the wire (MP2) |
| MR-P-5 | `Restrict([PCMA])` with `Transcode::None` | `codec` carries `offer` and `strip` and **no** `transcode`: restricting is not transcoding (MP2, MP3) |
| MR-P-6 | `AsReceived` with `Transcode::None` | no `codec` key at all — not an empty dictionary (MP8b, E4, MR-E-19) |
| MR-P-7 | `AsReceived` with `To([PCMA])` | `codec` carries `transcode` and neither `offer` nor `strip` |
| MR-P-8 | `SrtpPolicy::Disabled` | `transport-protocol` = `RTP/AVP`, `DTLS` = `["off"]`, `SDES` = `["off"]` — all three present on a policy that asks for no encryption (MP8a, MR-E-19) |
| MR-P-9 | `Sdes { Rfc4568Both }` versus `Sdes { Sha1_80Only }` | `transport-protocol` = `RTP/SAVP` in both; `SDES` absent in the first and one exclusion in the second (§13.3, MP8b) |
| MR-P-10 | `DtlsSrtp { Passive }` and `DtlsSrtp { Active }` | `UDP/TLS/RTP/SAVP` with `DTLS` = `["passive"]` / `["active"]`, `SDES` = `["off"]` in both (MR-E-20) |
| MR-P-11 | A call between trunk A (`Disabled`) and trunk B (`Sdes`) | the `offer` carries **B's** keys and the `answer` carries **A's** (MP4) |
| MR-P-12 | The same call with the trunks exchanged | the mapping exchanges with them. This row exists because MP4 inverted is not an error anywhere — it is clear-text media on the leg that was supposed to be encrypted |
| MR-P-13 | A re-offer (`update`) on an established session | the same policy keys as the initial offer toward that peer, and still no `direction` (MP4, U4) |
| MR-P-14 | The signature of `media_keys` | it admits a policy and a command and nothing else: no request, URI, domain, source address, SDP, clock or randomness is in scope (MP1, MP6). The assertion is that the declaration compiles as written |
| MR-P-15 | A policy applied on `delete`, `query` and `ping` | none of the four keys appears (M4) |
| MR-P-16 | The `sdp` returned by a successful `offer` under any policy | forwarded verbatim; nothing parses it to confirm the policy was applied (MP10, O3) |
| MR-C-1 | `Sdes` on a trunk whose signalling transport is UDP or TCP | startup error naming the trunk and the transport; the node does not serve traffic (MP7, G-M1) |
| MR-C-2 | The same policy on a TLS trunk | accepted (MP7) |
| MR-C-3 | A quirk profile requiring SRTP, attached to a trunk declaring `Disabled` | startup error naming both the trunk and the profile (MP11, G-M5) |
| MR-C-4 | Two quirk profiles on one trunk, both requiring SRTP, the trunk declaring `Sdes` | accepted: constraints intersect, and intersecting needs no precedence rule (MP11) |
| MR-C-5 | A quirk profile carrying a field that *sets* an SRTP mode, a codec list or a transcoding list | rejected: the schema has no such field, so this fails to parse rather than failing to apply (MP11) |
| MR-C-6 | `Restrict([])`, and a `CodecName` that is empty or not a token, and a list with a duplicate entry | a startup error each, naming the trunk and the entry (G-M2, G-M3, G-M4) |
| MR-C-7 | A policy other than `{ AsReceived, None, Disabled }` while MP12 holds | startup error naming the trunk and MP12 (G-M6) |
| MR-C-8 | Each key of §13.3, against a live node at the §11 baseline (`CF-3`) | each is honoured, asserted by observing the **media** — the transport protocol on the leg, and the formats carried — never by parsing the reply's `sdp` in the platform. This is the row that discharges MP12 and V5; a test may read a description, the platform may not (MP10) |

## 13. Per-trunk media policy (`ME-6`)

Normative. This section is numbered after §12 rather than before it because §12's number is cited
from outside this file; nothing here is subordinate to the vectors, and §12.5 carries its rows.

Which codecs are offered toward a peer, and whether the leg facing that peer is SRTP, are
properties of the **trunk**. They are declared once, as data, in the trunk's own configuration.
They are not properties of the call, the request, the caller's domain, the tenant, the time of
day, or the branch of the routing logic that happened to produce the egress — and §13.2 is
written so that none of those is even reachable from the decision.

The failure this replaces is worth naming precisely, because it is what makes the rest of the
section look strict. When the codec offered depends on which arm of a NAT test a call fell
through, and SRTP is selected by matching a pattern against a domain, then two things are true at
once: nobody can answer "what do we offer this carrier?" without reading the routing logic, and
nobody can change the answer for one carrier without editing logic shared by all of them. Neither
is a bug in any one line. Both are the absence of a declaration.

### 13.1 The policy value

```rust
/// Everything the media decisions of §13.3 are permitted to read. There is nothing else.
pub struct TrunkMediaPolicy {
    pub codecs:    CodecPolicy,
    pub transcode: Transcode,
    pub srtp:      SrtpPolicy,
}

pub enum CodecPolicy {
    /// Offer what was received, in the order received. The default.
    AsReceived,
    /// Keep only these, in this order. A filter over what arrived — never an addition (MP3).
    Restrict(Vec<CodecName>),
}

pub enum Transcode {
    /// No transcoding. The default, and it is emitted rather than assumed (MP2).
    None,
    /// Produce these formats, transcoding where the peer did not offer them.
    To(Vec<CodecName>),
}

pub enum SrtpPolicy {
    /// RTP/AVP toward this peer, and any keying the other leg carried is not propagated.
    Disabled,
    /// SDES — keys in the description (RFC 4568). Requires confidential signalling (MP7).
    Sdes { suites: CryptoSuites },
    /// DTLS-SRTP — keys in the media path (RFC 5764), fingerprint in the description (RFC 8122).
    DtlsSrtp { role: DtlsRole },
}

/// Closed, and small on purpose: RFC 4568 §6.2 defines exactly two suites for SRTP, and the
/// only policy question worth a knob is whether the 32-bit authentication tag is admissible
/// (RFC 3711 §9.5).
pub enum CryptoSuites { Rfc4568Both, Sha1_80Only }

/// RFC 4145 §4.1. `Passive` is the default: a relay that accepts the DTLS association rather
/// than initiating it works from behind a NAT the platform does not control.
pub enum DtlsRole { Passive, Active }

/// An RTP payload-format name. Byte-compared and case-preserving, like every other token this
/// spec carries (E2); the platform assigns it no meaning beyond identity.
pub struct CodecName(Bytes);
```

`TrunkMediaPolicy` is a field of the trunk object `RT-2` builds. This spec fixes its meaning and
its effect on the wire; it does not fix where trunks are stored, how they are versioned, or how a
route plan arrives at one — those are [routing-trunks](../designs/routing-trunks.md)'s.

`OfferRequest` and `AnswerRequest` carry it as `toward` (§3.1): the policy of the peer that
receives the returned SDP, per MP4.

### 13.2 Rules

- **MP1 — the declared policy is the only input.** The derivation is one total, pure function:

  ```rust
  fn media_keys(policy: &TrunkMediaPolicy, command: Command) -> Vec<(NgKey, NgValue)>;
  ```

  Its signature is the enforcement, not a comment about it. There is no request in scope, no URI,
  no domain, no `From`, no source address, no SDP, no clock and no randomness — so a derivation
  that consulted one could not be written without changing this line, which is a spec change under
  review rather than a branch nobody reads. Two calls carrying the same policy produce the same
  keys, byte for byte, whatever else differs about them (MR-P-2). This is AGENTS.md rule 2 applied
  to the one decision this story exists to move: the pure part is the policy, and the driver's only
  remaining job is to put the bytes on a socket.

  `command` is the NG command of §7.1, and it is present for one reason: the derivation yields no
  keys at all for `delete`, `query` and `ping` (M4, MR-P-15). It does not vary with anything about
  the call.

- **MP2 — no transcoding unless declared, and the default is written down.** A trunk whose
  configuration omits `transcode` is `Transcode::None`, and `Transcode::None` appears in the
  effective-policy record §13.5 exports, so "we do not transcode toward this carrier" is a
  readable fact rather than the absence of one. Nothing else may turn transcoding on: not a codec
  list (MP3), not an SRTP mode, not a call class, and not a quirk profile (§13.6). Transcoding is
  the most expensive thing on this page and the easiest to acquire by accident; acquiring it
  requires writing the word.

- **MP3 — `codecs` filters, and only `transcode` adds.** `CodecPolicy::Restrict` may remove and
  reorder the formats the received description carried. It can never introduce one that was
  absent, because producing an absent format *is* transcoding, and transcoding is MP2's
  declaration.

  The consequence is stated here so that nobody meets it as a surprise: a `Restrict` list disjoint
  from what the peer offered leaves the stream with no common format, and the call fails (§13.4)
  instead of quietly consuming a transcoding slot. RFC 3264 §6 already describes that outcome —
  the answerer rejects a stream whose formats are unacceptable by setting its port to zero — so
  the failure is the offer/answer model working, not this platform inventing a fault. The
  alternative, inferring `To(...)` from a `Restrict` list, is precisely the defect this story
  exists to remove, one layer down: the transcoding bill would once again depend on something
  nobody declared.

- **MP4 — the policy in force is the *receiving* peer's, and it changes with the command.** An
  `offer` carries party A's description and returns the description that travels toward party B,
  so the keys on that command are **B's** trunk policy. An `answer` carries B's description and
  returns what travels toward A, so its keys are **A's**.

  | Command | Body carried | Returned SDP travels toward | `toward` is the policy of |
  |---|---|---|---|
  | `offer` (initial) | A | B | B |
  | `answer` | B | A | A |
  | `update` (re-offer by A) | A | B | B |
  | `update` (re-offer by B) | B | A | A |

  Getting this backwards is not a visible error: it is a call that negotiates encryption toward
  the wrong half of itself and fails as one-way or clear-text media, which is the same class of
  quiet wrongness O4 exists to prevent for `from_tag`. MR-P-11 and MR-P-12 assert it in both
  directions for that reason.

- **MP5 — SRTP is per leg, and the legs are independent.** The relay terminates each leg's media
  security separately, so `Disabled` on one side and `Sdes` on the other is an ordinary, supported
  configuration and not a contradiction to be resolved. That independence is the whole reason a
  *per-trunk* SRTP mode is coherent: were security a property of the call, a carrier that requires
  SRTP and an access side that cannot do it would have no expressible answer.

- **MP6 — there is no domain-, pattern- or hostname-derived selection, anywhere.** SRTP is not
  chosen by matching a regular expression against a request URI, a `To` domain, a `Contact` host or
  a source address, and neither is a codec list. MP1's signature makes this unrepresentable rather
  than merely forbidden — the same technique §7.3 I3 uses for the ICE variant that must not exist.
  Where a deployment genuinely wants "these carriers, one rule", the mechanism is that those
  carriers are trunks and a trunk's policy is data that can be shared by reference; it is not a
  pattern evaluated per call.

- **MP7 — `SrtpPolicy::Sdes` requires confidential signalling, checked at startup.** SDES puts the
  session key in the description (RFC 4568 §5), and RFC 4568 §7.2 requires that a description
  carrying one be protected in transit. A trunk declaring `Sdes` whose signalling transport is not
  TLS is therefore a **configuration error that refuses to start**, naming the trunk and the
  transport (MR-C-1) — not a log line, and not a runtime downgrade. A runtime warning about a key
  that has already been sent in the clear is a description of an incident, and the value of
  encrypting the media afterwards is close to zero. `DtlsSrtp` carries no key in the description
  (RFC 5763 §5 — only a fingerprint, RFC 8122) and so imposes no such requirement; its integrity
  requirement is that the fingerprint arrive unaltered, which is `AF-*`'s and the transport's.

- **MP8 — the keys that decide a mode are always explicit; the keys that restrict within one are
  sent when they restrict.**
  - **MP8a — mode keys, unconditional.** `transport-protocol` and `DTLS` are sent on every
    `offer`, `answer` and re-offer, for every `SrtpPolicy` **including `Disabled`**. This is §7.3
    I4's rule applied to media security, and the reason is stronger here than it is for ICE: an
    omitted key means whatever the node's default means at the version it happens to be running
    (§11), and the difference between two such defaults is the difference between encrypted and
    clear-text media. `transport-protocol` in particular is the single key that decides it, and it
    is never omitted for any policy.
  - **MP8b — restriction keys, when they restrict.** `SDES` is sent when the policy forbids SDES
    (`["off"]`) or excludes a suite, and is absent when it does neither. `codec` is sent when
    `codecs` is not `AsReceived`, or `transcode` is not `None`, or both. In each case the omitted
    case is the *identity*, whose only encoding is absence: an empty list or an empty dictionary
    violates E4 and asserts nothing that absence does not. §13.3's tables are the enumeration.

- **MP9 — version 1 negotiates nothing; it asserts.** There is no opportunistic SRTP, no
  best-effort fallback to RTP when a peer declines, and no codec policy conditioned on what the
  peer offered. All three would require reading the description that came back, which O3 forbids
  and MP10 restates. A trunk that must accept either is two trunks, or it waits for the story that
  gives this section an SDP model — recorded in §13.7 as the open question it is.

- **MP10 — the platform reads no body, to decide or to verify.** MP1 already forbids reading one
  to decide. Verification is the harder half and gets its own clause: after `offer` returns, the
  platform MUST NOT parse the returned `sdp` to confirm that the policy was applied — not for a
  metric, not for a log line, not "just for the assert". A verifying parser is an SDP model in the
  hot path acquired through the back door, with all of O3's failure modes and none of its
  discipline. Proof that the policy reached the wire comes from §12.5's byte-exact vectors and
  from `CF-3` observing the media itself; **a test may parse SDP, the platform may not**, and
  MR-C-8 is written on exactly that division.

- **MP11 — a quirk profile constrains media policy; it never sets it.** §13.6.

- **MP12 — no policy beyond the identity ships before `CF-3` is green** (§11 V5). Until the
  interop suite has confirmed, against the §11 baseline, that each §13.3 key is honoured, `ME-2`
  MUST reject at startup any trunk policy other than
  `{ AsReceived, None, Disabled }` — the identity policy, whose failure mode is the media-direct
  behaviour the platform already has. The asymmetry is deliberate: an unrecognised codec key
  degrades to "no manipulation", which is safe and loud, whereas an unrecognised SRTP key degrades
  to clear text, which is unsafe and silent.

### 13.3 The mapping onto NG keys

The keys, and the exact bytes each policy value produces. `⟨absent⟩` means the key is not emitted
at all (E4).

| Policy value | `transport-protocol` | `SDES` | `DTLS` |
|---|---|---|---|
| `Disabled` | `RTP/AVP` | `["off"]` | `["off"]` |
| `Sdes { Rfc4568Both }` | `RTP/SAVP` | `⟨absent⟩` | `["off"]` |
| `Sdes { Sha1_80Only }` | `RTP/SAVP` | `["no-AES_CM_128_HMAC_SHA1_32"]` | `["off"]` |
| `DtlsSrtp { Passive }` | `UDP/TLS/RTP/SAVP` | `["off"]` | `["passive"]` |
| `DtlsSrtp { Active }` | `UDP/TLS/RTP/SAVP` | `["off"]` | `["active"]` |

`SDES` is absent in exactly one row, and MP8a is why that is not a hole. The key that decides
encrypted-versus-clear is `transport-protocol`, and it is present in every row. `SDES` says one of
two further things — *not this mechanism at all*, or *this mechanism, minus a suite* — and
`Sdes { Rfc4568Both }` says neither: it admits both suites RFC 4568 §6.2 defines, so it excludes
none, and an empty exclusion list is what E4 forbids emitting. Absence there is E4 working, over a
mode `RTP/SAVP` and `DTLS: ["off"]` have already fixed between them.

| Policy value | `codec` |
|---|---|
| `AsReceived` + `None` | `⟨absent⟩` (MP8b) |
| `Restrict([a, b])` + `None` | `{ "offer": [a, b], "strip": ["all"] }` |
| `AsReceived` + `To([a])` | `{ "transcode": [a] }` |
| `Restrict([a])` + `To([a])` | `{ "offer": [a], "strip": ["all"], "transcode": [a] }` |

`strip` accompanies `offer` because `offer` alone states a preference and `strip` is what makes
the restriction a restriction; together they are MP3's filter. `transcode` is the only subkey that
can introduce a format, which is what makes MP2's default meaningful — remove `transcode` from a
policy and no combination of the other two can add anything.

Key order on the wire is E3's, over raw bytes, and is not the order of these tables: `DTLS`,
`ICE`, `SDES`, then `call-id`, `codec`, `command`, and so on. §12.2's blocks are the authority.

**These spellings are provisional** in the one specific sense §11 V5 defines: the encoding is
fixed and tested here, and the far end's acceptance of it is fixed and tested by `CF-3`. MP12
holds until then.

### 13.4 Failure, and what the platform can honestly see

A policy that the node cannot satisfy comes back as `result` = `error`, which is `Rejected` (§9).
Everything §9 says about it applies unchanged: the reason bytes are kept verbatim, are never
copied into a SIP response (X2), and **never** cause reselection (X1) — a codec or SRTP policy is
deterministic, so a second node rejects it identically and reselecting would spend `B_media`
learning that.

The platform cannot distinguish "no common codec" from any other rejection without reading either
the body or the reason string, and it does neither by default: the body is O3's and the reason is
X2's. §9 X3 already provides the only sanctioned route — a tenant policy mapping specific
rejection reasons to `488 Not Acceptable Here` (RFC 3261 §21.4.26) — and a codec-mismatch mapping
is exactly the case that provision was written for. It stays configuration, per trunk, and it is
never a default: the reason strings a node emits are the node's, and a default that pattern-matched
them would be behaviour derived from a version rather than from a contract, which V2 forbids.

What this leaves open, stated rather than hidden: **the observability of a codec mismatch is
poor.** An operator sees `Rejected` with a reason string, and the call sees `500` unless a mapping
is configured. §13.7 records it.

### 13.5 Configuration validation, and the effective-policy record

Every check below runs at **startup**, and every failure refuses to start, naming the trunk. This
follows the discipline [extension-framework](../designs/extension-framework.md) sets for module
manifests, for the same reason it gives: an invalid combination should fail a deployment, never a
call.

| # | Check | On violation |
|---|---|---|
| G-M1 | `Sdes` on a trunk whose signalling transport is not TLS (MP7) | error naming the trunk and its transport |
| G-M2 | `Restrict([])` — an empty restriction, which offers nothing | error naming the trunk |
| G-M3 | A `CodecName` that is empty or is not a token | error naming the trunk and the entry |
| G-M4 | A duplicate `CodecName` within one list | error naming the trunk and the entry |
| G-M5 | A quirk profile requiring SRTP on a trunk declaring `Disabled` (§13.6) | error naming the trunk **and** the profile |
| G-M6 | Any policy other than the identity while MP12 holds | error naming the trunk and MP12 |

**The effective-policy record.** Each trunk exports its resolved policy — every field, defaults
included, plus the quirk profiles that constrained it — as one structured record, readable without
placing a call. This is what makes the story's goal checkable rather than merely intended: "what
do we offer this carrier, and is that leg encrypted?" is answered by reading one record, and
nothing that is not in that record can reach §13.3. It is observability of a *declaration*, not of
traffic, so §10 P5's rule that health counters are never control inputs does not bear on it.

### 13.6 The seam with carrier quirk profiles (`EX-7`)

`EX-7` specifies carrier quirk profiles: per-peer header injection and SDP rewriting as bounded,
declarative data, attachable to a trunk or a domain, several of which may apply. Its acceptance
includes a quirk that also implies SRTP. That is where the two stories meet, and this is this
side's half of the seam. The vocabulary itself is `EX-7`'s and is not restated here.

- **MP11 — a quirk profile may *require* an SRTP mode; it may never *assign* one.** A profile
  carries a constraint (`requires_srtp`) that is checked at startup against the trunk's own
  declared `SrtpPolicy` (G-M5). It carries no field that sets `SrtpPolicy`, `CodecPolicy` or
  `Transcode`, and a profile schema that offers one is rejected (MR-C-5).

  Two reasons, and the second is the one that decides it:

  1. If a profile could set the mode, SRTP selection would once again be a consequence of *which
     profile matched* — a pattern, evaluated per call, deciding whether media is encrypted. That
     is the defect ME-6 exists to remove, wearing a better-typed hat. MP6 does not stop being true
     because the pattern moved into a profile.
  2. Profiles **compose** — `EX-7` says several may apply to one trunk — and assignments do not.
     Two profiles assigning different modes need a precedence rule, and every precedence rule over
     independently-authored configuration is a rule someone will be surprised by at three in the
     morning. Constraints compose without one: they intersect, order-independently, and two
     profiles both requiring SRTP agree (MR-C-4) while a profile and a trunk disagreeing is caught
     before the process serves traffic (MR-C-3).

- **Quirk SDP rewriting does not reach codecs or keying.** Whatever body rewriting `EX-7` defines
  is a separate mechanism with its own vector; it is not a second path to the four keys of §13.3,
  because those are derived from `TrunkMediaPolicy` by MP1's function and from nothing else. A
  quirk that needs a codec offered adds a constraint the trunk's policy must already satisfy, in
  the shape of MP11 — it does not edit the offer.

- **Left to `EX-7`:** the spelling and placement of the constraint in the profile schema, whether
  constraints exist for anything besides SRTP, and how a profile is bound to a trunk. This section
  fixes only that the direction is constrain-not-assign, and that violating it is a startup error.

### 13.7 What §13 does not decide

- **Where trunks live, and how their configuration is versioned and reloaded** — `RT-2`, with the
  `policy version` interaction [affinity-token](affinity-token.md) and `AF-1` own.
- **Which trunk a call takes** — the route plan's, [routing-trunks](../designs/routing-trunks.md).
  §13 begins after that question is answered, and deliberately cannot influence it.
- **Opportunistic or negotiated SRTP, and codec policy conditioned on the peer's offer** — both
  need to read a description (MP9, MP10). They are the same open question `ME-4` has, resolve to
  the same upstream SDP-model row (§2), and are not answered by adding a variant here.
- **Bandwidth, direction attributes, DTMF interworking and recording** — none is a codec or a
  keying decision, and each would widen a vocabulary §13.1 keeps closed on purpose.
- **The status a codec mismatch produces by default** — §9 X3's mapping is per tenant, and §13.4
  says why there is no default.

## 14. What this spec does not decide

- **Which calls are anchored** — `ME-4`, over the hook phases of
  [hook-framework](hook-framework.md) §3, with `ME-5` implementing the module.
- **Which node, and reselection** — `ME-3`: rendezvous hashing over the node-set epoch, and the
  open question the design records about recomputing a selection whose token cannot be refreshed.
- **Pool operation** — `KO-7`: managed and external modes, host networking, port ranges, the
  private control interface, and draining.
- **The interop container and its digest** — `CF-3`.
- **The trunk object, and everything §13 leans on but does not own** — `RT-2` for the trunk
  itself, `EX-7` for the quirk-profile vocabulary §13.6 constrains. §13.7 is that list in full.
