---
id: AF-5
title: Round-trip tokens through Record-Route and Route
pillar: Cluster
status: in-progress
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, proxy]
note: round trip lands; the Path variant waits on affinity-token M7's direction value, which the spec defers to M3 — not on the kernel
---

# Round-trip tokens through Record-Route and Route

## Goal
Carry the token through the proxy: minted into Record-Route on dialog-forming requests, verified from Route on mid-dialog requests — at any edge, with zero lookups.

## Acceptance
- [x] A dialog-forming request through edge A yields a Record-Route token; the mid-dialog request arriving at edge B routes on it with the cross-node lookup counter at zero (harness scenario).
- [ ] The Path variant lands once the upstream typed Path header exists.

## Progress

**The round trip is closed and the M2 counter reads zero.** `crates/sipx-clstr-sim/tests/token_round_trip.rs`
is the scenario: alice calls through **edge A**, which mints the `Record-Route` pair; bob answers and
alice learns the route set; the `BYE` then goes to **edge B**, which has never seen the dialog, holds
an empty target map and no registrar, verifies the pair and forwards on the message alone.
`viz::invariants` reads `cross_node_dialog_lookups = Some(0)` and edge B's own lookup list is empty,
while edge A's has exactly the one T2 address-of-record lookup a dialog-forming request should make —
so the zero is the mid-dialog path's property, not an inert scenario's.

**Verification now sits inside the engine, which is what the roadmap said it had to.** `PX-13` made
`Input::Upstream` return `on_targets(…)` synchronously for in-dialog requests, so a driver that fed
`TokenFact(Invalid)` afterwards got its `403` **after** `Effect::Forward`, and one that fed it first
hit `on_token`'s `request is None` path and terminated with no response. The fix is a new
`Effect::VerifyToken { token, partner }`: route preprocessing reports what the popped platform
`Route`s carried, the engine asks and **stops**, and §5.1 does not run until `Input::TokenFact`
arrives. The engine still holds no key and no clock — the verdict is an input, as rule 2 requires.
An unsolicited `TokenFact` is now ignored rather than acted on.

**The `Record-Route` pair is minted, TERM topmost** (affinity-token §7 M1/M2), and preprocessing pops
**both** consecutive platform `Route`s so §8 S9's pair check has its partner. The first-popped token
governs. A node with no key set feeds no `Input::TokensMinted` and gets the single tokenless entry
the platform has always emitted — the pair is a property of the token, not of Record-Routing.

**The `ACK` for a 2xx takes P2 too.** `route_ack` gained a verdict parameter, so a tokened `ACK`
cannot reach `AckRoute::Forward` without one; the refusal is `AckRefusal::UnverifiableToken` — a
record, never a response (§7.2 K3). The harness trace shows edge A verifying the `ACK`'s token before
forwarding it.

**The budget has one owner.** `TOKEN_PARAM` and `TOKEN_PARAM_BUDGET` moved to `sipx-clstr-affinity`,
where §5 owns the parameter name, the encoding and the budget together, and the proxy re-exports them
so its public surface is unchanged. A `const` assertion now compares the budget against
`MAX_TOKEN_LEN` at build time, so widening the layout fails the build instead of the wire. `AT-6`
pins `TOKEN_PARAM_BUDGET == 200` — the row states a number, and `check-vectors.py` is right to want it
compared.

**Deliberately not done, with reasons:**

- **The Path variant (Acceptance 2) — blocked on the spec, not the kernel.** The typed `Path` header
  *is* available (`sipx-sip/src/name.rs:109` at `v0.10.0`; the ledger row reads `landed in 0.4.0`),
  so this story's `note` was right about the dependency and wrong about the consequence.
  affinity-token §1 lists "Path minting semantics (M3, see §7 M7)" as **out of scope**, and M7 says
  the direction value for Path tokens "is fixed by the M3 stories". Minting one now means choosing a
  normative value the spec declines to fix, and Path is minted by the registrar on REGISTER
  (RFC 3327), not by F4. Left for M3.
- **§8's missing-token rejection.** §8 also makes a *tokenless* platform `Route` on a mid-dialog
  request a `403`, on §5's premise that "there is no tokenless platform Route on a mid-dialog
  request". That premise holds only once every edge mints, and no loader for
  `cluster-membership` §4's `keys[]` exists yet — so a keyless node's own `Record-Route` is tokenless
  and the rule would answer `403` to every in-call message on it. An absent `aft` is therefore
  reported as "nothing to verify". This belongs to whichever story makes a key set required.
- **`PB-A-1`/`-2`/`-3` stay deferred.** They name this story in `vector-scope.toml`, which is fenced.
  `PB-A-3` (a 2xx `ACK` at a foreign edge, routed by token) is now within reach — the mechanism is
  built and the `ACK` path verifies — and wants re-filing against a live owner along with `PB-A-1`
  (stickiness-miss duplicate-fork counter) and `PB-A-2`, both of which are transaction affinity
  rather than token round-tripping.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md). Blocked by AF-4, PX-5.
- Failing-first proof: `a_carried_token_is_verified_before_the_in_dialog_request_is_forwarded` in
  `crates/sipx-clstr-proxy/tests/vectors_proxy.rs`. At the merge base it emits `Effect::Forward` from
  `Input::Upstream`, which is the roadmap's recorded defect executed rather than described.
