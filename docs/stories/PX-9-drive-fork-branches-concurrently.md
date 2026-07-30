---
id: PX-9
title: Drive fork branches concurrently instead of draining them in order
pillar: Signalling
status: in-progress
priority: 1
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, driver]
note: a user with one dead device waits for Timer B before their live device's 200 OK is relayed
---

# Drive fork branches concurrently instead of draining them in order

## Goal

Make parallel forking actually parallel at the driver seam. The engine forks correctly and emits one
`Forward` per target, and then the driver drains each branch's response stream to completion before
looking at the next — so the slowest branch delays every branch behind it.

## Acceptance

- [x] The driver polls all branches of a fork group concurrently — a `select` over their streams, or
      a `JoinSet` — rather than `while let Some(...) = pending.pop()` with an inner loop that runs the
      stream to exhaustion.
- [x] **Failing-first**: a harness scenario registers two contacts at equal `q`, makes one a black
      hole, and requires the live branch's `200 OK` to be relayed in well under the kernel's Timer B.
      It fails today, and it fails by roughly the full timeout.
- [x] The comment at the drain loop is corrected. It currently asserts that "with M1's single fork
      group, awaiting them in order is the same thing" — which is false for any group with more than
      one target, and the ordinary two-device registration is exactly that: two contacts, both
      `q=1000`, placed in one group by `lookup.rs`.
- [x] RFC 3261 §16.7's response-selection behaviour is unchanged. This is a story about *when* branch
      events are read, not about which response wins; the existing `PX` vector rows for forking,
      `CANCEL` and Timer C must pass untouched.
- [x] Branch ordering does not become a source of nondeterminism in the harness. The simulator owns
      its scheduling, so concurrent polling must still replay byte-identically from a seed — if a
      `select` introduces an ordering choice, that choice is the harness's to make, not the OS's.

## Progress

- **Done.** `proxy_request` now reads every branch through a `tokio::task::JoinSet<BranchEvent>`:
  each branch's `Responses` is moved into a task for exactly one `next()` and handed back with the
  event, so N branches are awaited at once and the branch is re-armed as soon as its event is taken.
  The engine still receives **one input at a time in a total order** — read concurrently, reduced
  serially — which is the shape
  [proxy-transaction-driver](../designs/proxy-transaction-driver.md) already specified
  ("the task holds them in a `FuturesUnordered` … turned into one `Input` at a time"). A `JoinSet`
  rather than a `FuturesUnordered` because `futures` is not a dependency and adding one to a fenced
  manifest was not this story's to do; the acceptance names both.
- `Responses` had to travel with the event because it is not a `Stream` and exposes only
  `async fn next(&mut self)`. Re-creating that future per poll would deregister the receiver's waker
  and lose notifications, so the stream is moved rather than borrowed.
- **The failing-first test is a real-socket node test, not a harness scenario**
  (`crates/sipx-clstr-node/tests/fork_branches.rs`). `sipx-clstr-sim` models a node sans-IO and has
  no dependency on `sipx-clstr-node` at all, so it never runs `driver.rs` and the defect is not
  reachable from it. Same reasoning as `tests/admission_bound.rs`, and it is why the last acceptance
  item holds vacuously: nothing the harness replays was touched.
- Measured: black-hole-registered-first went from **32.003 s** to **0.7–1.5 ms** for the live
  device's `200 OK`, against a 4 s budget and a 32 s Timer B. Both registration orders are pinned,
  because which branch the old `pop()` drained first was decided by §7 L3's `refreshed_at` tie-break
  and only one of the two orders failed.
- **One residual, stated rather than buried.** `select_and_answer` forwards the *first* final in
  `finals` whose code equals the best code, and `aggregate_challenges` concatenates 401/407
  challenges in `finals` order. `finals` order was the drain order and is now the arrival order.
  The **selected status code is unaffected** — `best_final` is a `min` over the multiset, which is
  order-independent — and all 46 `PX` vector rows plus the sim's `proxy_cancel` and `proxy_torture`
  scenarios pass untouched. What can differ is which of two *equal-status* branch responses is
  copied, and the order of challenges inside an aggregate. Preserving that order is preserving
  head-of-line blocking, so the two cannot both be had; the design's `FuturesUnordered` shape
  sanctions arrival order, and §16.7 prescribes nothing among equals.
- Still open on the same loop, as the note below says: `SetTimer`/`ClearTimer`/`Terminate` are
  swallowed and `CancelBranch` is only logged, so Timer C never fires and a losing branch is never
  CANCELled. Concurrency makes a live 2xx fast; it does not make a live *non-2xx* fast, because the
  context still waits for the dead branch's Timer B before selecting. That wants its own story.
- **Considered for upstream: no.** Unchanged from the note below, and re-examined against the
  implementation: the kernel's primitive — one `Responses` per branch — is right, and what was
  missing was purely the driver's composition of several. The one thing that would have made this
  smaller is a `Responses` that implemented `Stream` (or exposed `poll_recv`), which would let a
  driver hold N of them in one `select` without a task per event. That is a small, protocol-generic
  kernel ergonomics gap rather than a defect, and it is worth raising in
  [docs/upstream.md](../upstream.md) — not edited here, because the ledger is not this story's file.

## Notes

- **The symptom a user sees.** Two registered devices, one of them unreachable over UDP. If the dead
  branch is popped first its stream produces nothing until the kernel's Timer B fires, and the
  `200 OK` already sitting in the live branch's stream is not read until then — roughly thirty
  seconds of silence before the answer is relayed.
- **This is a driver defect, not an engine defect.** `context.rs` groups and forks correctly per §16;
  the loss is entirely in how `driver.rs` consumes the effects. Keep the fix on that side of the
  sans-IO boundary — the engine must not learn about concurrency.
- Related and deliberately not folded in: the driver also swallows `SetTimer`/`ClearTimer`/`Terminate`
  and only logs `CancelBranch`, so Timer C never fires and a losing branch is never CANCELled — in-flight
  state is reaped only by the kernel's 180-second unanswered backstop. That is a second driver-seam gap
  on the same loop and wants its own story; note it here so whoever picks this up sees both.
- Considered for upstream: no. The kernel gives one response stream per branch and that is the right
  primitive; composing several is the driver's job.
