# Registrar authentication

Normative. How a sipx-clstr edge decides whether it believes who a REGISTER says it is.

Status: accepted (`RG-2`). Prefix `RA`.

## 1. Normative references

- **RFC 3261** §22 (authentication, and what digest does *not* provide — §7), §22.4 item 4 (the
  ETag-based nonce construction does not apply to SIP — §6.1), §20.44
  `WWW-Authenticate`, §20.27 `Proxy-Authenticate`, §20.7 `Authorization`, §20.28
  `Proxy-Authorization`, §21.4.2 `401`, §21.4.8 `407`, §26.1.1 (registration hijacking),
  §26.2.1–§26.2.2 (TLS and the SIPS scheme).
- **RFC 7616** — HTTP digest, which SIP profiles: the `HA1`/`HA2` construction (§3.4.1–§3.4.3),
  `qop=auth`, `nc`, `cnonce`, `stale`, the SHA-256 families; §3.3 (the `nonce` parameter, its
  uniqueness and its example construction — §6.1), §5.12 (parameter randomness, the source that
  uniqueness needs); §5.3–§5.5 for the integrity and replay limits §7 turns into a decision.
- **RFC 2617** — the obsoleted form the deployed base still speaks, cited in §7 for the
  `request-digest` (§3.2.2.1) and `A2` (§3.2.2.3) constructions.
- **RFC 8760** — the additional algorithms and the downgrade warning that motivates §4.

## 2. What is here and what is not

The digest **primitives** are the kernel's (`sipx-ua::challenge`, upstream `S-16`): nonce
minting, challenge emission, the hash formulas, verification, and the replay window. This
platform does not reimplement them — see the [upstream ledger](../upstream.md). Two
implementations of one algorithm eventually disagree about who is authenticated, and the one that
disagrees quietly is a security bug.

What is here is the **policy**, which is a deployment's and therefore this platform's:

| Here | Not here |
|---|---|
| which tenants require authentication, and in which realm | how a digest is computed |
| where credentials come from | how a nonce is minted or recognised |
| the identity a successful REGISTER is recorded under | the replay window's data structure |
| which status a refusal carries, and what the response says | `stale` semantics |

**Authentication runs before REGISTER processing**, not inside it. It decides whether a request is
allowed to become a `RegisterCommand` at all; the location service then receives the authenticated
principal as an input fact ([location-service](location-service.md) §S3, and its `principal`
column). Authorization — *may this principal write this AoR* — is S4 there, and separate: a
correctly authenticated user may still have no business rewriting somebody else's bindings.

## 3. The decision

For a REGISTER arriving at an edge, in order:

1. **A1.** If the tenant's policy does not require authentication, the request proceeds with no
   principal. A binding written without authentication carries `principal: None`, which is a
   recorded fact rather than an absence — `RG-3`'s audit trail must be able to say *unauthenticated*
   rather than merely fail to say anything.
2. **A2.** If the request carries no `Authorization`, challenge it: `401` with a
   `WWW-Authenticate` naming the tenant's realm, a freshly minted nonce, the offered algorithm
   and `qop="auth"`.
3. **A3.** If the credentials name a realm that is not this tenant's, refuse with `403`. A realm
   is a protection space; answering another one is not a wrong password, and challenging again
   would loop.
4. **A4.** Look the username up in the tenant's credential store. **A username that does not
   exist takes the same path as a wrong password** — the request is refused exactly as A6 refuses
   one. Distinguishing them is a user-enumeration oracle and is not information the far end is
   entitled to.
5. **A5.** Verify. On success the request proceeds with the principal of §5.
6. **A6.** On a wrong digest, a foreign nonce, a `qop` mismatch, an algorithm this edge did not
   offer, or a replayed nonce-count: challenge again with `401` and a **fresh** nonce, `stale`
   absent. A client that has already answered once and is told `401` without `stale` will stop and
   ask a human, which is the correct outcome for a credential that is actually wrong.
7. **A7.** On an expired nonce with otherwise-correct credentials: challenge again with `401`, a
   fresh nonce and **`stale=true`**. The client re-computes and re-sends without prompting anyone.
   Conflating A7 with A6 makes every expiry look like a bad password, and users answer that by
   changing passwords that were fine.

