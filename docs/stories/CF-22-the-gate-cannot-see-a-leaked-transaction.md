---
id: CF-22
title: The gate cannot see a leaked transaction
pillar: Foundation
status: ready
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
- [ ] A gate step asserts that the node's outstanding-transaction count returns to zero after a
      completed call, within RFC 3261's 64·T1 absorption window.
- [ ] It runs without Docker, PostgreSQL or the external `sipx` CLI — otherwise it lands in the same
      not-in-the-gate category as `e2e-call.sh` and catches nothing locally. The deterministic harness
      already drives full call flows in virtual time and is the obvious host.
- [ ] The assertion is on the **count reaching zero**, not on it merely decreasing: the observed
      failure drained 16 → 10 → 5 → 3 and then stopped, so a "went down" check passes it.
- [ ] **Failing-first:** with `impl/PX-13` (`234ab7b`) merged, the new check is red. With it reverted,
      green. That branch is preserved precisely so this can be demonstrated rather than argued.
- [ ] The check names what leaked — at minimum the count and the elapsed window — so a red says which
      resource, not merely that something is wrong.

## Progress
- (not started)

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
