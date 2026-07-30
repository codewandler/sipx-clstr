---
id: CF-13
title: Driver tests bind fixed ports, so two checkouts cannot be tested at once
pillar: Foundation
status: ready
priority: 2
epic: conformance-harness
areas: [ci, build, node]
note: observed twice in one afternoon — "Address already in use" and a load-sensitive flake, neither caused by the diff under test
---

# Driver tests bind fixed ports, so two checkouts cannot be tested at once

## Goal

The node crate's integration tests bind **hard-coded ports** on real sockets. Two worktrees running
`cargo test` at the same time collide, and the failure appears in whichever diff happens to be under
test — which is the worst possible place for it, because the natural reading is "my change broke
this".

Observed twice in one session, both times provably unrelated to the diff being tested:

- `tests/admission_bound.rs`: `dp11_a_flood_cannot_exceed_the_admission_bound` and
  `dp11_the_shed_counters_reach_a_human` failed with
  `sipx-clstr: io: Address already in use (os error 98)`. The file was untouched by the commit under
  test, passed in isolation, and passed in every later run.
- A separate review saw `dp11_the_shed_counters_reach_a_human` fail once and pass on four
  subsequent runs. That test sleeps 1500 ms waiting for a 500 ms sampling tick
  (`tests/admission_bound.rs:405-433`), so it is load-sensitive by construction, and a parallel
  fan-out is exactly the load.

Known offenders: `admission_bound.rs` (15081), `fork_branches.rs` (15091/15092),
`auth_observable.rs` (15091/15092 — already colliding with `fork_branches.rs` by inspection),
`startup_warns.rs`, and `scripts/e2e-call.sh` (15071/15081, which collides with the suite).

## Acceptance

- [ ] **Failing-first**: run the node crate's test suite twice concurrently from two checkouts and
      capture the `Address already in use` failure. The bug is a race, so the proof is a race.
- [ ] A test that needs a listener gets one that cannot collide — bind port 0 and read back the
      assigned port, or an equivalent. The node already prints `listening on <addr>` after binding
      precisely so a caller need not guess.
- [ ] `scripts/e2e-call.sh`'s ports stop colliding with the suite, or the collision is made
      impossible rather than merely unlikely.
- [ ] The load-sensitive assertion in `dp11_the_shed_counters_reach_a_human` either becomes robust
      under load or is explicitly marked as serial. A test that fails once in five under fan-out
      trains people to re-run rather than to read, which costs more than the test is worth.
- [ ] After the change, two concurrent full runs both pass — demonstrated, not asserted.

## Notes

- Filed by the coordinator after `PX-10` and an independent review each hit it from different
  directions on the same afternoon. Neither could attribute the failure to its own diff, and both
  spent real effort proving a negative — that cost is the reason this is `priority: 2` and not `3`.
- This is a property of the harness, not of any story that happened to trip on it. The fan-out
  workflow this repository now uses makes it structural: every added worktree raises the collision
  probability, and the symptom always lands on an innocent diff.
- The fixed ports are house style and were reasonable when the suite ran alone. `fork_branches.rs`
  deliberately chose 15091/15092 to avoid the known ones, which is the workaround this story
  replaces — and `auth_observable.rs` then picked the same pair, which is the workaround failing.
- Ports are not the only shared resource: `/tmp` here is a tmpfs shared across worktrees and filled
  during this session, producing `Disk quota exceeded` and bare exit-1s from a proof harness. Worth
  checking whether the suite should use a per-run temporary directory too.
