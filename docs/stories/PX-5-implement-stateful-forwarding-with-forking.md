---
id: PX-5
title: Implement stateful forwarding with forking
pillar: Signalling
status: done
priority:
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: M1 #5 · stateful forwarding and forking; found a loop-detection defect in PX-1 §6
---

# Implement stateful forwarding with forking

## Goal
Implement transaction-stateful forwarding (§16.2): one server transaction, N branches, response context, and Record-Route insertion carrying the affinity token.

## Acceptance
- [x] Parallel and serial forking vectors pass, including best-response selection (§16.7) and provisional forwarding.
- [x] Record-Route insertion is implemented with the token as an opaque placeholder value; 
AF-5 swaps in the real mint/verify library in M2 (no dependency on AF-4 here).
- [x] Branch failure (transport error, timeout) advances or concludes the context per the spec.

## Progress
`sipx-clstr-proxy`, 47 tests. Five modules, each answering a numbered section of the spec:
`config` (identities, the cookie key), `cookie` (§6), `validate` (§4, V1–V7), `forward` (§7,
F1–F11), `context` (§8, R1–R11 and the orchestration).

**Every vector run:** `PB-V-1` … `PB-V-9`, `PB-P-2` … `PB-P-4`, `PB-F-1` … `PB-F-5`, `PB-R-1` …
`PB-R-10`. `PB-C-*` is `PX-6`, `PB-S-*` is `PX-4` (M2), `PB-A-*` needs a second node (M2).

**Effect order is asserted, not just produced.** `PB-F-1` checks the *sequence*
`[ResolveTargets, Forward, SetTimer]`: a Timer C armed before its INVITE went out would measure the
wrong interval, and only an ordered assertion catches that.

**The things that are easy to get subtly wrong, and how they are pinned:**

- **`Max-Forwards: 0` refuses every method, including OPTIONS** (`PB-V-2`). A proxy that answered
  OPTIONS on the target's behalf leaks topology and misleads whoever is diagnosing the call.
- **A branch `503` never reaches the caller as `503`** (`PB-R-8`, `PB-R-9`): it becomes `500`,
  because the caller must not be told the destination is unavailable when what happened is that we
  could not get there.
- **A second `2xx` for one INVITE is forwarded too** (RFC 6026, `PB-R-4`) — two 200s is a fork, not
  a bug, and dropping the late one leaves a UAS in a call nobody answered.
- **A re-INVITE is not Record-Routed.** RFC 6141: mid-dialog Record-Route does not alter an
  established route set, so adding one is noise that also costs a token's worth of bytes on every
  renegotiation.
- **`401`/`407` aggregate every challenge from every challenging branch** (`PB-R-6`). A UAC that
  saw one realm cannot satisfy the others.
- **The token budget is per *parameter*, not per header line** — the affinity-token spec's own
  correction to `PB-F-1`'s shorthand. Asserting on the line would reject a compliant token. An
  oversized token is refused rather than truncated, because a truncated token fails verification at
  the next hop, later and more confusingly.
- **The cookie key does not print itself.** A key that appears in a log is not a key.

## Open question for PX-1: §6's cookie cannot detect a loop

**`PX-1` §6 lists "topmost incoming `Via`" among the loop-detection cookie's fields. Including it
makes V4 structurally unable to fire.** The argument is a proof, not a preference:

1. Pass one: the request arrives with the caller's `Via` on top. The cookie is `C₁`. We forward with
   our own `Via` — whose branch carries `C₁` — pushed in front of it.
2. The request loops back to us. The topmost `Via` is now *ours from pass one*.
3. We recompute over the current state. The top `Via` changed, so the cookie is `C₂ ≠ C₁`.
4. V4 looks for one of our `Via` entries carrying a cookie equal to `C₂`. Ours carries `C₁`. No
   match → judged a spiral → forwarded → round the cycle until `Max-Forwards` expires at every node
   on it.

The topmost `Via` decides where the **response** goes, not where the **request** is routed, so it is
not part of "all information affecting processing of a request" (RFC 3261 §16.3 step 4). RFC 3261
§16.6 step 8 does recommend it — as *entropy*, for the part of the branch that must be unique per
client transaction.

**Implemented accordingly:** the cookie covers Request-URI, To tag, From tag, `Call-ID`, `CSeq`
number and the `Route` sequence; the topmost `Via` feeds the branch's unique part instead. Two tests
pin both halves — `the_topmost_via_does_not_change_the_cookie` and
`the_topmost_via_does_change_the_branch` — and `PB-V-6` now asserts an actual `482` end to end,
which it could not have done under §6 as written. **§6 needs the correction.**

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md), seam settled by
  [proxy-transaction-driver](../designs/proxy-transaction-driver.md).
- `hmac` + `sha2` joined the workspace for the keyed cookie — the same crates the kernel already
  uses, so there is one hash stack rather than two. The key is normalized to 32 bytes so HMAC's
  length precondition holds by construction and there is no impossible error branch to invent
  behaviour for.
- `pop_via` rebuilds the header collection to remove one `Via`, exactly as the smoke-test stub does.
  Third site now; sipx `S-15` removes all of them.
- Timer C is *armed* (F11) and *reset* (R4) here because both are part of forwarding. What happens
  when it **fires** is `PX-6`, and `on_input` returns nothing for it rather than guessing — a
  visibly missing case beats a partial one.
