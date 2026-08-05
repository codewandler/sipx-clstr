---
id: CF-28
title: A stalled peer outlives the node it was stalled on
pillar: Foundation
status: ready
priority: 3
design:
epic:
areas: [harness]
note: found reviewing CF-26 — killing or restarting a node does not clear StopReading, so a dead node silently absorbs traffic forever and a "new process" re-accumulates held writes
---

# A stalled peer outlives the node it was stalled on

## Goal

Make the harness's connection faults compose the way their own documentation says they do.
`CF-26` gave the sim reconnect, restart and backpressure, and its review found four places where
one fault silently outlives or over-reaches another — each of which would make an `AF-7`
owner-RPC scenario pass or fail for a reason that is the harness's rather than the code's.

## Acceptance

- [ ] **A killed node stops absorbing writes.** `send` checks `not_reading` before reaching the
      link (`sipx-clstr-sim/src/sim.rs:786`), and neither `KillNode` (`:320`) nor `restart`
      (`:396`) clears it — only `resume_reading` (`:425`) does. Measured at CF-26's tip:
      kill alone gives `errors=9 stalled=0 held=0`; stall-then-kill gives
      `errors=0 stalled=10 held_for_dead_node=9`. That contradicts the intent stated three lines
      above the kill's own link cut (`:302-303`: "cutting only outbound would leave it silently
      absorbing traffic"). Failing-first test pins the stall-then-kill case.
- [ ] **A restart drains what the old process held.** `Fault::RestartNode`'s doc frames a restart
      as a new process with an empty table (`fault.rs:117-135`) and `StopReading`'s says a restart
      "discard[s] what was held" (`fault.rs:149`) — measured, it discards and then re-accumulates
      (`held_after_restart=5`).
- [ ] **A restart heals only what the kill cut.** `restart` sets `partitioned=false` for every
      peer whenever `was_killed` (`sim.rs:389-393`), so a `Partition` scheduled *before* the kill
      is silently healed by the restart, against `fault.rs:126`'s claim that it "puts its links
      back exactly as the kill cut them".
- [ ] **The documented precedence has a test.** Stall-beats-link (`fault.rs:146-150`) is real —
      with `StopReading(B)` and `Partition{A|B}` in one window, all four held writes deliver at
      the resume, three written while the link was cut — but nothing pins it, so a future
      reordering of `sim.rs:786` against `dispatch` flips it undetected.
- [ ] Every existing pinned vector and byte-for-byte replay still passes **unchanged**; the
      cross-version check `CF-26`'s review introduced (same probe compiled at two revisions,
      rendered traces compared) stays green.

## Progress

- Filed at `CF-26`'s integration from its independent review's four minor findings. None is
  reachable by an `owner-rpc` §10 row today — §10's `owner_rpc_dial_is_suppressed_after_failure`
  puts the owner down without a stalled client — so `AF-7` is not blocked by this.

## Notes

- The review also observed that the pre-existing "byte-for-byte" tests compare run-to-run
  **inside one binary**, so they cannot detect a cross-version shift. The stronger check it built
  (compile the same probe at two revisions, compare rendered traces) is worth keeping as a
  standing gate step rather than a one-off — consider it here or as its own story.
