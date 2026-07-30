---
id: PX-14
title: A terminal result must not revive a queued lower-q fork group
pillar: Signalling
status: in-progress
priority: 1
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: release blocker — after a 200 is forwarded, a later 487 starts a new INVITE to a never-launched target
---

# A terminal result must not revive a queued lower-q fork group

## Goal
Make acceptance, global rejection and upstream cancellation terminal for the whole target set, not
only for the branches that had already been launched.

## Acceptance
- [x] A 2xx, a 6xx, or an upstream `CANCEL` clears the queued target groups as well as cancelling the
      launched branches.
- [x] When a cancelling branch later settles, the final-response path does not call
      `fork_next_group` for a context that has already concluded.
- [x] **Failing-first vector, the cross-product the existing rows miss:** targets A and B at `q=1.0`
      and C at `q=0.5`; A answers `200`, then B answers `487`. Expected: no new request. On `86e6b10`
      A's `200` emits `[Respond, CancelBranch]` and B's `487` then emits `[Forward, SetTimer]` for C —
      a new INVITE after the call was already accepted.
- [x] The same is asserted for the 6xx and upstream-cancellation forms, not only the 2xx one.
- [x] The new rows are registered in `scripts/check-vectors.py` in the same commit that writes them.

## Progress
- **Done, gate green.** Spec first: §7.1 names the queue the rules needed to refer to (it was
  implemented and never specified, which is why the defect violated no written rule), §8 gains **R12**
  and §9 gains **C7**.
- The fix is `conclude_target_set()` in `crates/sipx-clstr-proxy/src/context.rs`, called from the R5,
  R6 and upstream-CANCEL paths. `may_fork_next_group()` replaces the three bare `!queued.is_empty()`
  tests and additionally refuses to fork an `answered`/`finished` context, so the invariant is stated
  rather than only implied by the queue being empty. `finish_if_settled` reads the same predicate,
  which also fixes a context that could never terminate: a 6xx with a queue behind it previously left
  `finish_if_settled` permanently blocked.
- Vectors `PB-T-1`/`-2`/`-3` in `crates/sipx-clstr-proxy/tests/vectors_proxy.rs`; the `PB-T` family is
  registered in `scripts/check-vectors.py` in the same commit, and `docs/reference/conformance.md` is
  regenerated (552 rows, 128 proved).
- Verified failing-first at merge base `2cb22dd`: all three emitted `Forward` + `SetTimer` for the
  `q=0.5` target after the call was concluded.

## Notes
- Found by the independent adversarial review of `86e6b10` (`v0.12.0`), finding **V-04**, reproduced
  by the protocol reviewer as an isolated state-machine execution and re-traced by the coordinator.
- Evidence: queue retention and group draining at `crates/sipx-clstr-proxy/src/context.rs:193-242`;
  terminal response handling at `:342-374`; upstream cancellation at `:379-411`; finish logic at
  `:631-647`.
- **Why the existing vectors miss it:** sequential grouping and terminal results are each covered, but
  never composed. `PX-9` made branches concurrent and `PX-11` settled which 4xx wins; neither asks
  what a *late* final response does to a queue that should already be dead.
- Considered for upstream: **no**, with a check. The forking state machine is this repository's
  (`proxy-behavior` §7); the kernel owns transaction policy, not proxy target selection. If the fix
  turns out to need transaction behaviour the kernel does not expose, file that half in the
  [upstream ledger](../upstream.md) rather than shadow-implementing it here.
