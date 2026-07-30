---
id: CF-12
title: A row counted as proved must assert what it claims
pillar: Foundation
status: in-progress
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

- [x] **Failing-first**: pick a row whose test demonstrably does not assert the row's claim, and
      show the checker passes it today. `PB-F-1` before `PX-10` is the worked example and is
      reconstructible from git; if a live instance exists, prefer it.
      → `scripts/check-vectors.py:self_test`, replaying the `7d21a22` row and test body verbatim.
      Before: `vectors: 1/1 rows proved, 0 deferred with a reason`, exit 0. After: `PB-F-1: its
      Expect column states 200 B, 180 s, and no assertion … compares it`, exit 1.
- [x] A row's registration carries enough for the check to be meaningful. The minimum that would
      have caught all three instances: a proved row names the **assertion**, not only the test — so
      a test that never mentions the row's quantity cannot claim it. Design this deliberately; a
      weaker rule that catches nothing is worse than no rule, because it will be believed.
      → `claims()` reads the row's `Expect` cell minus its citations; `compared()` reads the
      *compared arguments* of the test's assertions — not comments, not the signature, not each
      assertion's message argument. The report then names the value: `PB-V-1 | proved | … — asserts
      483`.
- [x] The check runs in `scripts/gate.sh` and reports honestly when it cannot decide. A row it
      cannot classify is not proved.
      → the `vectors` step already ran `--check`; a row with no vector-table line is `UNREADABLE`
      and counts as not proved, and a row whose value nothing compares is `shape only` and does not
      count as proved either.
- [x] A sweep of the currently-proved rows, with the result recorded as a number: how many assert
      their claim, how many are like `PB-F-1`. If that number is bad, **say so in the report** rather
      than quietly fixing the rows — the count is the finding.
      → **Of the 140 rows the report called proved, 61 state a value. 41 compare it; 20 do not.**
      No test was changed to improve that number. See `## The sweep` below.
- [x] `docs/reference/conformance.md`'s legend states what "proved" now guarantees and what it still
      does not. A reader currently has no way to know.
      → a *What these words mean* section, generated: what proved means, what shape only means, and
      four things proved still does not mean.
- [x] `scripts/gate.sh` green.

## Progress

- The checker now asks two questions instead of one. Coverage — *does a test claim this row* — is
  unchanged and still derived from test names; the byte-identical 140-row coverage set proves the
  refactor moved nothing. Beside it: *does that test compare what the row states?*
- The headline moved **140 → 120 of 498 proved**, with 20 rows reclassified `shape only`. Nothing
  regressed; the instrument stopped counting twenty rows it could not vouch for.
- `[[unasserted]]` in `vector-scope.toml` is the ledger of those twenty, one reason each saying what
  the test asserts instead. It is a ratchet, not a waiver: the gate fails on an unlisted row that
  becomes shape only, on a listed row whose proof has started comparing the value, and on a listed
  row that is not covered at all. Both stale directions were demonstrated, not assumed.
- Left for whoever picks these up: sixteen of the twenty are a missing assertion and little else —
  the `RA-D-*` and `LS-R-*` status codes especially, where `RA-D-2` already shows the idiom in the
  same file. Four are the check reaching its edge, and one is not a test problem at all (`PB-R-5`).

## The sweep

Run against the 140 rows the report called proved before this story:

| | rows |
|---|---|
| covered by a test | 140 |
| …of which state a value in the `Expect` column | 61 |
| …**…of which compare it** | **41** |
| …**…of which do not — the `PB-F-1` class** | **20** |
| …state no value; nothing to compare | 79 |

The twenty, with what each test compares instead, are in
[vector-scope.toml](../reference/vector-scope.toml) under `[[unasserted]]`:

`PB-V-3` `PB-F-1` `PB-R-5` `PB-R-9` `PB-C-1` `RA-D-5` `RA-D-7` `RA-D-8` `RA-D-9` `RA-A-3` `LS-L-1`
`LS-R-1` `LS-K-1` `LS-R-3` `LS-K-6` `CC-V-9` `LS-R-10` `LS-R-11` `LS-R-18` `LS-R-20`

Three things are worth naming out of that list:

- **`PB-R-5` is a row and a test that flatly disagree.** The row says that with branch A at `486`
  and branch B at `404`, the best response is `486`. `pb_r_5_the_best_of_two_failures_is_the_lowest_
  class_then_the_lowest_code` asserts `404`. §8 `R7` says only "the lowest class among received
  finals" and settles nothing within 4xx, so the spec does not adjudicate either. This has been
  counted as proved for the project's whole life. It is a **fourth instance of the pattern**, and
  the one the other three would not have caught: the number was not merely uncompared, it was
  contradicted.
- **Five of the ten registrar-auth rows in the list state `401` and none of them compares it.**
  `RA-D-5`, `RA-D-7`, `RA-D-8`, `RA-D-9`, `RA-A-3` all check `challenge.because` and the `stale`
  flag and stop. A challenge issued as `407` would pass every one of them.
- **`PB-F-1` is still shape only after `PX-10` fixed it.** Its test now compares the armed duration
  against `DEFAULT_TIMER_C` — the code's own constant — which is not the same as comparing it to the
  240 s the row states. Move the constant and the test still passes while the row goes quietly
  wrong, which is precisely how it came to read 180 s. The row that named the class is still in it.

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
