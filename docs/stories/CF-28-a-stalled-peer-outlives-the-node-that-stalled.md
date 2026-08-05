---
id: CF-28
title: A stalled peer outlives the node it was stalled on
pillar: Foundation
status: done
priority: 3
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: complete — kill dominates StopReading in either order, restart preserves independent link policy, and old held writes are discarded
---

# A stalled peer outlives the node it was stalled on

## Goal

Make the harness's connection faults compose the way their own documentation says they do.
`CF-26` gave the sim reconnect, restart and backpressure, and its review found four places where
one fault silently outlives or over-reaches another — each of which would make an `AF-7`
owner-RPC scenario pass or fail for a reason that is the harness's rather than the code's.

## Acceptance

- [x] **A killed node stops absorbing writes.** `send` checks `not_reading` before reaching the
      link (`sipx-clstr-sim/src/sim.rs:786`), and neither `KillNode` (`:320`) nor `restart`
      (`:396`) clears it — only `resume_reading` (`:425`) does. Measured at CF-26's tip:
      kill alone gives `errors=9 stalled=0 held=0`; stall-then-kill gives
      `errors=0 stalled=10 held_for_dead_node=9`. That contradicts the intent stated three lines
      above the kill's own link cut (`:302-303`: "cutting only outbound would leave it silently
      absorbing traffic"). Failing-first test pins the stall-then-kill case.
- [x] **A restart drains what the old process held.** `Fault::RestartNode`'s doc frames a restart
      as a new process with an empty table (`fault.rs:117-135`) and `StopReading`'s says a restart
      "discard[s] what was held" (`fault.rs:149`) — measured, it discards and then re-accumulates
      (`held_after_restart=5`).
- [x] **A restart heals only what the kill cut.** `restart` sets `partitioned=false` for every
      peer whenever `was_killed` (`sim.rs:389-393`), so a `Partition` scheduled *before* the kill
      is silently healed by the restart, against `fault.rs:126`'s claim that it "puts its links
      back exactly as the kill cut them".
- [x] **The documented precedence has a test.** Stall-beats-link (`fault.rs:146-150`) is real —
      with `StopReading(B)` and `Partition{A|B}` in one window, all four held writes deliver at
      the resume, three written while the link was cut — but nothing pins it, so a future
      reordering of `sim.rs:786` against `dispatch` flips it undetected.
- [x] Every existing pinned vector and byte-for-byte replay still passes **unchanged**; the
      cross-version check `CF-26`'s review introduced (same probe compiled at two revisions,
      rendered traces compared) stays green.

## Progress

- Filed at `CF-26`'s integration from its independent review's four minor findings. None is
  reachable by an `owner-rpc` §10 row today — §10's `owner_rpc_dial_is_suppressed_after_failure`
  puts the owner down without a stalled client — so `AF-7` is not blocked by this.
- Implemented on `impl/CF-28`: kill and restart both end `StopReading` and discard the old
  process's held writes. A kill's link cut is now an overlay at dispatch rather than a rewrite of
  `LinkPolicy`, so restart exposes exactly the independently scheduled policy — including a
  partition that predates the kill — while every write toward a killed stream peer still reports
  a transport error. Five named tests in `connection_faults.rs` pin both stall/kill event orders,
  the restart and partition repairs, and the documented stall-before-link precedence.
- Failing-first at merge base `96694aa`: the kill case reported `0` rather than `9` post-kill
  transport errors, the restart case delivered `0` rather than `6` post-restart writes, and the
  older-partition case reported `0` rather than `1` post-restart link break. The precedence test
  was already green at the base, as a characterization of the documented behavior it now pins.
- Independent review found the inverse schedule was still open: `KillNode` followed by
  `StopReading` re-added the dead node to the stalled set, and three later writes produced zero
  link breaks and three `stalled` notices. The failing-first test now runs that order too; the
  scheduled stall is recorded but cannot recreate a dead process's receive buffer. The original
  order now pauses before the kill and proves one write is held, then proves the kill discards it
  rather than merely observing an empty buffer that had never held anything.
- The accepted conformance-harness design now states the composition rule the implementation uses:
  stopped-node state makes the incident link produce its ordinary cut outcome without overwriting
  independently scheduled link policy; death dominates backpressure, and the link remains the only
  delivery-decision mechanism. The design records `CF-28` in its story scope and the local/upstream
  boundary explicitly.
- The same temporary probe source compiled at `96694aa` and at this implementation rendered
  byte-identical 95-line traces (SHA-256 `664a89588de2a8c5891b7df2feeccd98bd0dd189c87fd77075f6b8cb87197d9a`);
  the probe was removed afterward. The existing pinned trace literal and every existing seed and
  expected value are unchanged.
- Considered for upstream: **no — cluster simulation orchestration.** This changes how this
  repository's deterministic harness composes its node, link and backpressure faults; it adds no
  protocol-generic parsing, transaction, transport or testkit capability to the sipx kernel.

## Notes

- The review also observed that the pre-existing "byte-for-byte" tests compare run-to-run
  **inside one binary**, so they cannot detect a cross-version shift. The stronger check it built
  (compile the same probe at two revisions, compare rendered traces) is worth keeping as a
  standing gate step rather than a one-off — consider it here or as its own story.
