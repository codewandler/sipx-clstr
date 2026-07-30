---
id: CF-16
title: Sweep for done stories that closed with named deltas unlanded
pillar: Foundation
status: done
epic: conformance-harness
areas: [docs, ci]
note: EX-8 was named for two deltas, closed having landed one, and nothing noticed for months
---

# Sweep for done stories that closed with named deltas unlanded

## Goal

`EX-8` was named — by another document, in writing — as the owner of two changes: `EX-6`'s half and
the `SyntaxDecl` `Replace`/`Field` split with rules `G9`–`G14`. Its acceptance covered the first
only. It closed `done`. The second half sat orphaned until `EX-11` tripped over it and `EX-12` landed
it, and in between a real capability was missing: a deployment could not both anchor media and run an
SDP quirk.

Nothing on the board notices this. A story's acceptance is checked against itself, so a story that
closes having satisfied *its own* acceptance is `done` even when another document named it as the
owner of work it never did. The board cannot see the difference between "finished" and "finished the
part it wrote down".

This is the same family as the conformance findings this epic keeps turning up — `CF-8`, `DP-12`,
`PX-10`, `CF-12` — but one level up: not a claim unproved, a **claim of ownership unhonoured**.

## Acceptance

- [x] Sweep every `status: done` story for deltas named by another document — a design, a spec, or
      another story — that never landed. `EX-8` is the known instance and it is already closed; the
      question is how many others there are.
      → Swept all **81** `done` stories against the whole document corpus. **The count is 3**:
      `CF-8`, `AF-1`, `CX-1`. Method, evidence and the adjudication of every near-miss below.
- [x] **Report the count as a number, before fixing anything.** If it is zero, say so and close this
      as a one-off; that is a genuinely useful answer. If it is not, each one gets a story or a
      correction, and the number goes in the ledger.
      → **3**, produced before any fix. Each has a story:
      [`CF-17`](CF-17-the-conformance-report-omits-three-quarters-of-the-rows-it-counts.md),
      [`EX-13`](EX-13-hook-framework-still-calls-the-settled-token-budget-provisional.md),
      [`CX-6`](CX-6-file-the-three-ledger-rows-cx-1-was-named-for-and-never-filed.md). The
      `CHANGELOG.md` line is the coordinator's — that file is fenced during this wave.
- [x] The mechanism is closed, or explicitly judged not worth closing. A candidate: when a design or
      spec says "`X-9` does this", something checks that `X-9`'s acceptance actually mentions it.
      That is a real check but it needs a convention for naming an owner; a cheaper one is to fail
      when a `done` story is named as owner by text that still describes the work as pending.
      Choose deliberately and record why — a check nobody can satisfy will be deleted.
      → **Judged not worth closing, on measured precision.** Reasoning below. The cheaper variant the
      item proposes was built and measured too, and it is the one that fails: see *Why not a checker*.
- [x] `docs/stories/EX-7-*.md`'s frontmatter is reconciled: it is `status: done` while its acceptance
      references deltas that only landed with `EX-12`. Either the status or the acceptance is wrong.
      → Verified reconciled, by `EX-12`, before this story ran; **no edit needed**. The acceptance was
      the wrong half and `EX-12` rewrote it. Evidence below.
- [x] `scripts/gate.sh` green.

## Progress

### The count: 3

Swept **81** `status: done` stories (frontmatter-parsed) against every `.md` and `.toml` under `docs/`,
`website/docs/`, `README.md` and `AGENTS.md`. The class swept for is the narrow one the Notes require:
*another* document asserting that a **specific story** will do a **specific thing**, where the story is
`done` and the thing never landed.

