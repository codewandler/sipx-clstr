---
id: CF-16
title: Sweep for done stories that closed with named deltas unlanded
pillar: Foundation
status: ready
priority: 2
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

- [ ] Sweep every `status: done` story for deltas named by another document — a design, a spec, or
      another story — that never landed. `EX-8` is the known instance and it is already closed; the
      question is how many others there are.
- [ ] **Report the count as a number, before fixing anything.** If it is zero, say so and close this
      as a one-off; that is a genuinely useful answer. If it is not, each one gets a story or a
      correction, and the number goes in the ledger.
- [ ] The mechanism is closed, or explicitly judged not worth closing. A candidate: when a design or
      spec says "`X-9` does this", something checks that `X-9`'s acceptance actually mentions it.
      That is a real check but it needs a convention for naming an owner; a cheaper one is to fail
      when a `done` story is named as owner by text that still describes the work as pending.
      Choose deliberately and record why — a check nobody can satisfy will be deleted.
- [ ] `docs/stories/EX-7-*.md`'s frontmatter is reconciled: it is `status: done` while its acceptance
      references deltas that only landed with `EX-12`. Either the status or the acceptance is wrong.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

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
