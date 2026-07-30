---
id: PX-12
title: Perform the CANCEL and timer effects the driver discards
pillar: Signalling
status: ready
priority: 1
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, node]
note: release blocker — PX-6 proved the effects are produced; nothing performs them, and a Timer C is armed and never fires
---

# Perform the CANCEL and timer effects the driver discards

## Goal
Wire the proxy engine's cancellation and timer effects to the real driver, so the behaviour `PX-6`
and `PX-10` proved in the engine also happens on a socket.

## Acceptance
- [ ] `ProxyEffect::CancelBranch` sends the CANCEL rather than logging it.
- [ ] `ProxyEffect::AnswerCancel` answers the CANCEL's own transaction, first, per §16.10.
- [ ] `SetTimer`/`ClearTimer` reach a real clock, so an armed Timer C fires and its first act is to
      cancel the branch (§9 `C5`).
- [ ] `ProxyEffect::Terminate` terminates the context instead of being dropped.
- [ ] An upstream `CANCEL` is associated with the existing server transaction rather than creating an
      independent proxy context.
- [ ] **Failing-first test over real sockets:** a forked INVITE with one ringing branch is CANCELled
      upstream and the ringing branch receives a CANCEL; separately, a branch that goes silent after a
      provisional is cancelled when Timer C expires. Both fail on `86e6b10`.
- [ ] The `PB-C-*` rows keep their proofs and gain driver-level coverage, or the split between
      "engine emits" and "driver performs" is made explicit in the conformance report.

## Progress
- (not started)

## Notes
- Found by the independent adversarial review of `86e6b10` (`v0.12.0`), finding **V-02**; all three
  independent reviewers converged on it.
- Evidence: `crates/sipx-clstr-node/src/driver.rs:1213-1231` matches
  `AnswerCancel | SetTimer { .. } | ClearTimer { .. } | Terminate => {}` — a literal discard — and its
  own comment states that on this driver "a Timer C is armed with the right value and never fires".
  `CancelBranch` immediately above logs `"branch cancellation is PX-6's, not yet wired to a socket"`.
  Effects are produced correctly at `crates/sipx-clstr-proxy/src/context.rs:379-439`.
- **`PX-6` is not a dishonest close.** Its Acceptance is engine-scoped and its vectors prove effect
  *production*; the deterministic harness does perform these effects, which is why `PB-C-5`/`PB-C-6`
  pass. What no story owned was the driver half — and the README and site advertised the engine's
  capability as a shipped one until `0.12.0` narrowed them.
- Considered for upstream: **partly.** CANCEL-to-transaction association is protocol-generic and must
  be checked against `sipx-transport` first ([upstream ledger](../upstream.md)); scheduling the
  proxy-TU Timer C and performing effects the engine already emits is local driver work.