| # | Owner | Named by | The delta that never landed | Filed as |
|---|---|---|---|---|
| 1 | `CF-8` | [asserted-identity](../specs/asserted-identity.md) §13, [cluster-config](../specs/cluster-config.md) §12, [number-normalisation](../specs/number-normalisation.md) §11 | `docs/reference/conformance.md` never got the registered families. 7 prefixes are in `SPECS`, 0 of their 30 families in `FAMILIES`, and `render()` iterates `FAMILIES` — so **395 of the 533 rows the report counts appear in no table**. `CF-8` ticked both "with their families named in `FAMILIES`" and "conformance.md regenerates to include the new families". | [`CF-17`](CF-17-the-conformance-report-omits-three-quarters-of-the-rows-it-counts.md) |
| 2 | `AF-1` | [hook-framework](../specs/hook-framework.md) §5 class (b), `G5`, vector `HF-7` | `AF-1` landed and [affinity-token](../specs/affinity-token.md) §3 made the 64-byte module-fact sub-budget normative ("This spec is the budget authority"). Three `hook-framework` rows still call it a "placeholder until AF-1 fixes the layout" / "provisional 64 bytes until AF-1". `EX-8` diagnosed this in writing and recorded it as adjacent-not-mine; nobody owned it after. | [`EX-13`](EX-13-hook-framework-still-calls-the-settled-token-budget-provisional.md) |
| 3 | `CX-1` | [asserted-identity](../specs/asserted-identity.md) §2, [number-normalisation](../specs/number-normalisation.md) §1, [proxy-behavior](../specs/proxy-behavior.md) §1 | Three gaps "for `CX-1` to file against sipx" — the RFC 3966 `tel:` splitter (twice, once needed in public API) and the RFC 5393 branch-cookie computation. [`upstream.md`](../upstream.md) has **no row for any of them**: zero hits for `3966`, `5393`, `branch.cookie` across all 16 rows. `number-normalisation` records `RT-2` as *blocked* on the splitter. | [`CX-6`](CX-6-file-the-three-ledger-rows-cx-1-was-named-for-and-never-filed.md) |

Reproducing the three, on this tree:

```
$ python3 -c "import importlib.util,sys,collections; spec=importlib.util.spec_from_file_location('cv','scripts/check-vectors.py'); cv=importlib.util.module_from_spec(spec); sys.modules['cv']=cv; spec.loader.exec_module(cv); rows=cv.spec_rows(); out=[r for r in rows if cv.family_of(r) not in cv.FAMILIES]; print(len(out),'of',len(rows),'rows render in no section')"
395 of 533 rows render in no section

$ grep -cE 'provisional|placeholder' docs/specs/hook-framework.md
4                       # H11's "provisional or final" is RFC wording; the other three are the budget

$ grep -cE '3966|5393|branch.cookie' docs/upstream.md
0
```

### Why not a checker — the mechanism decision

Both variants the Acceptance offers were built and measured before being rejected.

The **cheaper** variant — "fail when a `done` story is named as owner by text that still describes the
work as pending" — is the one that decides it. Implemented over 16 delegation constructions
(`handed to`, `deferred to`, `left to`, `belongs to`, `owned by`, `filed as`, `X's to …`, `until X`,
`for X to …`, `X decides|covers|tracks|holds`, …) it finds **159 delegation-shaped mentions, 100 of
them naming a `done` story**. Three are real. That is **3% precision, 97 false alarms**, and the
false alarms are not fixable by better patterns, because the distinguishing feature is tense and
mood, not vocabulary:

- `"until EX-12 landed the split"` — resolved, past tense, correct as written.
- `"Until CF-8, this table is unenforced"` — open, and one of the three.

Both match `until\s+X`. Separating them is reading comprehension. A gate at 3% precision is a gate
whose output is ignored by the second week and deleted by the fourth — which is precisely what this
story's own Notes predicted ("a check nobody can satisfy will be deleted"), and the measurement is
the evidence for taking that warning seriously rather than as a caution to be brave about.

The **stronger** variant — "check that `X-9`'s acceptance actually mentions the thing" — is worse, not
better. It requires a machine-readable convention for naming an owner across specs, designs and
stories, and then requires that acceptance text be matchable against spec prose. Instance 2 shows why
it would still miss: `hook-framework` names `AF-1` for a *value*, `AF-1`'s acceptance is about the
token layout, and the two are the same obligation described in different words. The check would fire
on it either way — as a false alarm if `AF-1` had reconciled the wording, as a true one if not — and it
cannot tell which without reading both.

**So: no checker for this class.** The three instances are fixed by the three stories above, and the
class is left to review. The one structural change that would actually prevent recurrence costs
nothing to state and is recorded in `CX-6`: **a spec deferring work should name a ledger row or an
epic, not a story**, because a story closes and a row does not. That is a convention worth adopting
the next time a spec defers, not a gate.

A **different and much cheaper** property surfaced during the sweep and is worth a gate — every `done`
story leaves the two records `AGENTS.md` already requires (a `CHANGELOG.md` citation, and one ticked
acceptance box). Total, no convention to invent, 12 real instances, zero judgment calls. Filed
separately as [`CF-18`](CF-18-a-story-can-read-done-while-its-own-record-says-nothing-landed.md),
and explicitly **not** this story's mechanism: it is closure hygiene, not ownership, and it would not
have caught `EX-8`.

