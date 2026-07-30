---
id: PX-11
title: Settle which 4xx wins when two branches fail, because the row and its test say different things
pillar: Platform
status: done
priority: 1
epic:
areas: [proxy, docs]
note: PB-R-5 claims 486 is forwarded; its own test asserts 404, and the spec settles neither
---

# Settle which 4xx wins when two branches fail, because the row and its test say different things

## Goal

`PB-R-5` and the test that "proves" it state opposite outcomes, and have done for the life of the
project while the row counted as **proved**.

The row (`docs/specs/proxy-behavior.md:267`):

```
| PB-R-5 | A `486`, B `404`, all concluded | best = `486` forwarded |
```

The test (`crates/sipx-clstr-proxy/tests/vectors_proxy.rs`):

```rust
assert_eq!(statuses(&out), [404], "404 beats 486 within 4xx");
```

§8 `R7` says only "the lowest class among received finals" and settles nothing *within* 4xx, so the
specification does not adjudicate between them either. RFC 3261 §16.7 step 6 chooses the lowest
**class** and then permits any response within it, so the RFC does not force the answer — which is
exactly why the spec has to give one.

This was invisible because `check-vectors.py` counted a row proved when a test *name* mapped to it,
whatever the test asserted. `CF-12` changed that, and this row is the reason it matters: the value
was not merely uncompared, it was **contradicted**.

## Acceptance

- [x] `proxy-behavior` §8 gains a within-class rule, or `R7` states explicitly that the choice within
      a class is unspecified and the row is rewritten to assert only what is guaranteed. Either is a
      defensible answer; silence is not, because silence is what let a row and its test disagree.
      → §8 `R7` now defers to new **§8.1**, a rank table with the reasoning that picks it.
- [x] Whichever way it goes, `PB-R-5`'s row text and its test agree afterwards, and the row leaves
      `[[unasserted]]` in `docs/reference/vector-scope.toml` — it is a real proof again, not a
      recorded gap. → row unchanged at `486`; the test asserts `486`; the entry is deleted and
      `conformance.md` reads ``| `PB-R-5` | proved | … — asserts `486` |``.
- [x] The decision cites RFC 3261 §16.7 step 6 and says what the RFC does and does not fix. "The RFC
      allows either" is the *premise* of the decision, not a substitute for making it. → §8.1 quotes
      step 6 in full and separates its MUST, its SHOULD and its MAY before deciding inside the MAY.
- [x] If the chosen answer changes behaviour, the change is in `sipx-clstr-proxy` with a failing-first
      test; if it changes only the row, say plainly in the story that no code moved and why that is
      correct. → behaviour changed: `context.rs` `within_class_rank` + `best_final`, failing-first
      via `pb_r_5_…` (`left: [404] right: [486]`).
- [x] Consider the sibling cases while the question is open: two 5xx, and a 4xx against a 6xx (the
      RFC does fix that one — 6xx wins). Add rows if they are missing. → `PB-R-13` (two 5xx) and
      `PB-R-14` (6xx over a stored 4xx), plus `PB-R-11` and `PB-R-12` for the two rank boundaries.
- [x] `scripts/gate.sh` green.

## Progress

**The decision: the row was right, the code was wrong, and the code moved.** `486` beats `404`.
`proxy-behavior` §8 gains **§8.1**, a within-class rank; `R7` now points at it.

RFC 3261 §16.7 step 6, quoted in §8.1, splits three ways: 6xx is a **MUST**, the lowest class
otherwise a **SHOULD**, and the pick inside the chosen class an explicit **MAY** — "the proxy MAY
select any response within that chosen class" — with one **SHOULD** steer, "give preference to
responses that provide information affecting resubmission … such as 401, 407, 415, 420, and 484".
Neither `486` nor `404` is in that set, so `PB-R-5` sits squarely inside the MAY and the RFC will
not answer it. That is the premise. The answer comes from two things the RFC does not say and this
spec now does:

1. **A forked proxy answers one question with one response.** Each branch's final is a statement
   about one *contact*; what goes upstream is a statement about the *AoR*. `404 Not Found` says the
   address does not exist — falsified the moment another branch answers at all. Sending it over a
   `486` tells the caller something we know to be untrue about what they asked for, and tells them
   to stop rather than to retry. Forking to a busy contact and a stale one is an ordinary
   registration, so this is user-visible, not a tie-break nobody sees.
2. **Numeric order was doing a job it cannot do.** The old `min_by_key((class, code))` is not a
   preference: it let `408` — a branch that never answered — beat `486`, a branch that did, and it
   would let a downstream `400 Bad Request` beat both, reporting a fault in our own message as the
   callee's status. Lowest code survives as the *tie-break inside a rank*, which is a job it is
   good at.

The rank: 1 = `401 407 415 420 484` (step 6's own SHOULD, verbatim); 2 = `480 486` (a branch reached
the user and the user's side answered); 3 = everything else, lowest code first. Ranks 1–2 are 4xx
only — the class the RFC scopes its preference to — so 5xx and every other class fall wholly to the
tie-break, claiming nothing beyond the MAY. `R8` still removes `503` before any of it runs.

Rows: `PB-R-5` **unchanged** — it was correct and the test was not. Added `PB-R-11` (rank 1 over a
lower code: `404` + `484` → `484`), `PB-R-12` (`486` + a timeout → `486`; silence must not outrank
an answer — the case the old rule got most plainly wrong), `PB-R-13` (two 5xx → `500`, tie-break
only), `PB-R-14` (a stored `404` then a `600` → `600`, the MUST). All four assert the literal the
row states, per `CF-12` — no comparison against a constant of the code's own.

`PB-R-10`'s test was collateral: it asserted `[408]` under a heading of "408 beats 486 within 4xx",
which the new rule reverses. The row itself claims only "as `408` from branch", so the test now
pairs the timeout against a branch `500` — still asserting the literal `408`, now proving the row's
actual claim (a timeout is an ordinary 4xx final) instead of a within-class ordering the row never
made. `PB-R-10` stays proved.

Counts: `120/498 proved, 20 shape-only` → `125/502 proved, 19 shape-only, 358 deferred`. Up, not
down. Considered for upstream (`AGENTS.md` #6): **no** — §16.7 step 6 hands the choice to the proxy
deliberately, so a platform's answer to a MAY is policy, not kernel protocol, and sipx must not
prejudge it for its consumers.

## Notes

- Found by `CF-12`'s sweep and confirmed independently at integration by reading the row and the test
  side by side. `CF-12` deliberately did not pick an answer — it is a specification decision, not a
  checker fix — which was the right call.
- This is the **fourth** instance of one pattern and the one the other three could not have caught:
  `CF-8` found the headline derived rather than measured, `DP-12` found a rule's row sitting in the
  deferred column, `PX-10` found a row proved by a test that compared nothing. Here the row and the
  proof were both present, both ran, and said opposite things.
- Do not settle it by changing the row to `404` on the grounds that the code already does that. That
  is deciding the specification from the implementation, which inverts `AGENTS.md` non-negotiable #4.
  Decide what is right, then move whichever side is wrong.
- Worth noting for whoever picks this up: `486 Busy Here` and `404 Not Found` say materially
  different things to a caller — one is "they are there and unavailable", the other "no such user".
  Forking to a registered contact that is busy and an unregistered one is an ordinary case, so this
  is a real user-visible choice, not a tie-break nobody sees.
