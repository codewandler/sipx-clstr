---
id: AF-5
title: Round-trip tokens through Record-Route and Route
pillar: Cluster
status: in-progress
priority: 1
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, proxy]
note: blocked by AF-4 only — PX-5 is done and T-14's typed Path landed in v0.4.0
---

# Round-trip tokens through Record-Route and Route

## Goal
Carry the token through the proxy: minted into Record-Route on dialog-forming requests, verified from Route on mid-dialog requests — at any edge, with zero lookups.

## Acceptance
- [ ] A dialog-forming request through edge A yields a Record-Route token; the mid-dialog request arriving at edge B routes on it with the cross-node lookup counter at zero (harness scenario).
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

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md). Blocked by AF-4, PX-5.
