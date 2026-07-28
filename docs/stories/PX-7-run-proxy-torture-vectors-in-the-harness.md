---
id: PX-7
title: Run proxy torture vectors in the harness
pillar: Signalling
status: done
priority:
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, harness]
note: M1 #8 · the PB table as a generated, checked report — and it found a deleted test
---

# Run proxy torture vectors in the harness

## Goal
Run the complete PX-1 vector suite plus adversarial schedules — retransmission storms, reordering, duplicate UDP — as seeded harness scenarios in CI.

## Acceptance
- [x] Every PX-1 vector is executed by a named harness scenario; failures reproduce from the printed seed.
- [x] Adversarial schedules (retransmission, reorder, duplication) run green across a seed corpus in CI.

## Progress
**`docs/reference/conformance.md` is generated, committed, and checked**: 34 of the proxy spec's 42
`PB` rows proved, 8 deferred with a reason and a story. `ET-1` later added the probe spec's `EP`
rows to the same report. `scripts/check-vectors.py --check` is in the
gate and in CI, and fails three ways:

1. a spec row is neither covered nor deferred;
2. a deferral has no reason or no story;
3. **a deferred row is covered** — a stale deferral, which is how a coverage report starts lying
   about what it proves.

**Coverage is derived from test *names*, not from a list.** `fn pb_v_8_a_max_breadth_of_one_…` covers
`PB-V-8`; a `// covers: PB-R-4` comment does the same for a test that does not want the name. A
hand-maintained list would rot silently — deleting a test would leave the claim standing. A name
cannot: deleting the test deletes the claim, which is exactly what happened below.

### What the check found the moment it existed

**`PB-V-9`'s test had been deleted, and nothing noticed.** It went with the block I rewrote when
fixing the loop-detection cookie, and the suite stayed green because 45 other tests still passed. The
checker flagged the row as unproved on its first run. Restored, with a note in the test saying how it
came back.

**`PB-V-8` was not implemented at all.** `Max-Breadth` bounds *parallel* fan-out; with a budget of 1
and two targets my code forked both anyway. RFC 5393 §5.2 requires the surplus to be **serialized
behind**, not truncated — truncating silently loses a device the user registered, which is the kind
of failure nobody notices until a call does not ring on one phone. Fixed: the group is split at the
budget and the remainder stays queued.

Four more rows had no proof and now do: `PB-P-1` (a strict-routing predecessor put our Record-Route
in the Request-URI), `PB-P-5` (an expired token is `403` exactly like a tampered one — treating
"expired" as softer would make the expiry the attacker's choice), `PB-R-4` (a late `2xx` in its own
right rather than as a corollary), `PB-R-10` (a branch timeout as `408`).

### The adversarial half

`crates/sipx-clstr-sim/tests/proxy_torture.rs`: 35 % loss, 30 % duplication, 1–120 ms jitter, and a
retransmission loop standing in for Timer A/E — the harness has no transaction layer, so without it
heavy loss would just lose calls and the scenario would assert nothing about the engine.

Four properties over a **pinned 8-seed corpus**:

- the call completes despite the network;
- **a retransmitted INVITE never forks twice** — asserted on the branches the engine created, not on
  messages observed, because the network legitimately duplicates messages and only the branch count
  is the property;
- every seed replays byte for byte;
- a total partition concludes the call rather than hanging.

`HARNESS_SEED=0x… cargo test` replays exactly one run, every failure message prints the seed that
produced it, and a malformed `HARNESS_SEED` **panics** rather than silently falling back to the
corpus — otherwise a developer would think they had reproduced something they had not. The corpus is
pinned rather than random so that green means the same thing twice; `CF-1`'s discipline is that a
nightly sweep finds new seeds and each one gets appended here as an explicit regression.

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
- `docs/reference/vector-scope.toml` holds the deferrals: `PB-C-4` (a CANCEL matching nothing is the
  driver's), `PB-S-1`…`PB-S-4` (stateless mode, `PX-4`, M2) and `PB-A-1`…`PB-A-3` (transaction
  affinity, multi-node, M2). It is the narrow PB-only ancestor of the registry `EX-2` specifies and
  `CF-2` reports from, and folds into it when that exists.
- The generated report is on the published site, so a reader who does not run the suite can still see
  what is proved.
