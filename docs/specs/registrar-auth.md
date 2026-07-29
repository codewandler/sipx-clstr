# Registrar authentication

Normative. How a sipx-clstr edge decides whether it believes who a REGISTER says it is.

Status: accepted (`RG-2`). Prefix `RA`.

## 1. Normative references

- **RFC 3261** §22 (authentication), §20.44 `WWW-Authenticate`, §20.27 `Proxy-Authenticate`,
  §20.7 `Authorization`, §20.28 `Proxy-Authorization`, §21.4.2 `401`, §21.4.8 `407`.
- **RFC 7616** — HTTP digest, which SIP profiles: the `HA1`/`HA2` construction, `qop=auth`,
  `nc`, `cnonce`, `stale`, the SHA-256 families.
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
AoR a REGISTER writes comes from `To` rather than from anything the digest covers. The exposure is
therefore a credential aimed at a different registrar in the same realm within one nonce lifetime.
Revisit if realms ever span registrars with different trust.

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

## 7. Test vectors

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
| RA-R-2 | Same nonce and `nc`, **different** digest | The second is refused (A6): a captured credential reused against a different request |
| RA-R-3 | `nc` counts up across refreshes on one nonce | Each authenticates |
| RA-R-4 | `nc` goes backwards | Refused (A6) |
| RA-R-5 | Many distinct nonces, more than the window holds | The window's memory does not grow with traffic. An evicted nonce loses its replay history and its counts start over — see §6, and note that it is *bounded by the nonce lifetime*, not unbounded |

**The tenant boundary (RA-T).**

| # | Given | Expect |
|---|---|---|
| RA-T-1 | Two tenants, same username, different passwords | Each authenticates only against its own tenant's credential |
| RA-T-2 | Credentials valid in tenant A presented to tenant B | Refused — the realm differs, so RA-D-4 |
| RA-T-3 | One tenant requires authentication, another does not | Independent; the second is not weakened by the first (A1) |