### Near-misses, adjudicated

Recorded so the next sweep does not re-litigate them. All are `done` stories named as an owner, and all
honoured it:

- **`DP-1`** — `ET-1` says "`DP-1`'s schema must enforce" the `echo`-beside-a-proxy-role refusal.
  Honoured: [cluster-config](../specs/cluster-config.md) `R1`/`R4`/`R6`, enforced in
  `crates/sipx-clstr-node/src/config/mod.rs:784`, with vectors `CC-R-2`/`CC-R-3`.
- **`RT-1`** — `proxy-transaction-driver` §116 says `RT-1` decides the resolver question. Honoured, and
  settled *upstream*; [`upstream.md`](../upstream.md) row `T-17` records the decision.
- **`DX-12`** — `DX-13` says "`DX-12` owns fixing" the gate never reading a documented command. The
  delta landed, via `CF-15`/`check-site.py` rather than `DX-12`. `DX-13`'s bullet is now stale
  narration in a Progress log; harmless, not filed.
- **`PX-1`** — its "reviewed against `AF-1`" box is ticked and its Progress explains that `AF-1` closed
  it (157 B ≤ 200 B). The older "Open:" bullet below is a historical log entry, correct as history.
- **`RG-2`** — `affinity-token` `CT7` defers "what happens to that request" to `RG-2`. Genuinely
  undecided, but the consumer (`AF-7`, the flow table) is not `done`, so nothing is missing yet.
- Out of scope by construction: deferrals to `ME-4`, `RT-8`, `RT-2`, `EX-3`, `EX-5`, `PX-4`, `AF-6`,
  `CX-5` — none of those is `done`, so a pending description of pending work is correct.

### `EX-7` (Acceptance item 4) — already reconciled, verified not edited

The item asks which of status or acceptance was wrong. **The acceptance was**, and `EX-12` had already
rewritten it before this story ran — so `status: done` is now truthful and the correct action was to
change nothing. All five boxes are `[x]`; the third names `G10`/`G13` and records that it read
"disjoint union over the composed set" until `EX-11` derived the sets never intersect; the fourth cites
`hook-framework` §8.1/§9.1 and 31 `QP` rows. The Progress records the orphaning and that `EX-12` landed
it in §6/§7 (`G9`–`G14`)/§8.1/§9.1. Confirmed against `git diff e61308e main -- docs/stories/EX-7-*.md`.

### `docs/reference/vector-scope.toml` header

Fixed per the Notes. It called itself "the narrow, `PB`-only ancestor" of the registry; it is in fact
the project's whole deferral ledger — **389 `[[deferred]]` rows over 11 prefixes**, naming 12 stories,
in which `PB` (8 rows) is the third *smallest* family, plus a second **19-row `[[unasserted]]` table**
the old header did not mention at all. True when written, stale from `CF-8`'s six registrations onward.
The "folds into `EX-2`/`CF-2`'s registry" sentence is kept: both are still `backlog`. The file is not
restructured.

The first draft of this header cited per-prefix counts taken from every `id =` line in the file, which
silently summed both tables to 408 and would have put a fresh wrong number where the stale one had
been. Caught by checking the arithmetic against `[[deferred]]` alone.

### Note on the tree this describes

The worktree was created 54 commits behind `main` (`e61308e` vs `62dcc4f`), which the coordinator
identified and instructed be merged before the count was taken. It was: `git merge main`, a
fast-forward. **The count above was produced from scratch on the merged tree and describes `62dcc4f`.**
An earlier partial pass against the stale base was discarded rather than reconciled — 21 files under
`docs/stories/` differ across that range, including `EX-12`, which is the story this one is built on.

## Notes

- Found by `EX-12` while landing the orphaned half, and reported as the *real* defect behind the
  story it was given: "EX-8 closed on half its named scope and orphaned the other half silently;
  nothing on the board notices a story that closes with named deltas unlanded."
- Do not turn this into a requirement that every cross-reference between documents be machine-checked.
  Most references are context, not delegation. The narrow case is a document asserting that a
  *specific story* will do a *specific thing*, which is a promise the board should be able to keep.
- `docs/reference/vector-scope.toml` is now 2700+ lines with 389 deferrals, and its header still
  describes it as "the narrow, `PB`-only ancestor" of the conformance registry. Also stale, also
  nobody's job — same shape, worth fixing while sweeping.
- The cheapest useful version of this story is the sweep alone. If the count comes back small, the
  mechanism may not be worth building, and saying that with evidence is a complete outcome.
