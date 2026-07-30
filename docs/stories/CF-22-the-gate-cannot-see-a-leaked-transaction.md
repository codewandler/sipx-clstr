---
id: CF-22
title: The gate cannot see a leaked transaction
pillar: Foundation
status: in-progress
priority: 1
epic: conformance-harness
areas: [gate, node]
note: re-specified — the first Acceptance demanded a bound correct code cannot meet; PX-13 drains at 128·T1 and is not a leak
---

# The gate cannot see a leaked transaction

## Goal
Make an unbounded transaction leak fail the gate, rather than only the one CI job that happens to
wait for a drain.

## Acceptance

- [x] A gate step asserts the node's transaction accounting returns to **zero** after a completed
      call, within the worst case RFC 3261 actually permits — `128·T1` plus slack for a proxied
      non-INVITE to a silent next hop, **not** `64·T1`.
- [x] It runs without Docker, PostgreSQL or the external `sipx` CLI. The deterministic harness is the
      host; `crates/sipx-clstr-sim/tests/transaction_drain.rs` already does this and should be kept.
- [x] The assertion is on the count **reaching zero**, not on it decreasing: the incident that
      prompted this drained 16 → 10 → 5 → 3 and then stalled within one window.
- [x] **`PX-13` is GREEN under the corrected bound**, and a test proves the distinction the story is
      actually about: a transaction that never terminates fails, one that terminates at `128·T1` does
      not. Injecting a genuinely unbounded hold is the failing-first proof — *not* `impl/PX-13`.
- [x] `scripts/e2e-call.sh`'s drain loop waits past `128·T1`, and its comment stops describing the
      window as `64·T1`. Its message stops calling the result "a leaked transaction" unless it is one.
- [x] The check names what is still held — count, breakdown, elapsed window and the RFC window — so a
      red says which resource rather than that something is wrong.

## Progress

- **Re-specified 2026-07-30 after the first attempt was parked. The original Acceptance was mine and
  it was contradictory**, which is why no rework by the implementor could satisfy it: it demanded
  "red with `impl/PX-13`, green without" while the Goal asked for an **unbounded** leak to fail. Every
  correct formulation makes `PX-13` green, so the only way to keep it red was to assert a bound
  correct code cannot honour.
- **`PX-13` does not leak. It drains at exactly `64.000 s` = `128·T1`, measured.** Two absorption
  windows, and both are RFC-mandated for a proxied non-INVITE to a silent next hop:
  - §17.1.2.2 — Timer F = `64·T1` on the non-INVITE **client** transaction. §16.8 confines Timer C to
    INVITE, so no proxy-level timer can conclude the branch sooner.
  - §16.7 — the proxy MUST forward the best final response, and there is none until the branch
    concludes. Until then the **server** transaction sits in Trying/Proceeding, which §17.2.2 gives
    **no timer at all**.
  - §17.2.2 — Timer J = `64·T1` starts only *when the final response is sent*, i.e. after the first
    window has already elapsed.

  The pinned kernel implements exactly that (`timing.rs:69` `timeout() = t1 * 64`; `:100`
  `timer_j(Unreliable) = timeout()`), and the harness trace shows Timer E retransmitting the BYE
  capped at T2 (0.5, 1.5, 3.5, 7.5 … 31.5 s), Timer F at `32.000`, `respond 500`, then Timer J at
  `64.000`.
- **The first check was green on `main` for the wrong reason.** The two traces are byte-identical
  until the BYE reaches the edge, where `main` looks up 0 targets and answers `480` itself while
  `PX-13` forwards to the remote target. `main` passes because it answers a dialog's BYE instead of
  delivering it — which *is* `V-03`, the defect `PX-13` fixes. **The check rewarded the bug and
  punished the fix**, and merged it would have blocked `PX-13` permanently.
- **The real defect is `scripts/e2e-call.sh:284`** — `seq 1 100` × `sleep 0.5` = 50 s, strictly
  between `32 s` and `64 s`, with a comment budgeting for one window. `PX-13` was reverted on a
  threshold inside RFC 3261's own worst case. That revert was the coordinator's call and it was
  wrong; correcting the window is part of this story.
- **Option (a) was considered and rejected on evidence**, not preference: "a proxy should not hold a
  server transaction while it discovers a dead peer" has no normative footing — §17.2.2 gives that
  state no timer, and the kernel's own backstop documents itself as *"a backstop against never, not a
  deadline"*. Cutting a non-INVITE branch short of Timer F would drop the BYE of a slow-but-live peer.
- **The artifact is sound and must be kept, not rewritten.** Fence clean, three files, `fmt` and
  `clippy` green, the deliberate harness infidelity verified symmetric across both trees (identical
  blob hash, traces identical through event 48), and the 4 s slack correct for exact virtual time.
  Only the assertion needs re-aiming.
