# Registrar authentication

Normative. How a sipx-clstr edge decides whether it believes who a REGISTER says it is.

Status: accepted (`RG-2`). Prefix `RA`.

## 1. Normative references

- **RFC 3261** §22 (authentication, and what digest does *not* provide — §7), §20.44
  `WWW-Authenticate`, §20.27 `Proxy-Authenticate`, §20.7 `Authorization`, §20.28
  `Proxy-Authorization`, §21.4.2 `401`, §21.4.8 `407`, §26.1.1 (registration hijacking),
  §26.2.1–§26.2.2 (TLS and the SIPS scheme).
- **RFC 7616** — HTTP digest, which SIP profiles: the `HA1`/`HA2` construction (§3.4.1–§3.4.3),
  `qop=auth`, `nc`, `cnonce`, `stale`, the SHA-256 families; §5.3–§5.5 for the integrity and replay
  limits §7 turns into a decision.
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

**The replay window's cost is the kernel's too, and it is not free.** `sipx-ua`'s window is a
`VecDeque` scanned front-to-back (`challenge.rs:194,388` at the pinned `v0.7.0`) while the nonce
being checked is always the newest entry at the back, so every authenticated request walks all 4096
entries under whatever lock the caller holds. That is a defect in an auth primitive, so this
platform may not fix it — it is filed in the [upstream ledger](../upstream.md) as
*A replay window that is O(1) on the hot path*, and `RG-15` recorded it there rather than building a
second window here. What this platform owns of the same hot path — its credential store — is `O(1)`
since `RG-15`.

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

## 6. The nonce secret, and what a multi-edge deployment inherits

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
| Binding the nonce to more of the request — RFC 7616 §5.4's "request-specific element" | Three reasons, any one sufficient. It is kernel machinery rather than policy: nonce minting and recognition are `sipx-ua`'s (§2), so it is an [upstream](../upstream.md) conversation and not a change this repository may make. It destroys §6's property that a nonce is verifiable from the secret and the realm alone, which is what lets any edge answer another edge's challenge without shared state. And it refuses correct clients — re-registering a *changed* `Contact` against a live nonce is ordinary (RFC 3261 §10.2). |
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

**The tenant boundary (RA-T).**

| # | Given | Expect |
|---|---|---|
| RA-T-1 | Two tenants, same username, different passwords | Each authenticates only against its own tenant's credential |
| RA-T-2 | Credentials valid in tenant A presented to tenant B | Refused — the realm differs, so RA-D-4 |
| RA-T-3 | One tenant requires authentication, another does not | Independent; the second is not weakened by the first (A1) |

**The audit trail (RA-L).** The rules are §9, which follows this table because §8 is where every
vector in this specification lives.

| # | Given | Expect |
|---|---|---|
| RA-L-1 | A REGISTER refused under A6 | One record, at `warn`, naming the status and the reason §3 already computed (§9 L1). None of the presented password, nonce, `cnonce`, response digest or username appears in it (§9 L2) |
| RA-L-2 | A REGISTER challenged under A2 | One record, and **not** as a refusal: nothing was wrong, nothing was offered (§9 L1) |
| RA-L-3 | A REGISTER that proceeds under A1 | One record saying *unauthenticated*, rather than no record (§9 L3) |
| RA-L-4 | A REGISTER whose credentials verify under A5, and whose `Contact` then fails to parse | The success record is still emitted, naming §5's principal. §3's outcome does not depend on what happened after §3 (§9 L1) — and the same holds one step down for an A1 proceed that then fails to parse |

## 9. The audit trail

Normative. What an authentication decision must leave behind, and what it must not.

Added by `RG-15`. Until then §3's decision computed *why* it refused — `ChallengeResponse::because`
is documented "Why, for logs and tests" — and the driver discarded it, so no `401`, `403` or success
anywhere in the register path produced a single line. §3 A1 already justified the open-tenant
principal by "`RG-3`'s audit trail must be able to say *unauthenticated*", and nothing wrote that
anywhere.

1. **L1.** Every outcome of §3 produces exactly one record, naming the outcome and, for a refusal,
   the reason. A2's challenge is an outcome and is **not** a refusal — it is the first half of a
   round trip the client is expected to complete, and recording it as trouble buries the real thing
   under every phone's ordinary first REGISTER. This is the only defence §6's exposure has: with no
   rate limiting and a 300-second nonce lifetime, a refusal nobody can count makes brute force
   against a tenant both undetectable and unbounded.

   **The record is owed for the decision, not for what follows it.** A request that A5 authenticates
   and that then fails to become a `RegisterCommand` — a malformed `Contact`, a missing `Call-ID` —
   has still had an outcome under §3, and it is recorded with the principal §3 proved. An
   implementation that reads the outcome off whatever the *whole* admission returned will lose
   exactly this case, because the rejection it ends at looks identical to one that never
   authenticated. `RG-15` shipped that bug and `RA-L-4` is why it cannot come back.
2. **L2.** A record carries **no credential material**: not the password, not the nonce, not the
   `cnonce`, not the response digest, and not the presented username. Nothing has been proved about
   a request that failed, so every string in its record comes from a closed set this platform owns.
   The mechanism is a return type, not a discipline — the reason is a `&'static str`, which cannot
   carry a runtime value however carelessly a driver writes the line. Same argument as
   `cluster-config` §8 V9's `dsnRef`: a log line is the artefact most likely to be copied into an
   issue.
3. **L3.** A success records the principal of §5 — the identity the digest **proved**, which the
   binding is already stored under, so naming it adds no exposure. A proceed under A1 records
   *unauthenticated* explicitly. An absent record and an unauthenticated one are different facts,
   and a trail that cannot tell them apart cannot answer "who wrote this binding".

**What the record identifies the far end by** is the source address the driver observed, not
anything the message claimed. It is the field an operator acts on, and it is the one field in the
record that no attacker chooses.
