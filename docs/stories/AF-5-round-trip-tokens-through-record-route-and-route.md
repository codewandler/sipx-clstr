---
id: AF-5
title: Round-trip tokens through Record-Route and Route
pillar: Cluster
status: in-progress
priority: 1
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

- **2026-07-31 — implemented, independently reviewed, `REWORK`, and the rework died on
  infrastructure.** The branch is **`impl/AF-5` at `d47542e`** (22 files, +1777/-87), worktree
  preserved at `/home/timo/projects/sipx-clstr-AF-5`. A rework agent was dispatched with the finding
  below and was killed by an org monthly spend limit before writing anything, so **round 1 of 2 is
  still available**. Nothing is merged. The implementor's own account of the work is in that branch's
  copy of this file and is largely confirmed — read it there.
- **Blocking finding: a token on the *second* popped platform `Route` is consumed and never
  verified.** `route::preprocess` pops up to two platform `Route`s
  (`crates/sipx-clstr-proxy/src/route.rs:60-77`) but decides *whether to verify at all* from the first
  slot only: `let first = popped.first().cloned()?;` (`:76`) runs `set_routes` (`:77`), then
  `let token = first?;` (`:80`) returns `None` when the first-popped entry had no `aft` — **after** a
  second, token-bearing platform `Route` has already been popped. No `Effect::VerifyToken` is emitted
  and the request forwards. Reproduced with probes on a scratch copy, each with its own
  `CARGO_TARGET_DIR`:
  - `BYE`, `Route: <sip:edge-1.example;lr>` then `<sip:edge-1.example;lr;aft=STOLEN>` →
    `kinds = [Forward]`, `forwarded, Route left = None`.
  - `BYE`, `<sip:edge-1.example;lr>`, `<…;aft=ORIG>`, `<…;aft=TERM>` (mixed keyed/keyless cluster) →
    `kinds = [Forward]`, `forwarded with Route = ["<sip:edge-1.example;lr;aft=TERM>"]` — a platform
    `Route` forwarded on to the next hop.

  Breaks `proxy-behavior` §5 P2 (`../specs/proxy-behavior.md:122` — "if it carries an affinity token,
  verify it") and skips `affinity-token` §8 S9 for a request that *does* present the pair — which is
  what this diff's own test says the two-pop exists to prevent
  (`two_consecutive_platform_routes_are_popped_as_a_pair_for_s9`). **Not** covered by the deliberate
  "absent `aft` → nothing to verify" deviation, whose stated rationale (`proxy/src/types.rs:70-90`) is
  a keyless node's own tokenless `Record-Route`; here a token is present, popped, and ignored.
- **Two secondary items, neither a live bug.** (a) The `awaiting_verdict` latch is consulted only in
  `on_token` (`proxy/src/context.rs:177`); `on_targets` and `on_upstream` do not check it, so ordering
  is **not** the state-machine property this story claims — probes show an unsolicited
  `TargetsResolved` while awaiting yielding `[Forward]`, and a second `Upstream` then a late `Invalid`
  reproducing `PX-13`'s shape (a `403` after the forward). Unreachable through any in-tree driver
  (`node/src/driver.rs:1162-1164` builds a fresh context per request and feeds `Upstream` once), so it
  is a **claim gap**: either strengthen the latch or correct the claim. (b) **`CF-22` cannot see a
  wedged context by name** — `Outstanding` (`sim/tests/transaction_drain.rs:170-184`) counts
  client/server transactions and the driver's `peers` map, not the `contexts` map, and no scenario in
  that file produces a token, so the path is untested. A wedge would still trip `held.total()` via the
  uncollected server transaction, but nothing proves it.
- **The absent-token fallback is accepted for now and filed forward.** It grants nothing today: the
  engine discards the verdict's claims (`TokenVerdict::Valid { .. } => self.route_it()`,
  `context.rs:184`) and an in-dialog request's target is the Request-URI, which a Route-less request
  already gets. It becomes a real downgrade the moment any claim — home shard, media node, tenant —
  becomes a routing input. Filed as
  [`AF-8`](AF-8-a-tokenless-mid-dialog-route-must-not-be-a-silent-downgrade.md) so it is not carried
  by a source comment alone.
- **What the review verified as sound — do not re-litigate.** The **failing-first proof is honest**:
  `a_carried_token_is_verified_before_the_in_dialog_request_is_forwarded`, copied verbatim to a fresh
  worktree at merge base `e6fffd6` with its own target dir, compiles at base and fails on its own
  assertion — `panicked at crates/sipx-clstr-proxy/tests/vectors_proxy.rs:1739: a token was carried
  and no verdict has arrived: nothing may be forwarded yet`. No shared build cache exists on this
  machine. **The zero-lookup claim is not inert**: mutating `if route::is_in_dialog(&request)` →
  `if false` makes edge B emit a cross-node dialog lookup, answer `480`, and fails both
  `the_mid_dialog_request_at_edge_b_routes_on_the_token_with_zero_cross_node_lookups` and
  `the_edges_share_a_key_set_and_nothing_else`; edge B genuinely holds an empty target map and no
  registrar, and edge A's single T2 lookup is asserted. **M2's TERM-topmost ordering** is checked
  positionally *and* by decoded direction at three points, so a swap fails all three. **The 200-byte
  budget has one owner** — defined once in `affinity/src/codec.rs`, re-exported by the proxy, with
  `const _: () = assert!(WORST_CASE_PARAM_LEN <= TOKEN_PARAM_BUDGET)` tying it to `MAX_TOKEN_LEN` at
  build time; `AT-6`'s `assert_eq!(TOKEN_PARAM_BUDGET, 200)` pins the row to that owner, satisfying
  `CF-12` without a third copy. Gate green in the implementor's worktree, all steps, 45 test binaries,
  0 failures. Nothing in-tree mentions `aft`, so item 5 breaks no script or manifest.
- **Acceptance item 2 (Path) stays unticked and the reason is spec-backed** —
  `../specs/affinity-token.md:54` lists "Path minting semantics (M3, see §7 M7)" as out of scope, and
  Path is minted by the registrar on REGISTER (RFC 3327), not by F4. It is M3's.
- **Lockfile: mine, taken.** The branch left `Cargo.lock` uncommitted with exactly two intra-workspace
  lines (`+ "sipx-clstr-affinity",` into the `sipx-clstr-proxy` and `sipx-clstr-sim` dependency
  lists) — correct fence behaviour. Saved to the coordinator's scratchpad; regenerate with
  `cargo check` at integration rather than reusing it.
- **`PB-A-1`/`-2`/`-3` want re-filing against a live owner.** `PB-A-3` (a 2xx `ACK` at a foreign edge,
  routed by token) is now within reach; `PB-A-1`/`-2` are transaction affinity, not token
  round-tripping. `vector-scope.toml` is fenced, so this is the integrator's edit — not done, because
  nothing merged.
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