- **Carry these three minors while re-aiming:** `Forward { .. }` hides `next_hop`, so the harness
  resolves `target.uri` — harmless today because bob's only Route is the edge's own Record-Route which
  `preprocess` P2 pops, but it is precisely the mistake `next_hop` exists to prevent and a future
  scenario with a surviving Route would route wrong silently. The harness `outstanding` omits a
  `handed_over` analogue, so it reproduces 2 of CI's 3, and the real driver keeps its context in a
  per-request task rather than a map — so Acceptance item 1's "the node's outstanding-transaction
  count" oversells slightly and should be worded to what is actually observed. And the port-explicit
  warning on `BOB_CONTACT` names the wrong mechanism: the canonical AoR includes the host and
  `10.0.0.9` ≠ `atlanta.example`, so the host already makes the keys distinct.

### Round 2 — re-aimed, and what it now proves

- **The bound is `128·T1`, derived in one place and measured in another.**
  `crates/sipx-clstr-sim/tests/transaction_drain.rs` writes every duration in terms of `T1`, so
  `ABSORPTION = 64·T1` and `BOUND = 2 × ABSORPTION` are the derivation rather than two constants that
  have to be kept in step.
- **`the_worst_case_the_rfc_permits_is_inside_the_bound` measures it rather than restating it.** Bob
  registers and then goes silent; the branch produces nothing, so Timer B runs to `64·T1` before
  §16.7 has a final response to forward, and only then does Timer H start. Measured drain: **exactly
  `64.000 s` = `128·T1`**, against `32.000 s` for a call that completes. Both probes are at
  **absolute** virtual instants (`127·T1`, then `128·T1 + slack`) rather than offsets from `BOUND` —
  which is what makes the test able to catch `BOUND` being wrong. Verified: halving `BOUND` to one
  window turns that test red with *"the node still holds 2 — 0 client, 1 server, 1 per-transaction
  entries"*. A one-window bound rejects correct code, exactly as it rejected `PX-13`.
- **`PX-13` is green.** All four tests pass on a tree carrying `234ab7b` (`git revert a02cd7c` on
  top of `main`). No scratch branch is left behind; `scratch/cf-22-px13` is deleted.
- **The failing-first proof is an injected hold, not a story.** `Hold::NeverAnswer(Method::Bye)`
  makes the node take the hang-up and never answer it: §17.2.2 gives a server transaction in Trying
  no timer at all, so nothing in the RFC will ever collect it. `a_hold_no_timer_collects_is_caught`
  runs the *same* call as the headline test, checks the hold is still there at the bound, and then
  checks it is **still exactly the same size** twenty windows later — which is what separates
  unbounded from slow.
- **`scripts/e2e-call.sh`** now budgets `100 s` (`drain_budget_s`), its comment carries the two-window
  arithmetic and names the `50 s` mistake that caused the revert, and its failure message says
  "something is held that no §17 timer will collect" instead of "a leaked transaction".
- **The three minors from the review are carried.** The `Forward { .. }` shortcut is now guarded: the
  harness asserts a forwarded request carries no surviving `Route`, so the day a scenario grows one,
  resolving `target.uri` instead of `next_hop` fails loudly instead of routing wrong in silence.
  `Outstanding`'s doc says what it counts and what it cannot — no `handed_over` analogue, because the
  real driver holds a proxied request's context in a per-request task (`driver.rs:1153`) rather than
  a map, so this reports two entries where `Handle::outstanding()` reports three; the difference is
  in the number, never in whether it is zero. And `BOB_CONTACT`'s comment now names the host as the
  mechanism that keeps it off the AoR key, with the explicit port as belt to that brace.

## Notes
- **Found the expensive way.** `PX-13` passed `scripts/gate.sh` in the implementor's worktree, passed
  it again on the integration branch, passed an independent review that re-ran its failing-first proof
  at the true merge base, and passed the local two-node call proof I ran by hand. It was merged and
  pushed. CI's `e2e` job then failed on the drain check, and the merge was reverted.
- The failing assertion, from `scripts/e2e-call.sh`:
  `the node still reports outstanding=3 after 50s — a leaked transaction`. Every call assertion before
  it passed — audio flowed, media went direct.
- **Why nothing local saw it.** No gate step starts a node, completes a call, and watches
  `outstanding` drain. `e2e-call.sh` does, and `CF-15` deliberately made it a **separate CI job**
  rather than a gate step, so that a red says "the end-to-end call broke" rather than "the gate is
  red" and so `gate.sh` stays runnable without a second checkout. That was a good decision and it
  leaves this hole: the one check that watches resource lifetime is the one contributors do not run.
- The deterministic harness (`sipx-clstr-sim`) already runs call flows in virtual time with the real
  engine, so it can observe the same counter without any of `e2e-call.sh`'s external dependencies.
  That makes this cheap, which is the argument for doing it rather than relying on CI.
- Related: `DP-11` reads `Handle::shed()` and `outstanding()` for its admission bound, so the
  instrument already exists and is already consumed — only the assertion is missing.
- Considered for upstream: **no.** The counter is the kernel's and is already exported; asserting that
  this platform's driver returns it to zero is orchestration.
