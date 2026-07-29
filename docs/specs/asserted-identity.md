# Spec: Asserted identity and privacy

**Status:** normative · **Crate:** _future — lands with the trunk model (RT-2)_ ·
**Stories:** RT-7 · **Design:** [routing-trunks](../designs/routing-trunks.md)

## 1. Normative references

- **RFC 3325** (`P-Asserted-Identity`, informational, but the contract every carrier interop is
  written against):
  - §1 (Applicability Statement) — the mechanism "is only applicable inside a 'Trust Domain' as
    defined in \[RFC 3324]"; nodes in it are "responsible for withholding that identity outside of
    the Trust Domain when privacy is requested"; and `Spec(T)` **MUST** specify eight things, of
    which items 4, 5 and 8 are what §3 below turns into required configuration.
  - §4 (Overview) — the header carries "a URI (commonly a SIP URI) and an optional display-name";
    a proxy forwarding to an element it does not trust "MUST remove all the P-Asserted-Identity
    header field values if the user requested that this information be kept private".
  - §5 (Proxy Behavior) — three separate rules, and this spec keeps them separate: a proxy that
    wishes to add the header on a message from an untrusted node "MUST authenticate the originator
    … and use the identity which results from this authentication"; "If there is no
    P-Asserted-Identity header field present, a proxy **MAY** add one containing at most one SIP or
    SIPS URI, and at most one tel URL"; and a PAI arriving from an element the proxy does not trust
    **MUST** be replaced with a single URI of the same scheme family or the header removed.
  - §6 (Hints for Multiple Identities) — `P-Preferred-Identity` is a hint, a hint that "does not
    correspond to any valid identity known to the proxy for that user" may be ignored or rejected
    "(for example, with a 403 Forbidden)", and "The proxy MUST remove the user-provided
    P-Preferred-Identity header from any message it forwards."
  - §7 (Requesting Privacy) — `id` present ⇒ proxies "MUST remove all the P-Asserted-Identity
    header fields before forwarding messages to elements that are not trusted"; `none` ⇒ the proxy
    "MUST NOT remove" them; with no `Privacy` header toward an untrusted element the proxy "MAY
    include … or it MAY remove it", a decision that "MUST be specified in Spec(T)"; and where
    privacy is requested, *all* values go, not the first.
  - §9.1 (`PAssertedID` ABNF) — one or two values; one ⇒ `sip`, `sips` or `tel`; two ⇒ one
    `sip`/`sips` and one `tel`. Its Table 2 addition fixes the method set (`o` for BYE, INVITE,
    OPTIONS, SUBSCRIBE, NOTIFY, REFER; `-` for ACK, CANCEL, REGISTER, INFO, UPDATE, PRACK) and
    gives a proxy the `adr` rights — add, delete, read.
  - §9.3 (`priv-value = "id"`) — the token means the user "would like the Network Asserted Identity
    to be kept private with respect to SIP entities outside the Trust Domain with which the user
    authenticated".
  - §10.1 — the worked example whose header forms §7.1 below matches byte for byte.
- **RFC 3324** (the requirements RFC 3325 §2 defers its vocabulary to):
  - §2.2 — a Network Asserted Identity is "derived … as a result of an authentication process";
    for a `sip`/`sips` URI "the domain included in the URI MUST be within the Trust Domain", and
    for a `tel` URI "the owner of the E.164 number in the URI MUST be within the Trust Domain".
  - §2.3 — `B` trusts `A` **iff** "1. there is a secure connection between the nodes, AND 2. B has
    configuration information indicating that A is a member of the Trust Domain", where a secure
    connection means messages "cannot be read by third parties, cannot be modified by third parties
    without detection and that B can be sure that the message really did come from A" — and "The
    level of security required is a feature of the Trust Domain i.e., it is defined in Spec(T)."