**"Fresh", in A2, A6 and A7, means §6.1's `N1`** — a nonce no other challenge has been given — and not
merely "minted again". The two come apart exactly where it matters: a nonce that is a function of the
clock alone is re-minted identically inside one second, so a refusal issued in the same second as the
credential it refused re-offers the nonce it has just rejected, and A6's "challenge again with a fresh
nonce" silently becomes "challenge again with the same one". `N1` is what makes A6 and A7 mean what
they say.

A proxy authenticating a non-REGISTER request follows the same decision with `407`,
`Proxy-Authenticate` and `Proxy-Authorization` substituted throughout. The two must not be mixed:
a server that challenges with `401` and reads `Proxy-Authorization` authenticates nobody while
appearing to work.

**The signed `uri` is not compared to the Request-URI, and that is a decision.** The digest covers
the method and the URI the client signed; verification uses the signed one. RFC 7616 §3.4.6 notes
that the two can legitimately differ — a client may sign a canonicalised form, and a proxy may have
rewritten the Request-URI in flight — so refusing on a mismatch rejects correct clients. What the
mismatch could otherwise buy an attacker is bounded: the **method** is taken from the request and
not from the credentials, so a captured REGISTER credential authenticates only a REGISTER, and the
AoR a REGISTER writes comes from `To` rather than from anything the digest covers. The exposure *from the
mismatch* is therefore a credential aimed at a different registrar in the same realm within one
nonce lifetime. Revisit if realms ever span registrars with different trust.

That bound covers this paragraph's question and no more. **§7 states what the digest binds in
full**, and it is less than these two paragraphs on their own would suggest.

## 4. Algorithm selection

The edge offers **one** algorithm, SHA-256 by default, and refuses credentials computed under any
other (A6). It does not offer several and accept the strongest answered.

RFC 8760 §3 is explicit that offering MD5 alongside a modern algorithm "opens the system to the
potential for a downgrade attack by an on-path attacker": a challenge is not integrity-protected,
so an attacker who can reorder the header fields chooses which one the client answers. A
deployment that must interoperate with MD5-only endpoints configures MD5 **for that tenant** and
accepts what that means, rather than every tenant carrying the weaker option permanently.

## 5. The principal

A successful verification yields the principal recorded on the binding:

```
<tenant> ":" <username>
```

byte-exact as the client sent the username, with no case folding. The tenant is included because a
username is only unique within one, and a principal that could name two tenants' users is an
authorization bug waiting for a cross-tenant lookup.

## 6. The nonce: uniqueness, the secret, and what a multi-edge deployment inherits

### 6.1 `N1` — a nonce is unique per challenge

Normative. **Every challenge this edge emits carries a nonce no other challenge has carried.** Two
clients challenged in the same realm in the same second are two nonces, not one.

RFC 7616 §3.3 defines the parameter that way — a nonce is *"a server-specified string which should be
uniquely generated each time a 401 response is made"* — and it is restated here as a rule of this
specification because §6.2's properties do not imply it and are easy to mistake for it. A nonce that
is a pure function of the second it was minted in is unforgeable, is expiry-checkable without a
table, and still collides.

**Uniqueness is what makes the replay window a per-client fact.** The window is keyed on the nonce
string and holds one nonce-count per nonce (§3 A6, `RA-R-3`, `RA-R-5`). Two clients holding the same
nonce therefore share one counter, and the sharing is only observable where their two answers are
verified by the same window — one edge (§6.2's second bullet is the other direction of the same
coin). At that edge the first to authenticate at `nc=1` takes the count, and the second — a
different user, a correct password, legitimately also at `nc=1` — is a repeat of a count with a
different digest, which is `RA-R-2`: a replay. §3 A6 answers it with `401` and no `stale`, which this
specification chooses so that a client "will stop and ask a human". So a **correct** credential is
refused and its user is sent to change a password that was never wrong — the outcome §3 A7 exists to
prevent, reached through a door A7 does not watch. `RA-R-8` is the row.

