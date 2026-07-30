---
id: PX-11
title: Settle which 4xx wins when two branches fail, because the row and its test say different things
pillar: Platform
status: ready
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

- [ ] `proxy-behavior` §8 gains a within-class rule, or `R7` states explicitly that the choice within
      a class is unspecified and the row is rewritten to assert only what is guaranteed. Either is a
      defensible answer; silence is not, because silence is what let a row and its test disagree.
- [ ] Whichever way it goes, `PB-R-5`'s row text and its test agree afterwards, and the row leaves
      `[[unasserted]]` in `docs/reference/vector-scope.toml` — it is a real proof again, not a
      recorded gap.
- [ ] The decision cites RFC 3261 §16.7 step 6 and says what the RFC does and does not fix. "The RFC
      allows either" is the *premise* of the decision, not a substitute for making it.
- [ ] If the chosen answer changes behaviour, the change is in `sipx-clstr-proxy` with a failing-first
      test; if it changes only the row, say plainly in the story that no code moved and why that is
      correct.
- [ ] Consider the sibling cases while the question is open: two 5xx, and a 4xx against a 6xx (the
      RFC does fix that one — 6xx wins). Add rows if they are missing.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

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
