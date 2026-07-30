---
id: PX-12
title: Perform the CANCEL and timer effects the driver discards
pillar: Signalling
status: backlog
priority: 1
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, node]
note: release blocker · blocked by CX-7's released CANCEL API; local timer/effect wiring must not construct CANCEL by shadowing the kernel
---

# Perform the CANCEL and timer effects the driver discards

## Goal

Wire the proxy engine's cancellation and timer effects to the real driver, so the behavior `PX-6`
and `PX-10` proved in the engine happens on real sockets without duplicating protocol-generic CANCEL
construction below the kernel boundary.

## Acceptance

- [ ] **Dependency:** sipx releases the CANCEL construction/dispatch API filed by `CX-7`, and this
      workspace pins that release before `CancelBranch` is implemented. No local request-cloning
      helper shadows the kernel's RFC 3261 §9.1 rules.
- [ ] The driver associates an upstream CANCEL with the existing INVITE server transaction and feeds
      `ProxyInput::UpstreamCancelled` to that context; it does not create an independent context.
- [ ] `ProxyEffect::CancelBranch` sends a kernel-built CANCEL on the branch's actual request,
      transaction identity and selected target rather than logging it.
- [ ] `ProxyEffect::AnswerCancel` answers the CANCEL's own transaction, first, per §16.10.
- [ ] `SetTimer` and `ClearTimer` own a race-safe timer per `(ProxyTimer, BranchId)`; a live Timer C
      feeds `TimerFired`, a cleared or superseded timer cannot fire, and no clock enters the pure
      engine.
- [ ] `ProxyEffect::Terminate` stops the context task and releases every timer/branch resource.
- [ ] **Failing-first test over real sockets:** a forked INVITE with one ringing branch is CANCELled
      upstream and the ringing branch receives a CANCEL; separately, a branch that goes silent after a
      provisional is cancelled when Timer C expires. The test asserts the CANCEL's own immediate 200,
      the INVITE's eventual 487 when no 2xx wins, and no transaction/timer leak. Both paths fail on
      `86e6b10`.
- [ ] The `PB-C-*` rows keep their proofs and gain driver-level coverage, or the split between
      "engine emits" and "driver performs" is made explicit in the conformance report.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-02**](../reviews/00-validated-synthesis.md#v-02--the-real-driver-does-not-implement-matched-cancel-or-timer-c); all three independent reviewers
  converged on it.
- Evidence: `crates/sipx-clstr-node/src/driver.rs:1213-1231` matches
  `AnswerCancel | SetTimer { .. } | ClearTimer { .. } | Terminate => {}` — a literal discard — and its
  own comment states that on this driver "a Timer C is armed with the right value and never fires".
  `CancelBranch` immediately above logs `"branch cancellation is PX-6's, not yet wired to a socket"`.
  Effects are produced correctly at `crates/sipx-clstr-proxy/src/context.rs:379-439`.
- **`PX-6` is not a dishonest close.** Its Acceptance is engine-scoped and its vectors prove effect
  *production*; the deterministic harness does perform these effects, which is why `PB-C-5`/`PB-C-6`
  pass. What no story owned was the driver half — and the README and site advertised the engine's
  capability as a shipped one until `0.12.0` narrowed them.
- **Upstream boundary:** CANCEL construction and transaction dispatch are protocol-generic and owned
  by sipx/`CX-7`; scheduling proxy-TU Timer C and performing the engine's existing effects are local
  driver orchestration.
- **Inherited from `PX-14`, and it becomes live here.** `proxy-behavior` §9 `C7` says "a target that
  was never tried is never tried", which is broader than the code: `on_targets` assigns
  `self.queued = targets` unconditionally (`crates/sipx-clstr-proxy/src/context.rs:209`), so an
  `UpstreamCancelled` arriving *before* `TargetsResolved` still forks the whole set. It is unreachable
  today only because `Input::UpstreamCancelled` has no producer outside tests and the driver resolves
  targets synchronously — **and wiring a real upstream CANCEL is exactly this story's job**. Either
  make the code obey C7 for that ordering, or narrow C7 and say why.
