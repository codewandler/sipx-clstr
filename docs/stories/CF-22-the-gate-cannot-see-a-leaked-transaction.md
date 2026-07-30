---
id: CF-22
title: The gate cannot see a leaked transaction
pillar: Foundation
status: in-progress
priority: 1
epic: conformance-harness
areas: [gate, node]
note: PX-13 passed the full gate and the local two-node proof, then leaked three transactions per call in CI
---

# The gate cannot see a leaked transaction

## Goal
Make an unbounded transaction leak fail the gate, rather than only the one CI job that happens to
wait for a drain.

## Acceptance
- [x] A gate step asserts that the node's outstanding-transaction count returns to zero after a
      completed call, within RFC 3261's 64·T1 absorption window.
- [x] It runs without Docker, PostgreSQL or the external `sipx` CLI — otherwise it lands in the same
      not-in-the-gate category as `e2e-call.sh` and catches nothing locally. The deterministic harness
      already drives full call flows in virtual time and is the obvious host.
- [x] The assertion is on the **count reaching zero**, not on it merely decreasing: the observed
      failure drained 16 → 10 → 5 → 3 and then stopped, so a "went down" check passes it.
- [x] **Failing-first:** with `impl/PX-13` (`234ab7b`) merged, the new check is red. With it reverted,
      green. That branch is preserved precisely so this can be demonstrated rather than argued.
- [x] The check names what leaked — at minimum the count and the elapsed window — so a red says which
      resource, not merely that something is wrong.

## Progress
- `crates/sipx-clstr-sim/tests/transaction_drain.rs` — one node running the kernel's own
  `sipx_sip::transaction::TransactionLayer` under the real forwarding engine and the real registrar.
  Two devices register, one calls the other, the callee hangs up, and one absorption window later the
  node must hold **nothing**. `outstanding` is counted the way `sipx-transport`'s endpoint counts it
  for `Handle::outstanding()`: transactions **plus** the driver's per-transaction map, because an
  entry that outlives its transaction is the leak a count of transactions alone cannot see.
- `scripts/gate.sh` gains a **`transaction drain`** step, ahead of `tests` (which also contains it).
  The duplication is the price of the step naming itself: this failure is a resource lifetime rather
  than a wrong answer, and a red has to say which resource.
- A second test, `the_store_is_deliberately_not_empty_before_the_window`, holds the first one honest:
  a node that tore transactions down the instant it answered would satisfy "empty at 36 s" while
  breaking §17, so the store is asserted **non-empty** one second after the call.
- **What actually failed in CI, established rather than assumed.** `scripts/e2e-call.sh` was
  reproduced locally against the `PX-13` tree (the kernel CLI built from the pinned `v0.10.0` tag) and
  the endpoint's counters were instrumented. The three that stalled were **one BYE server transaction
  in `Completed`, plus its `destinations` and `handed_over` entries**. The chain: `sipx dial` exits
  when its `--duration` elapses, so the callee's `BYE` arrives after the caller's device is gone;
  before `PX-13` the node answered it `480` itself (a dialog's remote target is not an address of
  record, so the lookup found nothing) and Timer J started at once, but after `PX-13` the `BYE` is
  correctly forwarded to that remote target, the client transaction burns Timer F for 64·T1, and only
  then does the upstream server transaction enter `Completed` and start its own 64·T1. Two windows,
  not one — past `e2e-call.sh`'s fifty seconds. The scenario here is that shape, in virtual time.

## Notes on the check
- The harness's driver forwards an ACK for a 2xx to the dialog's remote target rather than through
  the location service. `main`'s driver does the latter and drops it (`V-03`, which is `PX-13`'s
  subject, not this story's) — and modelling that defect here would make the two trees run *different
  calls*, so their drain times could not be compared. That is the one deliberate deviation.
- A kernel `TuEvent::Timeout` is fed to the engine as §16.9's branch failure rather than as the
  driver's `408`. What differs is the status the caller reads; what this scenario measures is how
  long the transaction lives.

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