**And it is reachable, not theoretical.** The condition is narrow but ordinary: one tenant that
requires authentication, two of its users challenged within the same second by the same edge, both
answering. It costs nothing to reach at a modest REGISTER rate, needs no attacker, and produces no
error anywhere — the edge believes it refused a replay. It became reachable when `tenant[].auth`
started producing a challenging registrar (`FC-3`); an open tenant (§3 A1) is untouched, and two
answers that land on two edges are untouched too, because the windows are separate (§6.2).

**What satisfies `N1`.** Per-challenge entropy in the nonce: draw random bytes at each challenge,
carry them in the nonce beside the issue time, and cover both with the MAC. RFC 7616 §5.12 asks for
exactly that source — server nonces *"should be generated by a strong random or properly seeded
pseudorandom source"*. It costs none of §6.2's properties: the nonce still carries its own issue
time, so expiry stays checkable, and the MAC still verifies from the secret and the realm alone, so
no table and no shared state appear.

**What does not: a timestamp alone, at any resolution.** RFC 7616 §3.3's example construction is
`timestamp H(timestamp ":" ETag ":" secret-data)`, in which the ETag is the per-challenge element —
and RFC 3261 §22.4 item 4 removes precisely that element for SIP: *"The example procedure for
choosing a nonce based on Etag does not work for SIP."* A SIP nonce that keeps the timestamp and
replaces the ETag with something **constant** — a realm, say — therefore has nothing left that varies
between two challenges, which is the shape this rule exists to forbid. A finer clock narrows the
collision window rather than closing it, and it closes nothing at all under the harness, where the
clock is pinned and a clock-derived nonce is byte-identical by construction.

`N1` is not §7.3's rejected option of binding the nonce to more of the *request*: entropy varies per
challenge, not per request, so re-registering a changed `Contact` against a live nonce stays ordinary
(RFC 3261 §10.2), and nothing about §6.2 is given up.

**Where `N1` is enforced, and its state today.** Nonce minting is the kernel's (§2), so this is a
requirement placed on `sipx-ua::challenge` rather than something this repository implements — the
[upstream ledger](../upstream.md) carries the row, the reproduction and the release it waits on.
Until a pinned kernel mints per-challenge entropy the rule is **stated and unmet**: `RA-R-8` is
deferred in [vector-scope.toml](../reference/vector-scope.toml) rather than passing, which is the
honest record of a normative rule the code does not yet keep.

### 6.2 The secret, and what a multi-edge deployment inherits

A nonce is verifiable from the secret and the realm alone, with no shared table — that is what
makes it usable at an edge. Two consequences follow, and both are properties of the deployment
rather than of this code:

- **Edges sharing a secret recognise each other's nonces.** A challenge issued by one edge can be
  answered at another, so the challenge/response round trip does not depend on transaction
  affinity.
- **The replay window is per-node.** A nonce-count replayed at a *different* edge than the one
  that first saw it is not caught. Recorded here rather than discovered in M2: closing it needs
  shared state, which is exactly the coupling the affinity design exists to avoid, so the
  M2 answer is likely to be a shorter nonce lifetime rather than a shared window.
- **The window is bounded, so it forgets.** It holds a fixed number of nonces and evicts the
  oldest. An evicted nonce is not rejected — it is treated as unseen, and its counts start over,
  so a captured credential for it becomes replayable again. The exposure is bounded by the nonce
  **lifetime** (§3 A7 expires it regardless) and by how much traffic it takes to evict, not by the
  window. The alternative is an unbounded window, which is a memory leak an attacker sizes by
  asking for challenges; a bounded one with a short lifetime is the better of the two.

An edge that generates its own secret at startup invalidates every outstanding nonce when it
restarts. Clients recover through `stale=true`, so the cost is one round trip and not a login.

## 7. What the digest binds, and what it does not

Normative, and written out rather than left to the reader because §3 and §8's `RA-R` rows are easy
to read as *"a captured credential cannot be reused"*, which is a stronger claim than the mechanism
makes.

### 7.1 The construction

With `qop=auth` — the only `qop` this edge offers (§3 A2, `RA-D-9`) — the response is
`KD(H(A1), nonce ":" nc ":" cnonce ":" qop ":" H(A2))` (RFC 7616 §3.4.1; RFC 2617 §3.2.2.1 is the
historical form it replaced), where

