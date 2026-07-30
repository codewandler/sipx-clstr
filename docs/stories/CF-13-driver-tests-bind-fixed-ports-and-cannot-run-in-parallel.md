---
id: CF-13
title: Driver tests bind fixed ports, so two checkouts cannot be tested at once
pillar: Foundation
status: done
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

- [x] **Failing-first**: run the node crate's test suite twice concurrently from two checkouts and
      capture the `Address already in use` failure. The bug is a race, so the proof is a race.
- [x] A test that needs a listener gets one that cannot collide — bind port 0 and read back the
      assigned port, or an equivalent. The node already prints `listening on <addr>` after binding
      precisely so a caller need not guess.
- [x] `scripts/e2e-call.sh`'s ports stop colliding with the suite, or the collision is made
      impossible rather than merely unlikely.
- [x] The load-sensitive assertion in `dp11_the_shed_counters_reach_a_human` either becomes robust
      under load or is explicitly marked as serial. A test that fails once in five under fan-out
      trains people to re-run rather than to read, which costs more than the test is worth.
- [x] After the change, two concurrent full runs both pass — demonstrated, not asserted.

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

## Progress

**Done.** No integration test in the node crate picks a port any more.

*The race, before.* Two copies of the node crate's socket suite, run at once from one build:
four tests failed across three files, twice with `sipx-clstr: io: Address already in use (os error
98)` quoted verbatim in the panic message — `dp11_the_shed_counters_reach_a_human`,
`dp11_a_flood_cannot_exceed_the_admission_bound`, `ra_l_1_a_refusal_is_recorded_...` and
`fc2_the_startup_line_says_whether_authentication_is_on`. The second copy passed clean, which is the
shape of the complaint: the failure lands on whoever loses, not on whoever is wrong.

*The fix.* Every node binds `127.0.0.1:0` and is asked what it got.

- `driver::run_reporting(config, |addr| …)` is new; `run` is now that with a report that goes
  nowhere. It fires exactly where `println!("listening on …")` already fires — after the bind and
  after every startup refusal — so an in-process caller learns the address from the same place, at
  the same moment, as a script waiting on stdout. No second readiness contract was invented.
- `tests/support/mod.rs` holds both halves: `start_in_process` for a driver in the test's own
  process, and `BinaryNode` for the two suites that drive the binary and read its stderr.
- The advertised address stays a literal, because `Advertised` refuses port zero and advertise is
  decided before the bind. It costs nothing here: `DP-5` makes the two independent by design, nothing
  binds the advertised address, and no test in this suite routes to one.

*The load sensitivity.* `dp11_the_shed_counters_reach_a_human` and all four of `startup_warns.rs`
waited on a 1500 ms sleep. They now wait on the **output** — `shed_requests` for the sampling tick,
`node listening` for the startup record — returning as soon as it appears. `startup_warns` went from
1.50 s to 0.05 s, and neither test can lose a race against a clock any more.

*`scripts/e2e-call.sh`.* The phone ports — `15071`/`15081`, the two that actually collided with the
suite — are now asked of the kernel. The **node's** port deliberately stays `--port`'s to choose:
`sipx dial` addresses the node through the request-URI, and a request-URI with an explicit port is a
different address-of-record from one without it (location-service §3.2 N7), so a node on an ephemeral
port could not be dialled at the AoR its phones registered under. It never collided with the suite.

*Proof.* Two full `cargo test --workspace --all-features` runs against separate target directories,
concurrently: 37 suites green in each, zero failures, zero `Address already in use`. Separately, the
socket suite was run **three** ways in flight, eight times over — 24 concurrent runs, all green.
`scripts/gate.sh` is green and `scripts/e2e-call.sh` completes with audio on kernel-drawn ports.

*One trap worth knowing.* The first version of `BinaryNode` read one line of stdout and dropped the
reader. That closes the pipe, and the node's very next `println!` — `advertising <addr>`, immediately
after `listening on <addr>` — then died on `EPIPE`. It survived when the scheduler kept the two
writes together and killed the node when it did not, which is to say it failed under exactly the load
this story is about. Both pipes are now drained to EOF for the node's whole life.
