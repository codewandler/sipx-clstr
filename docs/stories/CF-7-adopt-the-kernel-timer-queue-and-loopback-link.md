---
id: CF-7
title: Adopt the kernel timer queue and loopback link
pillar: Platform
status: ready
priority: 1
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: unblocked by sipx v0.7.0 — the queue is generic over its instant now; the link stays local
---

# Adopt the kernel timer queue and loopback link

## Goal
Replace the harness's own timer queue and in-process link with the kernel's, once sipx `X-14`
ships them, so the two repos share one cancellation discipline instead of two that drift.

## Acceptance
- [ ] `sipx-clstr-sim`'s generation-counter timer machinery is deleted in favour of the kernel's
      generalized `TimerQueue` (generic key, `now` passed in).
- [ ] The seeded loss/duplication/latency link is the kernel testkit's, not this crate's.
- [ ] Every existing scenario passes unchanged, at the same seeds, producing the same traces — or
      the seed changes are recorded deliberately in the CHANGELOG, because a silent reshuffle
      invalidates every regression seed in the suite.

## Progress
**Attempted 2026-07-29 and re-blocked on a gap `X-14` did not close.** `v0.4.0` does ship both
pieces — `sipx-testkit/src/link.rs` and a `TimerQueue` generalized over its *key* — but they were
generalized for the kernel's own loopback tests, which run on a tokio clock. Neither is drivable
from virtual time, and adopting either would cost more determinism than the convergence is worth.

- **`TimerQueue<K>` is generic over its key and not over its clock.** It is keyed to
  `tokio::time::Instant` (`sipx-transport/src/timers.rs:19`), and that type has exactly two
  constructors: `now()`, which reads the machine's clock, and `from_std`, which needs a
  `std::time::Instant` that has no zero either. There is no way to build an epoch. Anchoring on a
  real `now()` inside the simulation contradicts `time.rs`'s own premise — *"a simulation that
  could ask the operating system what time it is would be a simulation whose results depend on the
  machine that ran it"* — which is the property `CF-5` exists to guarantee.
- **`TimerQueue` also holds no payload.** The harness runs `EventQueue<Event>`: deliveries, timers
  and link breaks in **one** totally ordered queue, ties broken by insertion sequence. That single
  order is what makes the trace a pure function of (scenario, seed). `TimerQueue` stores keys only,
  so adopting it would split that into two heaps merged at pop time, and the total event order
  would become a function of how the merge breaks ties across two structures. That is a
  determinism regression wearing the costume of convergence.
- **`testkit::Link` is the wrong shape twice over.** It is two-sided (`Side::Left`/`Right`) where
  the harness needs an N-node mesh with partition state; it has no stream class, so RFC 3261 §16.9
  transport failure has nowhere to live; and it draws its faults from its own internal splitmix64
  rather than from `SimRng`. That last one alone fails this story's third criterion: moving fault
  randomness into the kernel's generator changes what every seed in the suite means.
- **Nothing was adopted and nothing was re-recorded.** The alternative was to bend the harness
  around the kernel's clock and re-record the pins; the pins are the regression suite, and a
  re-recorded pin proves only that the new behaviour is the new behaviour.

**Unblocked 2026-07-29 by sipx `v0.7.0`.** The kernel is now
`TimerQueue<K, I = Instant>` with `I: Ord + Copy + Add<Duration, Output = I>` — generic over its
instant, defaulting to the old type so no existing caller changed. `SimTime` satisfies those bounds
with one `Add<Duration>` impl, so the harness can hand in its own clock and the module doc's claim
("nothing here has an opinion about what an instant means") is finally true of the signature.

**The link half is settled the other way, and stays local.** `testkit::Link` is unchanged in
`v0.7.0`: still `Side::Left`/`Right` where the harness needs an N-node mesh, still no stream class
for RFC 3261 §16.9, still drawing faults from its own generator rather than `SimRng`. Adopting it
would change what every seed in the suite means, which this story's own third criterion forbids.
`CF-1` reserved the right to decide the testkit split **per component**; this is that decision,
recorded rather than left as an unmet checkbox — the queue converges, the mesh does not.

## Notes
- Upstream story: sipx [X-14](https://github.com/codewandler/sipx/blob/main/docs/stories/X-14-testkit-timer-queue-and-loopback-link.md),
  filed by `CX-1`. Ledger: [upstream.md](../upstream.md).
- Not a fork in the sense AGENTS.md rule 6 forbids: neither piece is protocol logic, both are
  small, and this story is the commitment to converge rather than to keep two.
- The seed-stability criterion is the one with teeth. The harness's own generator
  (`sipx-clstr-sim::rng`) is deliberately **not** part of this swap: it stays local precisely so
  that adopting kernel machinery cannot change what a seed means.