```
A2 = Method ":" request-uri
```

RFC 7616 §3.4.3, and RFC 2617 §3.2.2.3 for the same. `request-uri` here is the `uri` the **client**
signed and sent in its credentials, not the Request-URI as received — §3 says why the two are not
compared.

So for a REGISTER the digest binds exactly this, and nothing else:

| Bound | **Not** bound |
|---|---|
| the request **method**, taken from the request rather than from the credentials | `Contact`, and every parameter on it (`;expires`, `;q`, `;+sip.instance`) |
| the signed `uri` | `To`, and therefore the AoR the write lands on |
| realm, username and password, via `A1` | `Call-ID` and `CSeq` |
| the server `nonce`, the client `cnonce`, and `nc` | the `Expires` header |
| | the message body |

`qop=auth-int` would extend `A2` with `H(entity-body)` and change nothing else (RFC 7616 §3.4.3):
it covers the **body**. Every field in the right-hand column is a header field, and RFC 7616 §5.3 is
explicit — *"Most header fields and their values could be modified as a part of a man-in-the-middle
attack."*

### 7.2 The consequence

An `Authorization` captured from an authenticated REGISTER and reattached, unmodified, to a REGISTER
whose `Contact`, `CSeq` and `Expires` all differ hashes to the identical value. It does not merely
pass verification: it takes the **retransmission** branch of the replay window — the branch that
accepts a repeated `(nc, response)` pair on purpose, because that is what makes `RA-R-1` hold. To
the window the two events are the same event. `RA-R-6` pins this end to end, and pins what it
writes when the replayed REGISTER carries a single explicit `Contact`: the AoR then resolves to the
original contact **and** the attacker's, because RFC 3261 §10.3 step 7 adds a contact rather than
replacing the set. The phone that owns the AoR keeps ringing, so nothing about the victim's
experience says this happened — **for that variant.**

**The removal variant.** The same replayed `Authorization` is not limited to an explicit `Contact`.
Nothing in §7.1's bound-column covers `Contact`, `Expires` or `Call-ID`, so it hashes identically
when reattached to a REGISTER carrying `Contact: *`, `Expires: 0`, and a fresh `Call-ID` instead.
That request takes the wildcard path, not the additive one, and this registrar implements wildcard
removal as [location-service](location-service.md) §W3: *"`Call-ID` differs → remove"*, for every
stored binding at once, with no ordering check when the `Call-ID` is fresh — implemented at
`crates/sipx-clstr-registrar/src/process.rs:56-76`. A fresh `Call-ID` differs from every binding on
the AoR by construction, so W3's removal branch applies to all of them and none of B2/B3/B5's
ordering guard applies to any of them. The result is not a quiet fork: **every** binding on the AoR
is removed in one request, and the phone that owns the AoR stops ringing. RFC 3261 §26.1.1 names
exactly this as the point of registration hijacking — an attacker "could, for example, de-register
all existing contacts for a URI and then register their own device."

What is *not* weakened, and should not be re-derived by the next reader:

- the **method** comes from the request, so a credential captured from a REGISTER authenticates only
  a REGISTER (§3);
- the AoR a REGISTER writes to, or removes bindings from, comes from `To` and is gated by `S4` of
  [location-service](location-service.md) §5.1 — *is this principal authorized for this AoR* — so a
  substituted `To` is refused there, not here, for a wildcard REGISTER exactly as for an explicit
  one;
- the principal recorded on the binding (§5) is the one the digest proved, so the audit trail names
  the account whose credential was replayed rather than naming nobody.

### 7.3 The decision: accept, bounded by the nonce lifetime

