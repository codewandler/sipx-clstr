---
id: CF-7
title: Adopt the kernel timer queue and loopback link
pillar: Platform
status: ready
priority: 1
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: X-14 landed in sipx v0.4.0 — delete the harness's local copies
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
- **Unblocked 2026-07-29.** sipx `v0.4.0` ships `X-14`: the seeded link is
  `sipx-testkit/src/link.rs` with its own `tests/loopback.rs`, and the generalized queue is in
  `sipx-transport/src/timers.rs`. The workspace already pins `v0.4.0` (`RG-2`), so this is
  takeable with no further dependency work.
- `CF-5` shipped the harness with local implementations of both, which is what its fourth
  acceptance criterion ("components upstreamed per CF-1's split are consumed from sipx, not
  forked") could not satisfy while the upstream work was unfiled. This story is what closes it.

## Notes
- Upstream story: sipx [X-14](https://github.com/codewandler/sipx/blob/main/docs/stories/X-14-testkit-timer-queue-and-loopback-link.md),
  filed by `CX-1`. Ledger: [upstream.md](../upstream.md).
- Not a fork in the sense AGENTS.md rule 6 forbids: neither piece is protocol logic, both are
  small, and this story is the commitment to converge rather than to keep two.
- The seed-stability criterion is the one with teeth. The harness's own generator
  (`sipx-clstr-sim::rng`) is deliberately **not** part of this swap: it stays local precisely so
  that adopting kernel machinery cannot change what a seed means.
