---
id: PX-14
title: A terminal result must not revive a queued lower-q fork group
pillar: Signalling
status: ready
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

- [ ] A 2xx, a 6xx, or an upstream `CANCEL` clears the queued target groups as well as cancelling the
      launched branches.
- [ ] Late 2xx from an already-launched branch is still forwarded per RFC 6026; terminality prevents
      only never-launched targets from being started.
- [ ] When a cancelling branch later settles, the final-response path does not call
      `fork_next_group` for a context that has already concluded.
- [ ] **Failing-first vector, the cross-product the existing rows miss:** targets A and B at `q=1.0`
      and C at `q=0.5`; A answers `200`, then B answers `487`. Expected: no new request. On `86e6b10`
      A's `200` emits `[Respond, CancelBranch]` and B's `487` then emits `[Forward, SetTimer]` for C —
      a new INVITE after the call was already accepted.
- [ ] The same is asserted for the 6xx and upstream-cancellation forms, not only the 2xx one.
- [ ] The new rows are registered in `scripts/check-vectors.py` in the same commit that writes them.
- [ ] A one-branch leading q-group followed by a lower q-group terminates after 2xx rather than
      waiting for an event that can never legitimately advance the queue.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

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
