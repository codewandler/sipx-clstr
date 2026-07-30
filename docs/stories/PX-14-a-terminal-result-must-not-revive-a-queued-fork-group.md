---
id: PX-14
title: A terminal result must not revive a queued lower-q fork group
pillar: Signalling
status: done
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
- [x] Late 2xx from an already-launched branch is still forwarded per RFC 6026; terminality prevents
      only never-launched targets from being started.
- [x] When a cancelling branch later settles, the final-response path does not call
      `fork_next_group` for a context that has already concluded.
- [x] **Failing-first vector, the cross-product the existing rows miss:** targets A and B at `q=1.0`
      and C at `q=0.5`; A answers `200`, then B answers `487`. Expected: no new request. On `86e6b10`
      A's `200` emits `[Respond, CancelBranch]` and B's `487` then emits `[Forward, SetTimer]` for C —
      a new INVITE after the call was already accepted.
- [x] The same is asserted for the 6xx and upstream-cancellation forms, not only the 2xx one.
- [x] The new rows are registered in `scripts/check-vectors.py` in the same commit that writes them.
- [x] A one-branch leading q-group followed by a lower q-group terminates after 2xx rather than
      waiting for an event that can never legitimately advance the queue.
- [x] `scripts/gate.sh` is green.

## Progress
- **Done, gate green, integrated at `PX-14`.** Spec first: `proxy-behavior` §7.1 names the queue the
  rules needed to refer to — it was implemented and never specified, which is why the defect violated
  no written rule — and §8 gains **R12**, §9 gains **C7**.
- The fix is `conclude_target_set()` in `crates/sipx-clstr-proxy/src/context.rs`, called from the R5,
  R6 and upstream-CANCEL paths. `may_fork_next_group()` replaces the three bare `!queued.is_empty()`
  tests and additionally refuses to fork an `answered`/`finished` context, so the invariant is stated
  rather than only implied by the queue being empty.
- **A second defect, found in passing and declared:** `finish_if_settled` read the same predicate, so
  a 6xx with a queue behind it left the context permanently unfinishable. Independently reproduced at
  review: the base emits `[Respond]` and stays unfinished where the head emits `[Respond, Terminate]`.
- Vectors `PB-T-1`/`-2`/`-3`; the `PB-T` family is registered in `scripts/check-vectors.py` in the
  same commit as the rows, and the registration is load-bearing — deleting it fails the gate with
  "PB-T: 3 rows and no FAMILIES entry".
- **Independent review: `PASS`.** It re-ran the failing-first proof against a pristine base tree with
  its own build cache (all three rows fail there), and separately falsified the two risks worth
  falsifying: RFC 6026 late 2xx is preserved — `conclude_target_set` clears `queued` only and never
  touches `branches`, so A `200` then B `200` still forwards, as does a late 2xx after a 6xx — and
  `pb_v_8`'s Max-Breadth serialization survives, including on the Timer C and transport-error paths
  the test itself does not cover.
- **The one-branch acceptance item, as integrated.** Read literally it is unmet: the `200` emits
  `[Respond]` and `Terminate` arrives on the next input. Read as intended it is met, and the literal
  reading is *impossible* — emitting `Terminate` on the 2xx itself would drop exactly the RFC 6026
  late 2xx the item above it requires. Recorded here rather than silently ticked.
- **Known, not fixed:** C7's "a target that was never tried is never tried" is broader than the code.
  `on_targets` assigns `self.queued = targets` unconditionally, so an `UpstreamCancelled` arriving
  *before* `TargetsResolved` still forks the whole set. Unreachable today — `Input::UpstreamCancelled`
  has no producer outside tests and the driver resolves targets synchronously — and identical at base
  and head, so it is spec text outrunning code by one ordering rather than a regression. It becomes
  live the moment `PX-12` wires a real upstream CANCEL, and that story should close it.

## Notes

- Validated synthesis finding [**V-04**](../reviews/00-validated-synthesis.md#v-04--a-final-response-or-cancellation-can-revive-a-lower-q-fork-group), reproduced by the protocol reviewer as an isolated state-machine execution and re-traced by the coordinator.
- Evidence: queue retention and group draining at `crates/sipx-clstr-proxy/src/context.rs:193-242`;
  terminal response handling at `:342-374`; upstream cancellation at `:379-411`; finish logic at
  `:631-647`.
- **Why the existing vectors miss it:** sequential grouping and terminal results are each covered, but
  never composed. `PX-9` made branches concurrent and `PX-11` settled which 4xx wins; neither asks
  what a *late* final response does to a queue that should already be dead.
- **Upstream boundary:** no; queued target selection is this repository's proxy state machine. If a
  fix exposes a missing generic transaction capability, file that capability upstream rather than
  shadow-implementing it here.
