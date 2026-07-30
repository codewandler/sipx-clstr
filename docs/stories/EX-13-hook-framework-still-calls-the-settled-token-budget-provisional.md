---
id: EX-13
title: hook-framework still calls the settled token budget provisional
pillar: Platform
status: in-progress
priority: 3
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [hooks, specs]
note: AF-1 landed and made the 64-byte sub-budget normative; three hook-framework rows still say "placeholder until AF-1"
---

# hook-framework still calls the settled token budget provisional

## Goal

[affinity-token](../specs/affinity-token.md) §3 settled the module-fact sub-budget and claimed the
authority for it in writing:

> **[sipx-clstr] The module-fact sub-budget is 64 bytes** (`F ≤ 64`) … This spec is the budget
> authority; 64 B is adopted because it fits the F4 budget with headroom (§5 shows the arithmetic at
> F = 0 and F = 64).

`AF-1` is `done`. [hook-framework](../specs/hook-framework.md) has not been told:

- `§5` class (b) — "The budget authority is [affinity-token](../specs/affinity-token.md) (AF-1);
  **placeholder until AF-1 fixes the layout: 64 bytes** … this row is re-reviewed when AF-1 lands"
- `§9` `G5` — "Σ `TokenFact.max_bytes` over the selected set ≤ the module-fact sub-budget
  (§5 class b; **provisional 64 bytes until AF-1**)"
- `§9` vector `HF-7` — "`TokenFact` declarations summing to 72 bytes against the **provisional**
  64-byte sub-budget"

The number is right in all three places, so nothing is broken today. What is wrong is the *modality*:
a normative startup rule (`G5`) and the vector that proves it both describe a settled constant as
provisional and name a story that closed as the thing that will settle it. A reader deciding whether
`G5` may be relied on gets the wrong answer from the spec, and the next person to touch the layout has
two documents claiming authority over the same byte.

`EX-8` found this and wrote it down rather than fixing it, because it was genuinely outside its scope:

> Adjacent, found by `EX-6` and **not** part of this story: `docs/specs/hook-framework.md:187` and
> `:287` still call the 64-byte module-fact sub-budget a "placeholder until AF-1 fixes the layout".
> `AF-1` has landed — affinity-token §3 now states the 64-byte sub-budget normatively and names
> itself the budget authority, with §5 carrying the arithmetic — so the provisional wording is stale.

It has stayed there since. `:187` has since been fixed by unrelated editing; `:287` has not, and `G5`
and `HF-7` were never in that note at all.

## Acceptance

- [x] `§5` class (b), `G5` and `HF-7` state the sub-budget as settled and cite
      [affinity-token](../specs/affinity-token.md) §3 as the authority. No rule in this spec asserts
      a value it does not own — §3 owns 64, this spec owns the summation over the selected module set.
- [x] The words "placeholder", "provisional" and "until AF-1" are gone from every one of the three,
      and `grep -nE 'provisional|placeholder' docs/specs/hook-framework.md` returns only `H11`'s
      "provisional or final" response, which is RFC 3261 terminology and not a budget.
- [x] `HF-7`'s 72 > 64 arithmetic is unchanged and its test still passes. This story changes what the
      row *says about the status of* the number, never the number — if a vector's expectation moves,
      that is a different story and this one has gone wrong.
- [x] Whether `affinity-token` §3's "the width leaves room if a future version renegotiates the
      sub-budget" needs a matching forward-compatibility sentence in `G5` is decided either way and
      recorded. One spec keeping a door open that the other closes is how this class of drift starts.
- [x] `scripts/gate.sh` green.

## Progress

- Three rows corrected in `docs/specs/hook-framework.md`, and only those three — a
  `grep -nE 'AF-1|64' docs/specs/hook-framework.md` before the edit returned exactly the three lines
  named in the Goal, so the scope was closed by inspection rather than assumed.
  - `§5` class (b) (`:287`) — the *Bound* cell now names
    [affinity-token](../specs/affinity-token.md) §3 as the budget authority and states that this spec
    "sets no value of its own and owns only the summation over the selected module set (G5)". The
    constant is **not** restated here at all: a bound cell that repeats 64 is the second copy that
    made this drift possible.
  - `G5` (`:522`) — "provisional 64 bytes until AF-1" is replaced by the sub-budget "which
    [affinity-token](../specs/affinity-token.md) §3 fixes normatively", plus the
    forward-compatibility sentence decided below.
  - `HF-7` (`:732`) — "provisional 64-byte sub-budget" → "64-byte sub-budget (affinity-token §3 fixes
    the 64)". The expectation column is byte-for-byte untouched, so the `72 > 64` arithmetic and the
    error content the future test asserts against are unchanged.
