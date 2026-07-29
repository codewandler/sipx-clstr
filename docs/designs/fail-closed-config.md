# Design: Fail-closed configuration

**Status:** proposed · **Pillar:** Cluster · **Epic:** `fail-closed-config` ·
**Stories:** FC-1 … FC-4

## Why

The configuration document is now the *only* configuration surface (`DP-8` wrote the loader, `DP-10`
replaced the flags with it). Four of the keys it accepts are security-relevant policy, and nothing
applies any of them. A document that declares TLS, authentication, a domain allow-list and a binding
quota loads without an error, without a warning, and produces a node with none of the four.

This is not a missing feature. A missing feature is a key the schema does not have. This is a
configuration surface that **fails open**: it takes the operator's declaration, calls the document
valid, and serves the opposite. [cluster-config](../specs/cluster-config.md) §3 D6 already forbids
it, in words written before the defect existed:

> It MUST NOT parse a document it does not fully implement — not on a best-effort basis, not by
> ignoring what it does not recognise. **A half-understood security posture is worse than a node that
> will not start.**

The same principle is already applied one seam over, and applied well: a `locationStore` that cannot
be reached stops the node (`RG-12`, `NodeError::LocationStoreUnreachable`), on the explicit grounds
that a registrar which fell back to memory "would come up healthy, answer `200` to everything, and
serve bindings no peer can see — and nothing would say so." Every word of that argument transfers to
authentication and to transport. It was not transferred.

### What was measured, not guessed

Each of these was reproduced against a build of `HEAD`, by running the binary and speaking SIP to it.

| Declared in the document | What the node does | Proof |
|---|---|---|
| `tenant[].auth` — `required: true`, realm, SHA-256, credentials | runs `TenantAuth::open` | credential-less REGISTER answered `200 OK` |
| `transport: tls` (with `certRef`/`keyRef`) | binds **cleartext UDP** | plaintext UDP REGISTER on the "TLS" listener answered `200 OK` |
| `tenant[].domains: [example.test]` | not enforced | REGISTER for `attacker.invalid` answered `200 OK` |
| `tenant[].maxBindingsPerAor`, `expiry` | discarded; hardcoded defaults apply | quota holds at `TenantPolicy::default()`'s 10 |

Two aggravating factors, both also measured.

**The loader advertises the keys it ignores.** A typo'd `authh` is refused with
`expected one of: name, id, domains, auth, expiry, maxBindingsPerAor` — so `closed_world`'s error
message teaches the operator that `auth` is real, and then the loader drops it. A validator that
rejects typos is *evidence*, to a reasonable reader, that accepted keys are applied.

**The mechanism built to catch exactly this never fires.** `startup.rs` warns when a document names
sections the build cannot apply, and its comment says why: "worth saying out loud at startup rather
than discovering as behaviour that never happens." It fires into nothing. `from_document` runs before
`tracing_subscriber::try_init()`, so no subscriber exists yet; and `DEFERRED_SECTIONS` lists
*top-level* cluster keys only, so `tenant[].auth` and `listener[].tls` — sub-keys of sections the
loader does descend into — would not be covered even with a working subscriber.

The transport case is the worst of the four, because it does not merely omit a protection: it inverts
one. [registrar-auth](../specs/registrar-auth.md) §7.3 makes "carry signalling over TLS" its primary
normative mitigation, and §7.2 computes the residual risk of not doing so — on a cleartext hop a
captured `Authorization` replayed with `Contact: *`, `Expires: 0` and a fresh `Call-ID` removes every
binding on the address-of-record (RA-R-7). The node accepts the configuration that asks for the
mitigation and serves the risk.

## Approach

Three rules, and the stories are their consequences.

**1. Accepted means applied, or refused. There is no third state.** For every key on a
`closed_world` allow-list, exactly one of: the loader projects it and something consumes it, or the
loader refuses the document naming the key and the rule id. "Parsed into a struct field that nothing
reads" is the state this epic exists to remove — `TenantSpec.domains` is that state today, and it is
indistinguishable from enforcement to anyone reading the document.

**2. The closed world has to reach sub-keys.** §8 V2's closed world is currently enforced per
mapping, which catches a typo at any depth but says nothing about whether a *recognised* key at depth
is consumed. Deferral is tracked at one level and the security-relevant keys live at two. Whatever
replaces `Config::deferred` has to be able to name `tenant[].auth`, not just `registrar`.

**3. Refusal beats a warning wherever the key changes the security posture.** A warning is the right
answer for a section this node's roles do not consume (§4 R5's projected-away case). It is the wrong
answer for authentication and transport, because the operator who declared them has already decided
the node should not run without them. §3 D6's last sentence is the tie-breaker, and it does not say
"warn".

### Sequencing, and why FC-2 is not last

`FC-2` — making unapplied configuration visible — is worth doing first even though it fixes no
security posture by itself. It is one line of ordering plus a widened check, it is the only reason
the other three defects were invisible for a release, and it is what stops the next one being
invisible too. `FC-1` outranks it only because a silently-cleartext TLS listener is the finding an
operator can be hurt by today, and the published deployment guidance actively points at it.

`FC-3` and `FC-4` are the two halves of `tenant[]`. They are separate stories because `FC-3` is a
security-behaviour change that `DP-10` deliberately declined to fold into itself — "it deserves its
own failing-first test and its own line in the changelog" — and `FC-4` is resource policy that can
land without touching the auth path.

## Out of scope, named so it is not lost

- **The credential model.** `CredentialStore::password() -> Option<String>` makes recoverable
  plaintext the *contract*, not one store's shortcut: the kernel recomputes the digest from the
  cleartext, so no HA1-only store can satisfy the trait. Fixing that is a trait change plus an
  upstream conversation, and `FC-3` wires what exists rather than redesigning it. Noted in `FC-3`.
- **Where credentials come from.** §8 V9 forbids inline secrets and there is no `credentialSource`
  resolution at all, so `FC-3` has to decide how a document names credentials without inlining them.
  That decision belongs in this epic; a credential *store* belongs to `registrar-location`.
- **Nonce lifetime and algorithm.** `with_lifetime`/`with_algorithm` exist and are unreachable, and
  nonce lifetime is not a field in cluster-config at all — which matters because §7.3 names a short
  nonce lifetime as one of its two operator mitigations. `FC-3` adds the fields or records why not.
- **The chart's `cluster` tree**, which does not load at all — `KO-14` owns that, and this epic's
  rule 1 is the reason it matters.

## What a reader should not conclude

None of this makes the node secure. Authentication that is wired is still one tenant's digest over a
cleartext hop unless `FC-1`'s TLS path is also real, and the published documentation still describes
a CLI that no longer exists (`DX-13`). This epic makes the configuration surface *honest* — a
declaration is applied or the node refuses to start — which is a precondition for the security
claims, not one of them.
