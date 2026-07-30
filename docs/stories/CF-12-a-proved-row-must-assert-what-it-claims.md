---
id: CF-12
title: A row counted as proved must assert what it claims
pillar: Foundation
status: ready
priority: 1
epic: conformance-harness
areas: [ci, build, docs]
note: PB-F-1 said "Timer C set 180 s" and its test compared only effect kinds — the row and the code never met
---

# A row counted as proved must assert what it claims

## Goal

`check-vectors.py` counts a row as **proved** when a test name maps to it. It cannot tell whether
that test asserts anything the row actually claims, and at least once it did not.

`PB-F-1` reads "Timer C set 180 s". Its test — `pb_f_1_a_dialog_forming_invite_is_record_routed_
with_a_branch_and_timer_c` — asserted:

```rust
assert_eq!([Kind::ResolveTargets, Kind::Forward, Kind::SetTimer], …)
```

Effect *kinds*. No duration, anywhere. So a row saying 180 s and code arming 180 s coexisted for the
project's whole life without ever being compared, and when `PX-10` corrected the value the row had
been silently wrong about, the "proof" would not have noticed either way. `PX-10` fixed that row;
this story is about the **class**.

This is the third instance of one pattern, which is why it is worth a story rather than a fix:

- `CF-8` found the conformance *headline* was derived arithmetically (`len(rows) - len(waived)`)
  rather than measured, and made it count real proofs.
- `DP-12` found `CC-V-9` was **deferred** rather than proved — the rule had a row, the row was in the
  waived column, and the defect shipped.
- `PX-10` found `PB-F-1` was **proved by a test that could not fail for the reason the row states**.

Each time the instrument reported a healthier number than the evidence supported. The headline is
honest now; the rows underneath it are not yet.

## Acceptance

- [ ] **Failing-first**: pick a row whose test demonstrably does not assert the row's claim, and
      show the checker passes it today. `PB-F-1` before `PX-10` is the worked example and is
      reconstructible from git; if a live instance exists, prefer it.
- [ ] A row's registration carries enough for the check to be meaningful. The minimum that would
      have caught all three instances: a proved row names the **assertion**, not only the test — so
      a test that never mentions the row's quantity cannot claim it. Design this deliberately; a
      weaker rule that catches nothing is worse than no rule, because it will be believed.
- [ ] The check runs in `scripts/gate.sh` and reports honestly when it cannot decide. A row it
      cannot classify is not proved.
- [ ] A sweep of the currently-proved rows, with the result recorded as a number: how many assert
      their claim, how many are like `PB-F-1`. If that number is bad, **say so in the report** rather
      than quietly fixing the rows — the count is the finding.
- [ ] `docs/reference/conformance.md`'s legend states what "proved" now guarantees and what it still
      does not. A reader currently has no way to know.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

## Notes

- Found by `PX-10` while correcting `PB-F-1` itself, and reported rather than absorbed — the same
  discipline that surfaced the other two. Verified independently at the coordinator's integration:
  `git show 7d21a22:crates/sipx-clstr-proxy/tests/vectors_proxy.rs` shows the kinds-only assertion.
- **Do not turn this into a rule that every test must contain a magic number.** Plenty of rows are
  about ordering, shape, or refusal, and asserting a quantity would be meaningless for them. The
  property wanted is narrower: *if the row states a value, something in its proof must compare that
  value.*
- Related but distinct from `EX-12`, which is about rows the gate cannot see at all because no spec
  owns their prefix. This one is about rows the gate sees and believes. Doing both is reasonable;
  doing neither because each looks like bookkeeping is how the conformance report becomes decoration.
- The conformance report is this project's public capability statement — `website/docs/reference/
  conformance.md` points at it and `docs/vision.md` leans on it. An instrument that overstates is
  worse than none, because the overstatement is what gets quoted.
