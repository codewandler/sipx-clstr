---
id: CF-5
title: Implement the deterministic cluster harness
pillar: Platform
status: done
priority:
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness]
note: M1 #3 · the harness — PX-7 and RG-3 vector runs depend on it
---

# Implement the deterministic cluster harness

## Goal
Build what CF-1 designs: the seeded, virtual-time, multi-node simulation — virtual clock,
in-memory transports, scripted topologies — that every M1 vector story runs under. CF-1 decides;
this story ships the runtime.

## Acceptance
- [x] A scenario runs N platform nodes and M simulated endpoints on a virtual clock; timers fire
only when time is advanced, and two runs with the same seed produce identical traces.
- [x] In-memory transports support the loss/duplication/reordering/latency knobs CF-1 specifies
(partition and kill schedules arrive with CF-4).
- [x] A trivial end-to-end scenario (one node, REGISTER then INVITE against stub logic) passes
in CI as the harness's own smoke test.
- [ ] ~~Components upstreamed per CF-1's sipx-testkit split are consumed from sipx, not forked.~~
**Re-scoped to `CF-7`:** sipx `X-14` was only filed by `CX-1` this milestone and has not landed,
so there is nothing upstream to consume yet. The harness carries its own timer queue and link,
and `CF-7` is the commitment to delete them — including the requirement that adopting the
kernel's does not silently change what any recorded seed means.

## Progress
`sipx-clstr-sim`, 49 tests. Six modules, each answering one of CF-1's six commitments.

**Time is a discrete-event queue.** `SimTime` is nanoseconds since scenario start and exists only
inside the scheduler; advancing time is popping the queue. Equal deadlines break by insertion
sequence, which is what makes the total event order a function of (scenario, seed) rather than of
hash iteration. Timer cancellation is by generation counter — a stale entry is discarded at pop
rather than hunted down in the queue, since a timer that is set and cleared repeatedly would
otherwise make removal the hot path.

**The generator is written out, not imported.** `SplitMix64` seeding and xoshiro256\*\* output, in
about forty lines. `rand`'s own documentation says its general-purpose generators may change
between versions, and a cargo update that silently reshuffled every recorded seed would break the
one property the whole design exists for. It is pinned three ways: `SplitMix64`'s published
output for seed 0, xoshiro from state `[1, 2, 3, 4]` worked out by hand, and a composite vector.

- **The reference algorithm was implemented wrongly first**, and the hand-computed vector is what
  caught it. xoshiro's state update is sequential — each step reads what the step before it wrote
  — and the natural way to write it in Rust is one array literal computing every element from the
  *old* state. That produces a different, weaker generator that passes every smell test. The
  second output from state `[1, 2, 3, 4]` is zero **only** under the correct order, so that one
  assertion is the whole difference.

**Named streams.** Every consumer draws from a stream keyed by a stable label, so adding a link
never reshuffles the others. Certain outcomes consume no randomness at all — a clean link draws
nothing — so putting a lossless link into a topology cannot shift what every other link draws.

**Two link kinds, and the difference is load-bearing.** A datagram link samples per message and
gets its reordering from latency jitter, the way a wire does. A stream link is FIFO however the
jitter lands, never loses a message however lossy the policy, and fails by *breaking* — a
transport error (§16.9), not silence. A stream link that could lose a message would let a test
assert behaviour no real transport produces.

**Messages are serialized and re-parsed across every link**, exactly as on a wire, so a node that
builds a message it cannot itself write out is a bug the harness catches. An arrival that will not
parse is traced as malformed rather than dropped: a serializer bug and an idle network look
identical from the receiving end, and only one of them is a defect.

**The smoke scenario** (`tests/smoke.rs`) is one edge and two endpoints: both register, one calls
the other, the callee answers, ACK and BYE cross the node end to end. Five tests around it — the
call completes; the same seed replays byte for byte under jitter *and* duplication; loss is
visible in the trace rather than silent; latency puts the call where the clock says; an unroutable
target is refused with `404` rather than dropped. The logic is stubs, and `RG-3`/`PX-5` delete
them rather than adapt them.

**A livelocked scenario fails instead of hanging.** A step budget turns two nodes answering each
other forever into a failing test with a virtual timestamp, which is worth more than a CI job that
times out with no output.

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md). This story exists because
the epic's done-criteria said "implemented early in M1" while no story implemented it.
- The stub edge in the smoke test rebuilds the whole header collection to pop one `Via`, with a
  comment saying so. That is sipx `S-15` in miniature, and it is left visible rather than tidied
  away because it is the argument for the upstream story.
- Fault *schedules* — partition and kill windows over time — are `CF-4`. What the network needs
  from a schedule is here: `set_partitioned` on either direction of any link.