**Accepted, re-affirmed against the corrected impact.** This edge does not narrow what `qop=auth`
covers, and this specification will not describe digest as an integrity mechanism. RFC 3261 §22: it
*"provides message authentication and replay protection only, without message integrity or
confidentiality. Protective measures above and beyond those provided by Digest need to be taken to
prevent active attackers from modifying SIP requests and responses."* That is the property offered
here, and the bound on it is the nonce lifetime (§3 A7, §6) — nothing else shortens the window.
Accepting a quiet fork and accepting that the same replay can empty an AoR outright (§7.2) are not
the same decision, so this is re-argued rather than carried over: nothing about `qop=auth` treats a
REGISTER that adds a contact differently from one that removes every contact, because both replay
the identical unmodified header over the same unbound fields (§7.1), and both are bounded by the
same nonce lifetime and the same mitigations below. There is no cheaper decision available that
keeps the additive case open while closing the removal one — the rejected options in the table below
were evaluated against "close the exposure `qop=auth` leaves," not against one variant of it, and
every reason they fail applies to the removal variant exactly as it does to the additive one.

**Residual risk, in the form an operator can act on:** anyone who can read a REGISTER off the wire
can, for as long as that nonce lives, replay its `Authorization` on a REGISTER of their own
choosing — one carrying their own `Contact` forks the AoR to them, and one carrying `Contact: *`,
`Expires: 0` and a fresh `Call-ID` removes **every** binding on the AoR instead (§7.2). Both are RFC
3261 §26.1.1's registration hijacking, and an operator should weigh the second, not the first: it is
loss of service, not merely unwanted company on the AoR. So **carry signalling over TLS** (RFC 3261
§26.2.1, §26.2.2) and **keep the nonce lifetime short**; on a cleartext hop, digest authenticates
the account and protects nothing else about the message, including whether the AoR keeps working
at all.

Rejected, each with the reason it was rejected:

| Option | Why not |
|---|---|
| `qop=auth-int` | Does not close it. `auth-int` adds `H(entity-body)` to `A2` (RFC 7616 §3.4.3) — the body. A REGISTER's binding set is entirely header fields and its body is normally empty, so this changes nothing here while costing interop with every client that implements `auth` alone. It is the first thing a reader reaches for, which is why it is written down rather than left out. |
| One-time nonces (RFC 7616 §5.5) | Would close it, and breaks `RA-R-1`. A UDP retransmission *is* the same `(nonce, nc, response)` arriving twice; a nonce honoured once refuses the second copy and drops a phone that did nothing wrong. M1's fifth exit criterion says it must not. |
| Binding the nonce to more of the request — RFC 7616 §5.4's "request-specific element" | Three reasons, any one sufficient. It is kernel machinery rather than policy: nonce minting and recognition are `sipx-ua`'s (§2), so it is an [upstream](../upstream.md) conversation and not a change this repository may make. It destroys §6.2's property that a nonce is verifiable from the secret and the realm alone, which is what lets any edge answer another edge's challenge without shared state. And it refuses correct clients — re-registering a *changed* `Contact` against a live nonce is ordinary (RFC 3261 §10.2). **This rejects binding to the *request*, and says nothing against §6.1's `N1`:** per-challenge entropy varies per challenge rather than per request, keeps both properties above, and is required rather than rejected. |
| A registrar-side check — remember the `Contact` first seen under a nonce and refuse a change | The same false refusals, plus the memory is per-node exactly as the replay window is (§6): it would hold at the edge that issued the challenge and not at the edge that receives the answer. A check that is a boundary at one edge and not at another is worse than no check, because it reads as a boundary. |

**Revisit when** a deployment needs a guarantee stronger than "TLS plus a short nonce lifetime" on
a hop that is not TLS-protected, or if `S4` turns out not to gate the AoR after all — that is the
one assumption above this specification does not itself prove.

## 8. Test vectors

Normative. `RG-2`'s tests derive from these.

**The decision (RA-D).**

| # | Given | Expect |
|---|---|---|
| RA-D-1 | Tenant does not require authentication; no credentials | Proceeds; `principal` is `None` (A1) |
| RA-D-2 | Tenant requires it; REGISTER carries no `Authorization` | `401` with `WWW-Authenticate`: this realm, a fresh nonce, `qop="auth"`, the offered algorithm, no `stale` (A2) |
| RA-D-3 | Correct credentials against a fresh nonce | Proceeds; `principal` is `<tenant>:<username>` (A5, §5) |
| RA-D-4 | Credentials naming another realm | `403`; not challenged again (A3) |
| RA-D-5 | Wrong password | `401`, fresh nonce, **no** `stale` (A6) |
| RA-D-6 | Username not in the store | Byte-identical to RA-D-5 — no enumeration oracle (A4) |
| RA-D-7 | A nonce this edge did not mint | `401`, fresh nonce, no `stale` (A6) |
| RA-D-8 | Correct credentials, expired nonce | `401`, fresh nonce, **`stale=true`** (A7) |
| RA-D-9 | Credentials answered without `qop=auth` | `401`, no `stale` (A6) |
| RA-D-10 | A proxy challenge rather than a registrar one | `407`/`Proxy-Authenticate`, and `Proxy-Authorization` is what is read (§3) |