- **The number did not move.** `grep -c '72 > 64'` still returns 1, and the gate's `vectors:` line
  reads `125/533 rows proved, 19 covered for shape only, 389 deferred with a reason` both before and
  after — identical, so no row changed coverage class.

### Item 4 — decided: **yes, `G5` gets a matching forward-compatibility sentence**

`affinity-token` §3 deliberately keeps a door open — "values above 64 are invalid by rule, not by
width — the width leaves room if a future version renegotiates the sub-budget", because `facts len` is
a `u8`. `G5` is now written to keep the *same* door open, in the only terms `G5` is entitled to use:

> This rule owns the summation, never the bound: if a future token version renegotiates the
> sub-budget, §3 changes and this rule follows it unmodified — the value is read from the authority,
> not restated here.

The reasoning, and why the alternative was rejected:

- `G5` is a **summation** rule. What it asserts is `Σ TokenFact.max_bytes ≤ B`; it is value-agnostic
  by construction, and every part of it stays true for any `B`. Saying so costs nothing today and is
  simply an accurate description of the rule as written.
- The asymmetry *is* the defect class. If §3 says the budget may be renegotiated and `G5` embeds 64
  while saying nothing about renegotiation, then a future renegotiation is a two-spec edit with no
  pointer between them — the reader of `G5` has no way to know a second document governs the value.
  That is the exact shape of the bug this story is fixing, one renegotiation later.
- Making the follow-the-authority relationship explicit means a renegotiation touches **one** spec.
  §3 changes; `§5` class (b), `G5` and the affinity-token cross-reference in §3 all remain true
  unedited. Only `HF-7`, a vector that must state concrete arithmetic to be testable, would need
  re-deriving — and a vector row is the right place for that cost to land, because a vector is
  supposed to pin a specific case.
- Rejected alternative: leave `G5` silent on renegotiation and simply drop the word "provisional".
  That satisfies the grep and reads as settled, but it is settled in a way §3 contradicts — it closes
  a door §3 holds open. "Not provisional" and "not renegotiable" are different claims, and conflating
  them would have replaced a stale-modality bug with a live-contradiction bug.

No edit to `docs/specs/affinity-token.md` was needed or made: §3 already states the budget normatively,
already names itself the authority, and already carries the forward-compatibility clause. The fix was
entirely a matter of the *dependent* spec deferring to it.

- `docs/reference/vector-scope.toml`'s `HF-7` deferral was checked per the Notes and needs **no**
  correction — `CF-16` already rewrote its reason to "The budget itself is already normative in
  affinity-token §3; what is missing is the graph validation that enforces it," which is exactly the
  modality this story establishes in the spec. (It is also another implementor's file this wave.)

## Notes

- Found by [`CF-16`](CF-16-sweep-for-done-stories-that-left-named-deltas-unlanded.md)'s sweep. It is
  the cleanest instance of the class: the delta was named in writing, attributed to a specific story,
  correctly diagnosed by a third story, recorded as adjacent-not-mine, and then owned by nobody —
  exactly `EX-8`/`EX-12`'s shape, one document further along.
- Small diff, and deliberately its own story rather than a drive-by: `G5` and `HF-7` are normative
  rows in a spec the gate reads, and `HF-7` is a registered vector row, so the edit belongs somewhere
  that runs `check-vectors.py` and looks at the result.
- `HF` rows are all deferred to `EX-3` in `vector-scope.toml`, so `HF-7` has no test to break yet.
  The deferral's reason text mentions the budget; check whether it needs the same correction.
- Considered for upstream: no. The hook framework's module-fact budget is this platform's own
  layout; the kernel has no hooks and no affinity token.