- **RFC 3323** (the `Privacy` header):
  - §4.1.1.3 — "the hostname value 'anonymous.invalid' SHOULD be used for anonymous URIs", and the
    recommended `From` form.
  - §4.2 — `Privacy-hdr = "Privacy" HCOLON priv-value *(";" priv-value)` and
    `priv-value = "header" / "session" / "user" / "none" / "critical" / token`; the definitions of
    each value, including `critical` ("if these privacy services cannot be provided by the network,
    this request should be rejected. Criticality cannot be managed appropriately for responses");
    "Each legitimate priv-value can appear zero or one times in a Privacy header";
    "Intermediaries MUST NOT remove or alter a Privacy header whose priv-value is 'none'" and "An
    intermediary MUST NOT modify the Privacy header in any way if the 'none' priv-value is already
    specified"; and the sanction for an intermediary *adding* the header — "such intermediaries
    SHOULD only do so if they are operating at a user's behest, for example if a user has an
    administrative arrangement with the operator of the intermediary".
  - §4.3 — the `privacy` option-tag in `Proxy-Require`, which A32 answers.
  - §5 — "Privacy services MUST implement support for the 'none' and 'critical' privacy tokens";
    the `critical` failure is "a 500 (Server Error) response code" whose reason phrase SHOULD
    enumerate "the priv-value(s) which were not supported".
  - §5.1 — header privacy "must frequently act as a transparent back-to-back user agent (B2BUA)",
    the clause [proxy-engine](../designs/proxy-engine.md) (PX-8) already declined for v1.
  - §5.3 — user-level privacy, `From` anonymisation, and the observation this spec leans on: "dialog
    matching uses only the tags in the To and From headers, rather than the whole header fields",
    so the URI may be altered; the B2BUA `SHOULD` is scoped to modifying *dialog-matching* headers.
- **RFC 5379** (guidelines; the operative reading of the two above):
  - §4.1 Table 1 — the priv-value × header matrix. `From` is `anonymize` under **`user` only**;
    `P-Asserted-Identity` is `delete` under **`header` and `id`**, and has no treatment under
    `user`. That the two rows share no column is the whole of §9.1.
  - §4.3 — restates RFC 3323 §5's `critical` ⇒ `500`, and adds the stronger recommendation A31
    declines by default and makes a declared field instead.
  - §5.1.4 (From) — the anonymous form `From: "Anonymous" <sip:anonymous@anonymous.invalid>;tag=…`,
    "The tag value varies from dialog to dialog, but the rest of this header form is recommended as
    shown", and the Note that "does not prevent a privacy service from anonymizing the From header
    based on local policy" — the sanction for A26 being a *declared* per-trunk behaviour.
  - §5.1.8 (P-Asserted-Identity) — delete on `Privacy:id`, and "should delete … when user privacy
    is requested with Privacy:header before it forwards the message to an entity that is not
    trusted"; plus the "even when forwarding to a trusted entity … unless it can be confident"
    recommendation that A20 answers.
- **RFC 3261** §7.3.1 (a comma-separated header may be split across lines and the two forms are
  equivalent), §12.1.1 (a dialog is identified by Call-ID plus the two tags — not by the `To`/`From`
  URIs, which is why A26 may rewrite a `From` URI at all), §20 (angle brackets and URI parameters),
  §21.4.4 `403 Forbidden`, and §21.5.1 `500 Server Internal Error` — the status RFC 3323 §5 and
  RFC 5379 §4.3 both name "500 (Server Error)".
- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- sipx kernel contracts consumed: the lossless message model (every byte this spec does not name is
  re-serialised verbatim), `Uri` and its scheme discrimination, and `Headers` surgery
  (`remove_first`, `insert_at`, `retain` — upstream ledger `S-15`/PX-3) for the add and delete
  rights RFC 3325 §9.1 grants a proxy.
- This repo's specs consumed: [number-normalisation](number-normalisation.md) §2, `N4`, `N6`,
  `N17`, `N19`, `N23`, `N25`, `N31`, `N32` (the seam — §6.1); [proxy-behavior](proxy-behavior.md)
  §4 V6/V7, §5 P2, §7 F2/F4/F5, §8, §10 S4; [hook-framework](hook-framework.md) §3 H4/H9/H11;
  [registrar-auth](registrar-auth.md) §5 (the principal); [affinity-token](affinity-token.md) §3
  and `M8` (the 64-byte module-facts sub-budget A12 draws one octet from).

## 2. What this is, and what it is not

This spec fixes **whether this platform asserts an identity toward a given peer, which identity,
and what a caller's privacy request does to the message that leaves.** Its subject is a *peer* —
a trunk on the egress side, an ingress scope on the receiving side — because that is the grain the
obligation has. What a carrier requires differs per carrier, and the requirement is a compliance
obligation rather than a preference.

**In scope:** the trust declaration (§3); what the platform learns about identity at ingress and
what it refuses to carry (§5); where identity synthesis sits in the forwarding pipeline (§6);
identity selection (§7); the emission gate (§8); anonymous callers and the `From` header (§9);
`Privacy: critical` (§10); the configuration surface, its startup validation and the
effective-policy record (§11, §12).

**Out of scope, with owners:** the *shape* of a number inside any of these fields
([number-normalisation](number-normalisation.md), RT-6 — §6.1 is the seam); which egress a call
takes (RT-9) and the trunk object that carries this policy (RT-2); the egress header allowlist
(RT-5); the ingress scope vocabulary this spec's receiving side is keyed on (RT-8); the
configuration file's syntax and reload semantics (DP-1, KO-8); RFC 3323 `header`-level privacy —
Via, Contact and Record-Route obscuring — which [proxy-engine](../designs/proxy-engine.md) settled
out of scope for v1 under PX-8 and which RFC 3323 §5.1 puts in a B2BUA
([services-b2bua](../designs/services-b2bua.md)); connected-identity assertion in responses
(RFC 4916 — §14); and cryptographic identity (RFC 8224/STIR), which is a different mechanism with
a different trust model and is not a variant of this one.

**Upstream considerations** (AGENTS.md rule 6): considered for upstream — **no for the policy, yes
for two syntax primitives.** Whether an identity is asserted, which one, and toward whom has a
*trunk* as its subject, and the kernel has no trunks; that is platform orchestration and stays
here. Two primitives underneath it are protocol-generic and belong to the kernel on exactly the
argument that moved the `Headers` surgery API (upstream ledger `S-15`/PX-3):

1. **A parsed `Privacy` header.** RFC 3323 §4.2's grammar is closed apart from its `token` escape,
   and §4.2 also fixes that each legitimate value appears at most once. §4.2's constraints are
   header syntax, not policy. Every consumer that re-splits the raw value on `;` is shadow parsing.
2. **A typed `P-Asserted-Identity`/`P-Preferred-Identity` value list** that enforces RFC 3325
   §9.1's one-or-two rule and the scheme pairing, and that reads a message carrying the two values
   on one comma-separated line and on two header lines identically (RFC 3261 §7.3.1).

Both are candidate ledger rows for `CX-1` to file against sipx; the implementing story (RT-2's
trunk model) is what waits on them, not this spec.

## 3. The trust declaration

RFC 3325 §1 makes the whole mechanism conditional on a Trust Domain, and RFC 3324 §2.3 defines
membership operationally rather than aspirationally: `B` trusts `A` **iff** there is a secure
connection between them **and** `B` holds configuration saying `A` is a member. The second
condition is a configuration fact whose grain is the peer. A per-trunk identity policy is therefore
not a wrapper over a header — it is the deployment writing down where the edge of its trust domain
runs, one peer at a time.

| # | Rule |
|---|---|
| A1 | **Trust is declared per peer, and the default is `Untrusted`.** A trunk with no identity policy, or a policy that does not say, is a peer outside the trust domain. Fail closed: RFC 3325 §1 makes trust-domain nodes "responsible for withholding that identity outside of the Trust Domain", and a peer nobody vouched for is by construction outside it. |
| A2 | **`Trusted` carries its basis, because RFC 3324 §2.3 has two conditions and configuration is only one of them.** `TrustBasis` names what makes the connection secure. Its admissible values are §4's; the strength required "is defined in Spec(T)" (§2.3), so this spec does not rank them — it requires that one be written, refuses `Trusted` without one (`G-A3`), and puts the value in the effective-policy record (§12) so an audit reads a claim rather than infers one. |
| A3 | **Trust is a property of the peer, not of the direction.** The same peer's ingress face (an RT-8 scope) and egress face (an RT-2 trunk) carry the same trust value. A deployment that trusts a peer's messages but not its handling of ours, or the reverse, has two peers and declares two. Rationale: RFC 3324 §2.3's definition is symmetric in the secure connection and asymmetric only in who holds the configuration; a single-sided trust declaration would let a PAI in through a door it could not go out of. |

**What this discharges of `Spec(T)`.** RFC 3325 §1 requires a Trust Domain's `Spec(T)` to specify
eight things. This platform can answer five of them once, and three only per deployment — so those
three are configuration with no platform-wide default, and a deployment that has not answered them
does not start.

| `Spec(T)` item (RFC 3325 §1) | Where it is answered |
|---|---|
| 1. The manner in which users are authenticated | [registrar-auth](registrar-auth.md) — digest, and §5's principal is its output |
| 2. Securing communication among nodes within the Trust Domain | Deployment: the transport and its verification ([affinity-token](affinity-token.md) for what rides the message) |
| 3. Securing communication between UAs and nodes | Deployment; recorded per peer as `TrustBasis` (A2) |
| **4. How it is determined which hosts are part of the Trust Domain** | **`trust` on the peer (A1, A2). Required, no default.** |
| **5. The default privacy handling when no `Privacy` header is present** | **`privacy.when_absent` (A23). Required, no default (`G-A2`).** |
| 6. That nodes in the Trust Domain are compliant to SIP | Out of this document's reach — an operator's statement about the peer |
| 7. That nodes in the Trust Domain are compliant to RFC 3325 | Likewise; declaring a peer `Trusted` **is** that assertion, which is why A1 makes it explicit |
| **8. Privacy handling for identity as described in §7** | **§8's gate and §10's `critical` decision, parameterised per trunk.** |

## 4. Types

```rust
/// Everything the egress decisions of §7–§9 are permitted to read about the peer. There is
/// nothing else: no request, no clock, no store (A4).
pub struct TrunkIdentityPolicy {
    pub trust:   PeerTrust,
    pub assert:  Assert,
    pub privacy: PrivacyEgress,
}

pub enum PeerTrust {
    /// Outside this deployment's trust domain. The default, and the only value that needs no
    /// justification (A1).
    Untrusted,
    /// Inside it — RFC 3324 §2.3 condition 2, with `basis` recording condition 1 (A2).
    Trusted { basis: TrustBasis },
}

pub enum TrustBasis {
    /// TLS, with the peer identity the route plan already verifies (`Target::verify_as`).
    TransportTls,
    /// Mutual TLS: the peer authenticated to this platform as well.
    MutualTls,
    /// A network the operator asserts is private. Accepted, never inferred, and reproduced
    /// verbatim in the effective-policy record (§12). The note is required (`G-A8`).
    DeclaredPrivateNetwork { note: Bytes },
}

pub enum Assert {
    /// This platform creates no `P-Asserted-Identity` toward this peer. It says nothing about a
    /// field already in hand — that is §8's question, not this one (A19).
    Never,
    Identity {
        /// Tried in declared order; the first that yields a value wins (A14). At most one of
        /// each variant, and `Literal` only last (`G-A4`, `G-A5`).
        source:  Vec<IdentitySource>,
        form:    AssertForm,
        on_none: OnNoIdentity,
    },
}

pub enum IdentitySource {
    /// The identity the ingress step resolved for the authenticated principal (§5, A5).
    Principal,
    /// A `P-Preferred-Identity` the caller sent, honoured only if it is in the principal's
    /// assertable set (RFC 3325 §6, A7).
    PreferredHint,
    /// A `P-Asserted-Identity` that arrived from a peer this platform trusts (RFC 3325 §5, A6).
    ReceivedPai,
    /// The `From` URI **as received** — never as it will be sent (A10).
    ReceivedFrom,
    /// A literal declared on this trunk: the platform's own identity, not the caller's (A17).
    Literal(AssertedIdentity),
}

/// A filter on what a source may yield, not a converter (A15). Nothing is ever fabricated to
/// satisfy a form.
pub enum AssertForm { Sip, Tel, SipAndTel }

pub enum OnNoIdentity {
    /// Forward with no `P-Asserted-Identity`. RFC 3325 §5 makes creation a MAY, so omitting is
    /// always conformant. The default.
    Omit,
    /// Fail the branch with `403 Forbidden` — RFC 3325 §6's own sanctioned rejection.
    Reject,
}

pub struct PrivacyEgress {
    /// RFC 3325 §1 `Spec(T)` item 5, and §7's "MUST be specified in Spec(T)". Required (`G-A2`).
    pub when_absent:      WhenNoPrivacyHeader,
    /// Whether this trunk writes RFC 5379 §5.1.4's `From` form, and when (§9).
    pub anonymous_from:   AnonymousFrom,
    /// Whether this trunk announces `Privacy: id` toward the peer (A27).
    pub announce_id:      AnnounceId,
    /// What a request whose privacy cannot be performed gets, absent `critical` (A31).
    pub on_unperformable: OnUnperformable,
}

pub enum WhenNoPrivacyHeader { Include, Remove }
pub enum AnonymousFrom      { AsReceived, Rfc5379 }
pub enum AnnounceId         { Never, WhenWithheld }
pub enum OnUnperformable    { Forward, Reject }

/// One RFC 3325 §9.1 value set: at least one of the two is `Some`, and never two of a scheme.
pub struct AssertedIdentity { pub sip: Option<Uri>, pub tel: Option<Uri> }

/// What the ingress step (§5) established, once, for this request. §7–§9 read this and the trunk
/// policy, and nothing else.
pub struct IdentityFacts {
    /// [registrar-auth](registrar-auth.md) §5: `<tenant> ":" <username>`, when V7 produced one.
    pub principal:      Option<Principal>,
    /// The identities the platform will vouch for on that principal's behalf (A5). Resolved at
    /// ingress; never looked up at egress.
    pub assertable:     Vec<AssertedIdentity>,
    /// RFC 3325 §6's hint, retained as a fact after the header itself was removed (A7).
    pub preferred:      Option<AssertedIdentity>,
    /// 0..2 values, and empty unless the sender was trusted — A6 is why.
    pub received_pai:   Vec<AssertedIdentity>,
    pub received_from:  FromValue,
    pub privacy:        RequestedPrivacy,
    pub sender_trusted: bool,
}

pub struct FromValue {
    pub uri:                  Uri,
    pub display_name:         Option<Bytes>,
    pub tag:                  Bytes,
    /// The RFC 5379 §5.1.4 form arrived already anonymised by the UA.
    pub is_rfc5379_anonymous: bool,
}

/// RFC 3323 §4.2's closed set, plus the `token` escape it admits. A parsed value, never the raw
/// header bytes — the raw bytes are re-split by everyone who touches them and disagreed upon.
pub struct RequestedPrivacy {
    pub none: bool, pub id: bool, pub header: bool,
    pub session: bool, pub user: bool, pub critical: bool,
    pub unknown: Vec<Bytes>,
}

pub enum Emission {
    /// Emit exactly this, per §7.4's forms.
    Emit(AssertedIdentity),
    /// Emit none, and remove any `P-Asserted-Identity` in hand.
    Withhold(WithholdReason),
}

pub enum WithholdReason { AssertNever, NoIdentity, PrivacyId, PrivacyHeader, PolicyWhenAbsent }

pub enum FromDecision { AsReceived, Anonymise }

pub struct EgressDecision {
    pub pai:         Emission,
    pub from:        FromDecision,
    pub privacy_out: RequestedPrivacy,
    /// Which source produced the identity, or why none did. A synthesis nobody can explain at
    /// 03:00 is a synthesis nobody will keep — the same reason `Applied` exists in N's §3.
    pub trace:       AssertTrace,
}

pub enum Outcome {
    Forward(EgressDecision),
    /// `403` (A18) or `500` (A30, A31). Never a routing status: this decision is about identity,
    /// and `404` belongs to a normaliser that could not make a number acceptable (`N20`).
    Reject { status: u16, reason: Bytes },
}
```

## 5. Ingress — what the platform learns, and what it refuses to carry

| # | Rule |
|---|---|
| A4 | **Identity is established once, at ingress, and only *selected* at egress.** The ingress step runs at the `AfterAuth` boundary ([hook-framework](hook-framework.md) H4), where [proxy-behavior](proxy-behavior.md) §4 V7 has fixed the auth verdict, and publishes `IdentityFacts`. The egress decision is then one total, pure function: `fn egress_identity(policy: &TrunkIdentityPolicy, facts: &IdentityFacts) -> Outcome`. Its signature is the enforcement, not a comment about it — no store, no clock, no socket, no randomness, and no second look at the message. Two branches carrying the same `(policy, facts)` decide identically, which is what makes every row of §13 a fixture rather than a deployment (AGENTS.md rule 2). A deployment whose identity is not derivable from what ingress already established needs the asynchronous external hook (EX-6), which suspends at a declared phase — never a synchronous lookup at F5. |
| A5 | **The assertable set is what the platform will vouch for, and it is bounded at ingress.** For an authenticated request it is the set of identities the deployment holds for that principal ([registrar-auth](registrar-auth.md) §5); for an unauthenticated one admitted by source address (RT-8) it is empty. Every member satisfies RFC 3324 §2.2: a `sip`/`sips` URI's domain, and the owner of a `tel` URI's E.164 number, are within this deployment's trust domain — which is why `G-A6` refuses a literal outside the declared assertion domains. Where the set comes from is RG-1's and DP-1's; that it is resolved before egress and never after is this spec's. |
| A6 | **A `P-Asserted-Identity` received from a peer this platform does not trust does not survive ingress.** RFC 3325 §5 requires it replaced with a single URI of its own scheme family or removed; this spec removes it, because replacing it would mean asserting an identity chosen by the untrusted sender. `received_pai` is therefore empty whenever `sender_trusted` is false, and `IdentitySource::ReceivedPai` yields nothing on such a request. **`Privacy: none` does not save it.** RFC 3325 §7's `MUST NOT remove` governs an identity the platform legitimately holds — §5's own sentence scopes it to fields "that it generated itself, or that it received from a trusted source" — and a PAI from an untrusted sender is neither. Were it otherwise, two words in a header would be a complete identity spoof, and every rule below would be decoration. Same treatment, same reason, for a PAI on a response from an untrusted peer. |
| A7 | **`P-Preferred-Identity` is a hint that is checked, and a header that always goes.** RFC 3325 §6: the hint is honoured only when it is a member of the assertable set of A5; when it is not, it is ignored and selection continues down the source list (§7), which is §6's "add a P-Asserted-Identity header of its own construction" branch. `OnNoIdentity::Reject` is §6's other branch and produces the same `403 Forbidden` §6 names. Independently of any of that, and with no policy value able to change it: the header **MUST** be removed from every message this platform forwards (§6), whether or not the hint was used, whether or not the peer is trusted, and whether or not any identity was asserted. |

## 6. The egress pipeline — three steps, and why they sit where they do

### 6.1 The seam with number normalisation

[number-normalisation](number-normalisation.md) `N32` fixes one half of this seam: substitution
"rewrites the number inside the guarded field's **existing** URI; it never creates a field", and
"whether and how `PAssertedIdentity` is created belongs to `RT-7`". This spec is the other half,
and the two meet at a point rather than overlapping.

The ordering question is real and has an observable answer: **a `P-Asserted-Identity` created after
the trunk's normalisation profile has run has not been normalised, and one created before it has.**
A deployment that asserts a national-format number toward a carrier demanding E.164 would then have
to restate the E.164 rule inside its identity policy — the same requirement written in two places,
which is how the two drift. So the placement is fixed by rule, not left to an implementation.

| # | Rule |
|---|---|
| A8 | **Three egress steps, in this order, inside the window `N23` already reserves.** Per branch, after [proxy-behavior](proxy-behavior.md) §7 F2 (Request-URI ← target) and before F5 (`BeforeForward`, H9): <br>**E1 — identity** (§7, §8): decide whether a `P-Asserted-Identity` exists on this branch and whose identity it names; create, replace or remove accordingly. <br>**E2 — normalisation** (`N23`): the trunk's profile shapes whatever numbers remain. <br>**E3 — presentation** (§9): the `From` form of RFC 5379 §5.1.4 and the outgoing `Privacy` header. <br>E1 therefore settles the PAI question **before** `N23` runs, and E3 writes its constants **after**. |
| A9 | **The placement criterion, from which A8 follows.** A step runs **before** the trunk's normalisation profile iff its output is a *number whose shape is the trunk's business*, and **after** iff its output is a *constant an RFC fixes byte for byte*. An asserted identity is the first kind: it carries a telephone number and the carrier's format requirement applies to it exactly as it applies to the `From`. The anonymous `From` of RFC 5379 §5.1.4 is the second: §5.1.4 says "the rest of this header form is recommended as shown", so any transform applied to it is a departure from the recommended form. One criterion, both placements, and no ordering knob anywhere. |
| A10 | **Every source reads the ingress facts, never the outgoing message.** `IdentitySource::ReceivedFrom` reads `facts.received_from`, which was captured before any egress step ran. This is what closes the one genuine coupling between §9's `From` rule and §7's selection: a `From` this platform is about to anonymise on a branch can never become the identity it asserts on that branch, and it is closed by construction rather than by ordering. |
| A11 | **Synthesis is confined to out-of-dialog requests and to RFC 3325 §9.1's method set.** E1 creates a `P-Asserted-Identity` only on a request with no `To` tag and only for BYE, INVITE, OPTIONS, SUBSCRIBE, NOTIFY or REFER — §9.1's Table 2 addition marks ACK, CANCEL, REGISTER, INFO, UPDATE and PRACK `-`, and this platform does not add a header to a method the RFC excludes. The dialog bound is `N25`'s line, held deliberately in the same place: the peer already received the assertion on the dialog-forming request, and re-deriving it mid-dialog would need state that a mid-dialog request arriving at any edge does not carry. **The emission gate (§8) is not so confined** — it runs on every forwarded request and response, because withholding needs only the peer's trust value and the message's own `Privacy` header. |
| A12 | **What travels is two bits, and it travels in the message.** The caller's privacy request is established on the dialog-forming request and is not re-derivable from a later one, because a UA need not repeat `Privacy` on an in-dialog request. Two flags — `user` (does E3 anonymise the `From`?) and `id` (does §8 withhold?) — are contributed as a **one-octet module fact** into the F4 mint draft ([affinity-token](affinity-token.md) `M8`, the same `ContributeTokenFact` seam hook-framework H9 exposes) and returned by P2's `TokenFact` verdict on every later message of the dialog. One octet against §3's 64-byte sub-budget, gated at startup by affinity-token `G5` like every other declared fact. This is AGENTS.md invariant 5 taken literally: the alternative — a per-dialog record of who requested privacy — is the store PX-8 rejected and the affinity token exists to remove. When no token verified, there is no fact and the message's own `Privacy` header is the only input, which is the un-Record-Routed case where there is no later message to protect. |
| A13 | **Responses are gated, never synthesised.** A response carrying `P-Asserted-Identity` (RFC 5379 §4.1 marks the header `Rr`) passes §8's gate at `BeforeResponseForward` (H11) against the policy of the peer the response travels *to*. E1 never runs on a response: asserting a *connected* identity is RFC 4916's mechanism, not this one (§14). RFC 3323 §4.2 notes that "Criticality cannot be managed appropriately for responses", so §10 does not run on them either. |

## 7. Selection — which identity, and in what form

| # | Rule |
|---|---|
| A14 | **The source list is ordered, and the first source that yields a value wins.** Sources do not merge and do not fall back within a value: a source either produces an `AssertedIdentity` admissible under the declared `form` (A15) or produces nothing and the next source is tried. Declared order is applied order; there is no second ordering knob and no priority field. |
| A15 | **`form` filters what a source may yield; it never fabricates.** `Sip` admits only the `sip`/`sips` value, `Tel` only the `tel` value, `SipAndTel` both when both exist and whichever exists when only one does. A source holding no value admissible under `form` yields nothing and selection continues — so `form: Tel` on a trunk whose principal identity is a `sip` URI falls through to the next source rather than sending that carrier a scheme it does not read. RFC 3325 §9.1's one-or-two shape is therefore satisfied by construction: what is emitted is a subset of what was held, and what was held was already §9.1-shaped (§4). |
| A16 | **No display-name is ever synthesised.** RFC 3325 §4 permits one; this platform does not invent one, because a display-name is unverified free text and `P-Asserted-Identity` exists to carry a value that authentication produced (RFC 3324 §2.2). The one exception is not an exception to the principle: `IdentitySource::ReceivedPai` reproduces the value it received from a trusted peer byte for byte, display-name included, because that peer's Spec(T) compliance is what A1 declared. |
| A17 | **A literal is the platform's own identity, and the record says so.** `IdentitySource::Literal` is the "fixed number when no identity is available" case, and it is the one source whose value does not identify the caller. RFC 3324 §2.2 requires the asserted URI's domain — or its E.164 number's owner — to be within the trust domain, which a deployment's own presentation number is and a guess is not (`G-A6`). Every branch that used it is flagged `Literal` in the trace and counted in the effective-policy record (§12), because after that point a regulatory trace built on the header traces to this platform rather than to a subscriber, and that is a fact an operator must be able to see rather than deduce. |
| A18 | **`on_none` is the exhausted-list branch and has exactly two values.** `Omit` forwards with no `P-Asserted-Identity` — always conformant, since RFC 3325 §5 makes adding one a `MAY` — and is the default. `Reject` fails the branch with `403 Forbidden`, which is the status RFC 3325 §6 itself names. There is no third behaviour: "assert something anyway" is `Literal`, written on its own line where a reviewer sees it. |

### 7.1 The emitted forms

Two values are emitted as **two header lines**, matching RFC 3325 §10.1's own examples; a received
message carrying them on one comma-separated line is read identically (RFC 3261 §7.3.1), and the
count and order are preserved for `N4`. The `sip`/`sips` value precedes the `tel` value.

The URI is always wrapped in angle brackets — a `name-addr` with no display-name — regardless of
whether it carries parameters. Both that and the bare `addr-spec` form are `PAssertedID-value` per
RFC 3325 §9.1 (§10.1's example uses `addr-spec` for its parameterless `tel` URI), and choosing one
form unconditionally is what keeps the emitter from having two code paths whose difference is
invisible until a URI acquires a parameter (RFC 3261 §20).

## 8. The emission gate — trust × privacy

RFC 3325 keeps *creating* an identity (§5, paragraph 2) and *forwarding* one (§5 paragraph 3, §7)
apart, and so does this spec. Collapsing them into one switch is what makes "assert for regulatory
trace, and withhold from an untrusted peer" inexpressible — and that combination is the entire
reason §7 has an `id` token.

| # | Rule |
|---|---|
| A19 | **`Assert::Never` creates nothing and removes nothing.** It is a statement about synthesis alone. Whether a `P-Asserted-Identity` already in hand travels is decided by A20–A23, exactly as it is for a trunk that does assert. A deployment that wants no PAI to reach a peer under any circumstance writes `trust: untrusted` with `when_absent: remove`, which is A1's and A23's default pair and therefore what it already has. |
| A20 | **Toward a trusted peer, the identity travels.** RFC 3325 §5: a proxy "does not remove any P-Asserted-Identity header fields that it generated itself, or that it received from a trusted source" when forwarding to a node it trusts. `Privacy: id` does not withhold on such a branch, because §7's `MUST` is scoped to "elements that are not trusted". RFC 5379 §5.1.8 recommends removing even toward a trusted entity "unless it can be confident that the message will not be routed to an untrusted entity without going through another privacy service" — and declaring a peer `Trusted` under A1/A2 **is** that confidence, since RFC 3324 §2.3 makes membership conditional on Spec(T) compliance and RFC 3325 §1 item 8 puts privacy handling for identity inside Spec(T). A deployment that trusts a peer to route but not to honour `Privacy: id` has not met §2.3's condition 2 and declares that peer `Untrusted`. |
| A21 | **Toward an untrusted peer, `id` or `header` withholds — every value of it.** `id`: RFC 3325 §7's `MUST`, and where multiple values are present "all instances of the header field values MUST be removed". `header`: RFC 5379 §4.1 Table 1 marks `P-Asserted-Identity` `delete` in the `header` column, and §5.1.8 says a privacy service "should delete the P-Asserted-Identity headers when user privacy is requested with Privacy:header before it forwards the message to an entity that is not trusted". Withholding removes the header; it does not substitute an anonymous value in it. |
| A22 | **`Privacy: none` cannot be out-voted by configuration.** With `none` present, no `P-Asserted-Identity` is removed (RFC 3325 §7's `MUST NOT`) whatever `when_absent` says, and the `Privacy` header is forwarded byte-identical — RFC 3323 §4.2 twice: "Intermediaries MUST NOT remove or alter a Privacy header whose priv-value is 'none'", and "An intermediary MUST NOT modify the Privacy header in any way if the 'none' priv-value is already specified". So A27 adds nothing to it either. The `none` branch is evaluated before any policy value is read, which is what makes the rule structural rather than a check somebody can forget. It does **not** compel *creation*: RFC 3325 §5 leaves adding a header a `MAY`, and `none` forbids removal, not omission. Its one limit is A6's. |
| A23 | **With no `Privacy` header and an untrusted peer, `when_absent` decides — and it is required.** RFC 3325 §7 makes this the deployment's call and says it "MUST be specified in Spec(T)", so `G-A2` refuses a policy that omits it. A trunk with **no identity policy bound at all** is `Remove`, together with A1's `Untrusted` and A19's `Never`. This deviates from §7's "It is RECOMMENDED that the P-Asserted-Identity header fields SHOULD NOT be removed unless local privacy policies prevent it", deliberately and in one direction only: that recommendation presumes a peer the deployment has placed inside its trust domain, and an unconfigured peer is by construction one it has not. Taking §7's recommendation is one declared line (`when_absent: include`), visible in review, on a trunk that has also declared its trust. |
| A24 | **The gate is total, and §13's `AI-T` is its truth table.** For every `(trust, privacy, PAI-in-hand)` there is exactly one outcome, `AI-T` enumerates the product of the two axes that interact, and no other input reaches the decision. |

## 9. Anonymous callers, the `From` header, and why this is not an ordering question

The story this spec answers asks for the interaction with the anonymous-`From` rule to be
"specified, not left to ordering". It is not an ordering question, and RFC 5379 §4.1 is why.

### 9.1 Two questions, disjoint subjects

Read Table 1 of RFC 5379 §4.1 down the two rows that matter:

| Header | `user` | `header` | `session` | `id` |
|---|---|---|---|---|
| `From` | anonymize | — | — | — |
| `P-Asserted-Identity` | — | delete | — | delete |

The two rows share no column. `user` anonymises the `From` and does nothing to the
`P-Asserted-Identity`; `id` deletes the `P-Asserted-Identity` and does nothing to the `From`.
"Anonymous caller" is therefore **not one condition**, and a single flag conflating the two is
precisely the defect that makes the composition look order-dependent. There are two questions with
disjoint subjects, and their composition is a **product**, not a sequence.

| # | Rule |
|---|---|
| A25 | **The two questions are independent, and the platform answers both.** Q1 — *what does `From` carry?* — is triggered by `user` (Table 1) or by a `From` that arrived already anonymous, and is answered by `anonymous_from`. Q2 — *does `P-Asserted-Identity` leave, and carrying what?* — is §7's and §8's. Neither reads the field the other writes: §8 reads `facts.privacy` and the trunk's trust value, and Q1 reads `facts.privacy` and `facts.received_from`. Both read ingress facts (A10). So the two steps commute, the spec fixes the *outputs* rather than an order, and `AI-A-6` asserts the byte-identical result under both orders. `Privacy: id` alone therefore leaves the `From` untouched (`AI-A-3`) and `Privacy: user` alone leaves the `P-Asserted-Identity` in place (`AI-A-4`) — the two rows that show the axes are real. |
| A26 | **The anonymous `From` is RFC 5379 §5.1.4's form, and only the parts §5.1.4 names change.** The display-name becomes `"Anonymous"` and the URI becomes `sip:anonymous@anonymous.invalid` (RFC 3323 §4.1.1.3). The `tag` parameter is preserved byte-for-byte — §5.1.4: "The tag value varies from dialog to dialog, but the rest of this header form is recommended as shown" — as is every other byte of the header, and the `To` header is never touched (Table 1 does not list it). Rewriting the `From` **URI** is admissible because RFC 3261 §12.1.1 identifies a dialog by Call-ID and the two tags, which RFC 3323 §5.3 states in the same words; §5.3's B2BUA `SHOULD` is scoped to modifying *dialog-matching* headers, and the tag this rule preserves is the only dialog-matching part of the `From`. This is `N25`'s argument, and it is the same argument. The behaviour is per trunk and off by default (`AsReceived`): RFC 5379 §5.1.4's Note permits a privacy service to anonymise "based on local policy" — permits, not requires — so a trunk that anonymises has said so. |
| A27 | **`announce_id` says whether the peer is told.** `WhenWithheld` appends `id` to the forwarded `Privacy` header on exactly those branches where §8 withheld, so the peer's own downstream keeps withholding; `Never` forwards the header as received. RFC 3323 §4.2 sanctions an intermediary adding the header when "operating at a user's behest, for example if a user has an administrative arrangement with the operator of the intermediary" — a per-trunk declaration in the deployment's own configuration is that arrangement, which is the same "per-peer trust declaration" reading §3 takes of the whole policy. Constraints, all from §4.2: `id` appears at most once, so appending to a header already carrying it is a no-op; nothing is ever appended to a header containing `none` (A22); and the emitted syntax is `;`-separated per §4.2's ABNF. |
| A28 | **The platform does not strip priv-values it honours.** RFC 3323 §5's "SHOULD remove the corresponding priv-value" is a *privacy service* rule whose stated purpose is to stop another privacy service repeating the work. Repeating "do not send this identity" costs nothing and failing to repeat it is a leak — RFC 5379 §5.1.8 reasons the same way when it recommends deleting a PAI even toward a trusted entity absent confidence about the rest of the path. So `Privacy` travels as received, plus A27's append, minus nothing. The platform is not a privacy service (§10) and does not take on §5's bookkeeping, including its whole-header-removal `MUST` and its `Proxy-Require` cleanup, neither of which can be reached without first performing a strip. |

**The limit of A12, stated rather than discovered.** A mid-dialog request is anonymised on the
strength of the token fact of A12 or of a `Privacy` header the caller repeated. A dialog whose
Record-Route pair was never inserted — a request the platform did not stay in the path for — has
neither, and its later messages carry the `From` as received. Holding a dialog-scoped anonymity
record instead is the per-dialog store PX-8 rejected and invariant 5 forbids; the honest homes for
a stricter requirement are the token (which A12 already uses) or a B2BUA leg, where RFC 3323 §5.1
puts it.

## 10. `Privacy: critical` — the decision

**Decided: `critical` is supported, its performable set is declared per trunk, and a request whose
privacy this platform cannot perform in full is rejected with `500 (Server Error)`.**

The reasoning starts from what this platform *is*. RFC 3323 §5's `critical` obligation binds a
**privacy service**, and §5.1 says such a service "must frequently act as a transparent
back-to-back user agent" — which PX-8 declined for v1 and which vision principle 1 keeps out of the
proxy path. So this platform is not a privacy service. It is a proxy inside a Trust Domain
(RFC 3325 §1), and the one privacy function it performs on the proxy path — removing
`P-Asserted-Identity` on `Privacy: id` — is a *proxy* obligation under RFC 3325 §7, not a
privacy-service one.

Declining `critical` on that technicality would be the wrong answer anyway: a caller who wrote
`critical` asked to be rejected rather than exposed, and forwarding the request regardless is the
single outcome they ruled out. So the platform honours it, and bounds it by declaring exactly what
it can perform.

| # | Rule |
|---|---|
| A29 | **The performable set is `{ id }` plus `{ user }` on a trunk that declares `anonymous_from: rfc5379`, and nothing else.** `id` — RFC 3325 §7, A21. `user` — RFC 5379 §4.1 Table 1's only entry for `user` that this platform reaches is the `From` anonymisation of §5.1.4, which A26 performs *when the trunk declares it*, so performability is per trunk rather than platform-wide. **`header` is never performable**: it is Via, Contact and Record-Route obscuring (RFC 3323 §4.2), out of scope for v1 by PX-8 and pointed at a B2BUA by RFC 3323 §5.1. **`session` is never performable**: media anonymisation would depend on whether a call was anchored through a relay at all, and a privacy claim conditioned on another subsystem's runtime choice is a claim that is sometimes false. **An unregistered `token` (RFC 3323 §4.2) is never performable**, because the platform cannot know what it was asked to do. |
| A30 | **`critical` present and any requested priv-value outside A29's set ⇒ `500 (Server Error)`.** RFC 3323 §5: "if the privacy service is incapable of performing all of the levels of privacy specified in the Privacy header then it MUST fail the request with a 500 (Server Error) response code", restated by RFC 5379 §4.3. The reason phrase enumerates the priv-values that were not performed, which §5 says it SHOULD do. `Privacy: none;critical` is **not** a rejection: `none` requests no privacy functions, so the set of unperformable requested functions is empty and A30's condition is not met — a derivation from this rule rather than a special case, for a header RFC 3323 §4.2 tells user agents not to construct in the first place. |
| A31 | **Without `critical`, `on_unperformable` decides, and the default is `Forward`.** RFC 5379 §4.3 recommends rejecting with `500` even absent `critical`. This platform declines that by default and makes the recommendation available as one declared word, because taking it turns every optional privacy hint into a call failure at an element that never claimed to be a privacy service — and `critical` is the token a caller has precisely to demand the stricter behaviour. On `Forward`, the un-performed priv-value stays in the forwarded `Privacy` header (A28) for an element downstream that can honour it. `on_unperformable: reject` takes §4.3's recommendation, per trunk. |
| A32 | **`Proxy-Require: privacy` never reaches this spec.** The platform does not advertise RFC 3323 §4.3's `privacy` option-tag, so [proxy-behavior](proxy-behavior.md) §4 `V6` answers `420 Bad Extension` with `Unsupported: privacy` before any identity decision is made. That is RFC 3261's own mechanism for the case, and §4.3 says a UA sends the tag exactly so that "in the unlikely event that the user agent sends a request to an intermediary that does not support the extensions described in this document, the request will fail". Nothing here overrides it, and A29–A31 never see such a request. |

## 11. Binding and the configuration surface

Policies are named and referenced, never written inline at a binding site, so the same words cannot
say two things in two places — the shape [number-normalisation](number-normalisation.md) §10 uses,
for the same reason. DP-1 owns the file syntax and KO-8 the reload semantics; this is the data
model of §4 rendered in the platform's configuration language.

```yaml
identity:
  carrier-a:                              # an egress identity policy
    trust: untrusted
    assert:
      source: [principal, received_from, literal]
      literal: "sip:+4930000000@acme.example"
      form: sip
      on_none: omit
    privacy:
      when_absent: remove                 # required — RFC 3325 §1 Spec(T) item 5
      anonymous_from: rfc5379             # RFC 5379 §5.1.4
      announce_id: when_withheld
      on_unperformable: forward

  core-b:                                 # a peer inside the trust domain
    trust: { trusted: { basis: mutual_tls } }
    assert:
      source: [received_pai, principal]
      form: sip_and_tel
      on_none: omit
    privacy:
      when_absent: include
      anonymous_from: as_received
      announce_id: never
      on_unperformable: forward

trunks:
  - name: carrier-a
    identity: carrier-a
    normalisation: carrier-e164           # number-normalisation §9
```

That is the whole surface: a map of named policies, plus one name at each binding site. There is no
default policy and no built-in identity anywhere in the platform; a trunk that names none gets
A1/A19/A23's fail-closed triple, which is a stated policy rather than an absence of one.

## 12. Configuration validation, and the effective-policy record

Every check below fails the configuration **load**, naming the policy, the trunk and the rule. An
invalid policy fails a deployment, never a call.

| # | Check | Why |
|---|---|---|
| G-A1 | A trunk names an identity policy that does not exist | Naming the binding and the policy; no node starts on it |
| G-A2 | `privacy.when_absent` omitted | RFC 3325 §7 requires the decision "specified in Spec(T)"; there is no platform-wide answer (A23) |
| G-A3 | `trust: trusted` with no `basis` | RFC 3324 §2.3 has two conditions and configuration is only the second (A2) |
| G-A4 | `assert.source` empty, or a variant repeated | An empty list is `Never` written obscurely; a repeat is unreachable configuration (A14) |
| G-A5 | `Literal` anywhere but last in `source` | A literal always yields, so every later source is dead (A17) |
| G-A6 | A `literal` that is not a `sip`, `sips` or `tel` URI, that carries two values of one scheme, or whose `sip` host is not a declared assertion domain | RFC 3325 §9.1's shape, and RFC 3324 §2.2's requirement that the domain be within the trust domain (A17) |
| G-A7 | `anonymous_from: rfc5379` on a trunk whose bound normalisation profile guards `from` with `fallback: reject` | The trunk promises anonymity to callers its own profile rejects: a `From` that arrived already anonymous is `NotANumber` (`N6`), so the guard can never hold (`N31`) and `N18` rejects with `404` (`NN-E-5`). Error naming the trunk, the policy and the profile |
| G-A8 | `basis: declared_private_network` with an empty note | The one basis the platform cannot verify is the one that has to be justified in writing — the discipline `scripts/provenance-allow.txt` already applies to its own escape hatch |

**The effective-policy record.** Each trunk exports its resolved policy — every field of §4,
defaults included rather than assumed, and `TrustBasis` verbatim — as one structured record,
readable without reading the configuration that produced it. Per branch, the `AssertTrace` of §4
records which source produced the identity or which `WithholdReason` suppressed it, and branches
that used `IdentitySource::Literal` are counted separately (A17). Two questions have to be
answerable from the record alone, because both are asked under time pressure: *what do we assert
toward this carrier, and on whose authority?* and *which calls left carrying the platform's number
rather than a subscriber's?*

## 13. Test vectors

Every row is normative. Rows whose values are policies and facts are **decision-level**: executed
against `egress_identity` directly with no message in sight. Rows written as header lines are
**message-level**: bytes in, bytes out, through E1–E3 in the harness.

**Registration is deferred to `CF-8`, and the reason is not that it was forgotten.** The `AI`
prefix is not registered in `scripts/check-vectors.py`, and `docs/reference/vector-scope.toml` and
`docs/reference/conformance.md` carry no `AI` rows: those three files were fenced to another story
during the implementation wave that wrote this spec, which is the same wall `ME-1` and `AF-2` hit
and the reason `CF-8` exists. `CF-8`'s goal — every spec that carries a vector table — covers this
one by construction, but its inventory table was written before this spec existed and needs an
`AI` row adding alongside `LS`, `MR`, `AT`, `FR` and `HF`. The rows use the three-part `XX-Y-n`
shape already registered for `PB`, `EP` and `RA`, so they need registration only, not renumbering.
Every row here is deferrable on one reason — the types of §4 are a spec contract and no Rust exists
yet; `RT-2`'s trunk model is the story that lands them. Until `CF-8`, this table is unenforced,
which is exactly the condition that check exists to prevent.

**The fixture.** Trunk `carrier-a` with policy `carrier-a` exactly as §11 writes it, unless a row
says otherwise. The request is a dialog-forming INVITE:

```text
INVITE sip:+4930555000@edge.example SIP/2.0
From: "Anna" <sip:+4930111222@edge.example>;tag=a1
To: <sip:+4930555000@edge.example>
Call-ID: rt7-1@edge.example
CSeq: 1 INVITE
```

Principal `acme:anna`; assertable set `{ sip:+4930111222@acme.example, tel:+4930111222 }`.

**On "a vector per policy combination".** The declared axes are `trust` (2), the requested privacy
(RFC 3323 §4.2's five values plus absent and an unregistered token), the source list, and
`when_absent` (2). Their full cross product is not enumerated, and enumerating it would prove less
than the tables below do: `AI-T` is **exhaustive** over the two axes that interact — trust ×
privacy, which A24 declares total — while A19, A25 and A10 establish that the source list and the
`From` question are *independent* of it, each with its own rows. Independence proved once beats a
product enumerated once and re-enumerated wrongly on the next change.

**The emission gate (AI-T)** — exhaustive over `trust` × requested privacy, with a
`P-Asserted-Identity` in hand. `when_absent: include` unless the row says otherwise.

| # | Given | Expect |
|---|---|---|
| AI-T-1 | trusted peer, no `Privacy` header | `Emit` (A20) |
| AI-T-2 | trusted, `Privacy: none` | `Emit` — and the `Privacy` header forwarded byte-identical (A22) |
| AI-T-3 | trusted, `Privacy: id` | `Emit` — §7's `MUST` is scoped to untrusted elements, and declaring the peer trusted is RFC 5379 §5.1.8's "confidence" (A20) |
| AI-T-4 | trusted, `Privacy: header` | `Emit`, same reasoning (A20) |
| AI-T-5 | trusted, `Privacy: user` | `Emit` — Table 1 gives `P-Asserted-Identity` no treatment under `user` (A25) |
| AI-T-6 | untrusted, no `Privacy`, `when_absent: include` | `Emit` (A23; RFC 3325 §7's RECOMMENDED, taken by declaration) |
| AI-T-7 | untrusted, no `Privacy`, `when_absent: remove` | `Withhold(PolicyWhenAbsent)` (A23) |
| AI-T-8 | untrusted, `Privacy: none`, `when_absent: remove` | `Emit` — `none` out-votes the policy, never the reverse (A22) |
| AI-T-9 | untrusted, `Privacy: id` | `Withhold(PrivacyId)` (A21) |
| AI-T-10 | untrusted, `Privacy: header` | `Withhold(PrivacyHeader)` (A21, RFC 5379 §5.1.8) |
| AI-T-11 | untrusted, `Privacy: user` | `Emit` — `user` does not touch this header; the `From` is what it anonymises (A25) |
| AI-T-12 | untrusted, `Privacy: id`, two values in hand (`sip` + `tel`) | Both removed — RFC 3325 §7: "all instances of the header field values MUST be removed" (A21) |
| AI-T-13 | `assert: never`, trusted peer, PAI received from a trusted sender | Forwarded — `Never` creates nothing and removes nothing (A19) |
| AI-T-14 | `assert: never`, untrusted, `when_absent: remove`, PAI in hand | `Withhold(PolicyWhenAbsent)` — the same gate, unaffected by `Never` (A19) |
| AI-T-15 | untrusted, `Privacy: id;user` | `Withhold(PrivacyId)` **and** the `From` anonymised — both axes fire, neither because of the other (A25) |

**Selection (AI-S).**

| # | Given | Expect |
|---|---|---|
| AI-S-1 | `source: [principal, received_from, literal]`, principal resolved | `sip:+4930111222@acme.example`; trace `Principal` (A14) |
| AI-S-2 | Same, no principal (source-IP admission, RT-8) | The `From` URI's value; trace `ReceivedFrom` (A14) |
| AI-S-3 | Same, no principal, `From` already RFC 5379 §5.1.4-anonymous | `ReceivedFrom` yields nothing; falls through to the literal; trace `Literal` (A14, A17) |
| AI-S-4 | `source: [principal, received_from]`, neither yields, `on_none: omit` | No `P-Asserted-Identity`; the branch forwards; trace `NoIdentity` (A18) |
| AI-S-5 | As AI-S-4 with `on_none: reject` | `403 Forbidden` — the status RFC 3325 §6 names (A18) |
| AI-S-6 | `source: [preferred_hint, principal]`, hint `tel:+4930111222` ∈ assertable set | The hint; and `P-Preferred-Identity` removed from the forwarded request (A7) |
| AI-S-7 | As AI-S-6, hint `sip:+4930999999@acme.example` ∉ the set | Hint ignored, `Principal` used; the header still removed (A7, RFC 3325 §6) |
| AI-S-8 | `source: [received_pai, literal]`, PAI arrived from an **untrusted** sender | `ReceivedPai` yields nothing — A6 dropped it at ingress; the literal is emitted |
| AI-S-9 | As AI-S-8 but the sender was **trusted** | That value, display-name byte-identical (A16) |
| AI-S-10 | `form: sip_and_tel`, principal identity holds both | Two values, `sip` first (A15, §7.1) |
| AI-S-11 | `form: tel`, principal identity holds only a `sip` URI | `Principal` yields nothing; selection continues to the next source (A15) |
| AI-S-12 | `source: [literal]` | The literal; trace `Literal`, and the branch counted as platform-asserted in the effective-policy record (A17, §12) |

**Anonymous callers and `From` (AI-A).**

| # | Given | Expect |
|---|---|---|
| AI-A-1 | `Privacy: user`, `anonymous_from: rfc5379` | `From: "Anonymous" <sip:anonymous@anonymous.invalid>;tag=a1` — tag byte-identical (A26) |
| AI-A-2 | `Privacy: user`, `anonymous_from: as_received` | `From` byte-identical — §5.1.4's Note permits local-policy anonymisation, it does not require it (A26) |
| AI-A-3 | `Privacy: id` only, `anonymous_from: rfc5379`, untrusted | `From` byte-identical, PAI withheld — `id` is not `user` (A25, Table 1) |
| AI-A-4 | `Privacy: user` only, untrusted, `when_absent: include` | `From` anonymised **and** PAI emitted — the row that shows "anonymous caller" is not one flag (A25) |
| AI-A-5 | `From` arrives already in §5.1.4's form, `anonymous_from: as_received` | Forwarded byte-identical; nothing to do |
| AI-A-6 | AI-T-15's request (`Privacy: id;user`, untrusted) with E1 and E3's `From` step run in the opposite order | Byte-identical output either way — neither step reads the field the other writes (A25) |
| AI-A-7 | `Privacy: user`, `announce_id: when_withheld`, PAI emitted | No `id` appended — nothing was withheld (A27) |
| AI-A-8 | `Privacy: id`, untrusted, `announce_id: when_withheld` | PAI withheld; forwarded `Privacy: id` — one occurrence, not two (A27; RFC 3323 §4.2) |
| AI-A-9 | `Privacy: none`, `announce_id: when_withheld` | `Privacy: none` byte-identical, no `id` appended, PAI emitted (A22) |
| AI-A-10 | `anonymous_from: rfc5379`, `From` carries display-name `"Anna"` and `;tag=a1` | Display-name and URI replaced; every other byte, tag included, identical (A26) |
| AI-A-11 | In-dialog BYE toward the trunk, no `Privacy` header, dialog-forming request carried `Privacy: user` | `From` anonymised — the flag rode A12's token fact and P2's `TokenFact` returned it |
| AI-A-12 | The same BYE delivered to a **different** edge | Identical outcome — the fact is in the message, not in a node (A12, invariant 5) |

**The normalisation seam (AI-N)** — egress profile `carrier-e164` of
[number-normalisation](number-normalisation.md) §9 unless the row says otherwise.

| # | Given | Expect |
|---|---|---|
| AI-N-1 | `assert: never`; profile also guards `p_asserted_identity` with `fallback: to` | E1 creates nothing, so E2 sees `Absent` and traces `Skipped { GuardedFieldAbsent }` (`NN-G-10`); no PAI is forwarded |
| AI-N-2 | As AI-N-1 with that guard's `fallback: reject` | Still `Skipped`, **not** `Reject` (`NN-G-11`); the branch forwards |
| AI-N-3 | `source: [principal]` yielding `sip:4930111222@acme.example`; profile adds `p_asserted_identity` with `[{ ensure_prefix: "+" }]` | Forwarded `P-Asserted-Identity: <sip:+4930111222@acme.example>` — E1 created it, E2 shaped it (A8, A9) |
| AI-N-4 | As AI-N-3 but `Privacy: id` and an untrusted peer, and the profile guards `p_asserted_identity` with `fallback: reject` | Forwarded with no PAI and **no rejection**: E1 withheld before E2 ran, so the trunk's guard never sees a field and cannot fail a branch over a header that was never going to leave (A8) |
| AI-N-5 | `anonymous_from: rfc5379`, `Privacy: user`, profile normalises `from` with `[{ ensure_prefix: "+" }]` and no guard | `From: "Anonymous" <sip:anonymous@anonymous.invalid>;tag=a1` — E3 runs after E2, so the constant is written last and unshaped (A9) |
| AI-N-6 | As AI-N-5 but the `from` guard is `{ e164: global, fallback: to }`, and the `to` field is transformed `[{ ensure_prefix: "+" }]` | Forwarded as AI-N-5. Under the reverse order it would not be: E2 would see `anonymous`, `N6` would make it `NotANumber`, `N31` would make the guard never hold, and `N17` would substitute `To`'s phase-1 number into the `From` URI's user part — `From: "Anonymous" <sip:+4930555000@anonymous.invalid>`, the **callee's** number in the anonymous caller's `From`. The ordering rule is a privacy control, not a style choice (A9) |
| AI-N-7 | A trunk declaring `anonymous_from: rfc5379` bound to a profile whose `from` guard is `fallback: reject` | Configuration load fails, naming the trunk, the policy and the profile (`G-A7`) |

**`Privacy: critical` (AI-C).**

| # | Given | Expect |
|---|---|---|
| AI-C-1 | `Privacy: id;critical`, untrusted | Forwarded, PAI withheld — `id` is performable (A29) |
| AI-C-2 | `Privacy: header;critical` | `500 (Server Error)`, reason enumerating `header` (A30; RFC 3323 §5, RFC 5379 §4.3) |
| AI-C-3 | `Privacy: session;critical` | `500`, reason enumerating `session` (A29, A30) |
| AI-C-4 | `Privacy: user;critical`, trunk `anonymous_from: rfc5379` | Forwarded, `From` anonymised — `user` is performable **on this trunk** (A29) |
| AI-C-5 | `Privacy: user;critical`, trunk `anonymous_from: as_received` | `500`, reason enumerating `user` — performability is per trunk (A29) |
| AI-C-6 | `Privacy: id;fictional;critical` | `500`, reason enumerating `fictional` — RFC 3323 §4.2 admits extension tokens; the platform claims only what it performs (A29) |
| AI-C-7 | `Privacy: header` without `critical`, `on_unperformable: forward` | Forwarded; `header` stays in the forwarded `Privacy` for a downstream element (A28, A31) |
| AI-C-8 | As AI-C-7 with `on_unperformable: reject` | `500` — RFC 5379 §4.3's recommendation, taken per trunk (A31) |
| AI-C-9 | `Privacy: none;critical` | Forwarded, PAI emitted, header byte-identical — `none` requests no privacy functions, so nothing is unperformable (A22, A30) |
| AI-C-10 | `Proxy-Require: privacy` | `420 Bad Extension` with `Unsupported: privacy`, at proxy-behavior `V6`, before any rule here runs (A32) |

**Pipeline and binding (AI-P).**

| # | Given | Expect |
|---|---|---|
| AI-P-1 | Ingress from an untrusted source carrying `P-Asserted-Identity: <sip:+4930999@evil.example>` | Removed at ingress; `received_pai` empty; a trunk with `source: [received_pai, literal]` emits the literal (A6) |
| AI-P-2 | As AI-P-1 plus `Privacy: none` | Still removed — `none` protects an identity the platform legitimately holds, not one it must not hold (A6, RFC 3325 §5 vs §7) |
| AI-P-3 | Ingress carrying `P-Preferred-Identity`, hint unused | Removed from every forwarded copy (A7, RFC 3325 §6) |
| AI-P-4 | Fork to two trunks, one trusted and one untrusted, `Privacy: id` | The trusted branch carries the PAI, the untrusted branch does not; the upstream copy is untouched (A8) |
| AI-P-5 | In-dialog re-INVITE toward the trunk | No synthesis; the gate still runs (A11) |
| AI-P-6 | Out-of-dialog `OPTIONS` toward the trunk | Asserted — RFC 3325 §9.1's table marks OPTIONS `o` (A11) |
| AI-P-7 | `REGISTER` on a scope with an identity policy | No `P-Asserted-Identity` added — §9.1's table marks REGISTER `-`, and the registrar path is not this one (A11) |
| AI-P-8 | `200 OK` from the callee carrying `P-Asserted-Identity`, forwarded toward an untrusted ingress peer whose request carried `Privacy: id` | Withheld at `BeforeResponseForward` (H11) — RFC 5379 §4.1 marks the header `Rr` (A13) |
| AI-P-9 | The fixture INVITE retransmitted | Byte-identical forwarded message — `egress_identity` is a pure function of `(policy, facts)` (A4, proxy-behavior §10 `S4`) |
| AI-P-10 | A trunk with **no** identity policy bound | Nothing created; any PAI removed toward the peer; `Privacy` and `From` bytes untouched — A1/A19/A23's fail-closed triple |

**Byte-exact forms (AI-X).**

| # | Given | Expect |
|---|---|---|
| AI-X-1 | AI-A-1's forwarded `From` | `From: "Anonymous" <sip:anonymous@anonymous.invalid>;tag=a1` |
| AI-X-2 | AI-S-1's forwarded assertion, `form: sip` | `P-Asserted-Identity: <sip:+4930111222@acme.example>` |
| AI-X-3 | AI-S-10's forwarded assertion, `form: sip_and_tel` | Two header lines — `P-Asserted-Identity: <sip:+4930111222@acme.example>` then `P-Asserted-Identity: <tel:+4930111222>` (§7.1, RFC 3325 §10.1) |
| AI-X-4 | AI-A-8's forwarded `Privacy` | `Privacy: id` |
| AI-X-5 | `Privacy: user` received, PAI withheld, `announce_id: when_withheld` | `Privacy: user;id` — `;`-separated per RFC 3323 §4.2's ABNF, received order preserved, `id` appended (A27) |
| AI-X-6 | AI-S-1's assertion, the `From` carrying display-name `"Anna"` | `<sip:+4930111222@acme.example>` — no display-name is synthesised, and the `From`'s is not borrowed (A16) |

## 14. What this spec does not decide

| Not decided here | Where it belongs |
|---|---|
| Connected-identity assertion — a `P-Asserted-Identity` describing the *answering* party in a response | RFC 4916's `From`-change mechanism, a different negotiation with its own option-tag. A13 forecloses inventing a variant of it here; adopting RFC 4916 needs its own story |
| Cryptographic identity — signing the assertion rather than asserting it inside a trust domain | RFC 8224/STIR and RFC 8225. A different trust model, not a stronger setting of this one |
| The shape of the number inside any field this spec writes | [number-normalisation](number-normalisation.md), bound per trunk; §6.1 is the seam and A9 the ordering rule |
| Where the assertable set of A5 comes from, and how a principal maps to numbers | RG-1's store and DP-1's schema. This spec fixes only that it is resolved at ingress and never at egress (A4) |
| The ingress scope vocabulary the receiving side of A1/A6 is keyed on | RT-8 (source-IP admission as configuration) and RT-9 (scoped route selection) |
| `Privacy: header` — Via, Contact and Record-Route obscuring | Declined for v1 by PX-8; RFC 3323 §5.1 puts it in a B2BUA ([services-b2bua](../designs/services-b2bua.md)). A29 refuses to claim it rather than half-performing it |
| Which headers besides these reach a given carrier | RT-5's per-trunk egress header allowlist — the same grain, the same obligation, a different field set |
| A trunk-specific identity chosen by an external system per call | EX-6's asynchronous routing hook, which publishes a request-scoped fact that `IdentitySource` can then read at ingress. A synchronous lookup at F5 is the design smell A4 exists to forbid |
