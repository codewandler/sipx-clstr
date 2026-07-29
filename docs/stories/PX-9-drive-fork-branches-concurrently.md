---
id: PX-9
title: Drive fork branches concurrently instead of draining them in order
pillar: Signalling
status: ready
priority: 1
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, driver]
note: a user with one dead device waits for Timer B before their live device's 200 OK is relayed
---

# Drive fork branches concurrently instead of draining them in order

## Goal

Make parallel forking actually parallel at the driver seam. The engine forks correctly and emits one
`Forward` per target, and then the driver drains each branch's response stream to completion before
looking at the next — so the slowest branch delays every branch behind it.

## Acceptance

- [ ] The driver polls all branches of a fork group concurrently — a `select` over their streams, or
      a `JoinSet` — rather than `while let Some(...) = pending.pop()` with an inner loop that runs the
      stream to exhaustion.
- [ ] **Failing-first**: a harness scenario registers two contacts at equal `q`, makes one a black
      hole, and requires the live branch's `200 OK` to be relayed in well under the kernel's Timer B.
      It fails today, and it fails by roughly the full timeout.
- [ ] The comment at the drain loop is corrected. It currently asserts that "with M1's single fork
      group, awaiting them in order is the same thing" — which is false for any group with more than
      one target, and the ordinary two-device registration is exactly that: two contacts, both
      `q=1000`, placed in one group by `lookup.rs`.
- [ ] RFC 3261 §16.7's response-selection behaviour is unchanged. This is a story about *when* branch
      events are read, not about which response wins; the existing `PX` vector rows for forking,
      `CANCEL` and Timer C must pass untouched.
- [ ] Branch ordering does not become a source of nondeterminism in the harness. The simulator owns
      its scheduling, so concurrent polling must still replay byte-identically from a seed — if a
      `select` introduces an ordering choice, that choice is the harness's to make, not the OS's.

## Progress

- (running log)

## Notes

- **The symptom a user sees.** Two registered devices, one of them unreachable over UDP. If the dead
  branch is popped first its stream produces nothing until the kernel's Timer B fires, and the
  `200 OK` already sitting in the live branch's stream is not read until then — roughly thirty
  seconds of silence before the answer is relayed.
- **This is a driver defect, not an engine defect.** `context.rs` groups and forks correctly per §16;
  the loss is entirely in how `driver.rs` consumes the effects. Keep the fix on that side of the
  sans-IO boundary — the engine must not learn about concurrency.
- Related and deliberately not folded in: the driver also swallows `SetTimer`/`ClearTimer`/`Terminate`
  and only logs `CancelBranch`, so Timer C never fires and a losing branch is never CANCELled — in-flight
  state is reaped only by the kernel's 180-second unanswered backstop. That is a second driver-seam gap
  on the same loop and wants its own story; note it here so whoever picks this up sees both.
- Considered for upstream: no. The kernel gives one response stream per branch and that is the right
  primitive; composing several is the driver's job.