**Algorithms (RA-A).**

| # | Given | Expect |
|---|---|---|
| RA-A-1 | Edge offers SHA-256; client answers SHA-256 | Proceeds (§4) |
| RA-A-2 | Edge offers MD5 for a legacy tenant; client answers MD5 | Proceeds — configured per tenant, not globally (§4) |
| RA-A-3 | Edge offers SHA-256; client answers MD5 over the same nonce | `401`, no `stale`; the weaker answer is refused rather than accepted (§4) |
| RA-A-4 | Edge offers SHA-512-256; client answers it | Proceeds (RFC 8760) |

**Replay and retransmission (RA-R).**

| # | Given | Expect |
|---|---|---|
| RA-R-1 | The same authenticated REGISTER arrives twice — same nonce, same `nc`, same digest | **Both authenticate.** A retransmission is ordinary over UDP and is not a replay |
| RA-R-2 | Same nonce and `nc`, a **different** digest — the same credential answered for another method or another signed `uri` | The second is refused (A6). Note what this is *not*: the window compares the digest, so it catches a reuse only when the reuse changed something the digest covers (§7) |
| RA-R-3 | `nc` counts up across refreshes on one nonce | Each authenticates |
| RA-R-4 | `nc` goes backwards | Refused (A6) |
| RA-R-5 | Many distinct nonces, more than the window holds | The window's memory does not grow with traffic. An evicted nonce loses its replay history and its counts start over — see §6, and note that it is *bounded by the nonce lifetime*, not unbounded |
| RA-R-6 | A captured `Authorization` reattached to a REGISTER whose `Contact`, `CSeq` and `Expires` differ, with the method and the signed `uri` unchanged | **Authenticates, under the captured principal, and the binding is written.** The digest is byte-identical, so this takes RA-R-1's branch and not RA-R-2's, and the AoR ends up forking to both contacts. Accepted deliberately, bounded by the nonce lifetime — §7.3 |
| RA-R-7 | The same captured `Authorization` reattached instead to a REGISTER carrying `Contact: *`, `Expires: 0` and a fresh `Call-ID`, with the method and the signed `uri` unchanged | **Authenticates, under the captured principal, and every binding on the AoR is removed.** The digest is byte-identical, so this takes RA-R-1's branch exactly as RA-R-6 does; [location-service](location-service.md) §W3 then removes every stored binding because the fresh `Call-ID` differs from all of them, with no ordering check. Loss of service, not a fork — RFC 3261 §26.1.1. Accepted deliberately, bounded by the nonce lifetime — §7.3 |
| RA-R-8 | **Two different users** of one tenant are challenged at the same instant — so, under a mint that carries no per-challenge entropy, with the byte-identical nonce — and each answers correctly at `nc=1`, both answers verified by the same edge | **Both authenticate.** This is §6.1's `N1` as a vector: two challenges, two nonces, two counters, so the second correct credential is not the first one's `nc` repeated with a different digest and `RA-R-2` never fires. Fails where the nonce is a function of the clock alone — one shared counter, and the second user gets A6's `401` without `stale` for a password that is right |

**The tenant boundary (RA-T).**

| # | Given | Expect |
|---|---|---|
| RA-T-1 | Two tenants, same username, different passwords | Each authenticates only against its own tenant's credential |
| RA-T-2 | Credentials valid in tenant A presented to tenant B | Refused — the realm differs, so RA-D-4 |
| RA-T-3 | One tenant requires authentication, another does not | Independent; the second is not weakened by the first (A1) |
