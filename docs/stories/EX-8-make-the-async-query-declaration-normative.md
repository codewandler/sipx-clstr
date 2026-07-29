---
id: EX-8
title: Make the async query declaration normative in the hook-framework spec
pillar: Platform
status: done
priority: 1
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions, hooks]
note: filed by EX-6 — the design is accepted, the spec does not yet say it
---

# Make the async query declaration normative in the hook-framework spec

## Goal
Carry `EX-6`'s accepted design into [hook-framework](../specs/hook-framework.md) as normative text
with vectors, so the async external routing hook is a contract `EX-3` can implement against rather
than a design record it has to interpret.

## Acceptance
- [x] **§4** — `QueryOutcome` and `Disposition` are closed enums; E4 gains the two rules the design
      states: `Query` is the last effect of the invocation that emits it, and the outcome is decided
      exactly once (generation counter, late answers discarded at the input boundary).
- [x] **§4** — `QueryDeadline` is stated as an **engine-owned** timer class, adjacent to E6 and
      explicitly not a module timer, with the reason: E6 forbids a module timer from altering a
      transaction's outcome, and this one does.
- [x] **§6** — `QueryDecl` gains `timeout`, `retries`, `on`, `defaults`, `cache`, `limits`; the
      `external-route` module joins the §9 manifest cast.
- [x] **§7** — G7 (the outcome map is total; per-query and per-request budgets hold) and G8
      (dispositions resolve; the closed `Reject` status set `{403, 404, 480, 500, 503}`; 6xx is
      forbidden) are normative rules with the same "fails deployment, never a call" discipline as
      G1–G6.
- [x] **§8** — `hook_budget` is a profile value, so `EX-5`'s catalog carries it.
- [x] **§9** — vectors `HF-9` … `HF-13` cover: a missing outcome arm, a budget exceeded, an
      unresolvable default, a 6xx disposition, and an out-of-closed-world default.
- [x] The new rows are wired into the vector registry — `scripts/check-vectors.py` and
      `docs/reference/vector-scope.toml` — so `scripts/check-vectors.py --check` accounts for every
      one of them, covered or deferred with a reason.

## Progress
- **Filed 2026-07-29 at `EX-6`'s integration.** `EX-6` deliberately did not edit
  `docs/specs/hook-framework.md`, because `EX-1` closed it; the design's *What this hands to the
  spec* subsection enumerates the deltas so they are not rediscovered. This story is that hand-off,
  made explicit rather than left in a handoff note.
- The vector-registry wiring is called out separately in Acceptance because it was **fenced** during
  `EX-6`'s implementation wave — another story owned `scripts/check-vectors.py`,
  `docs/reference/vector-scope.toml` and `docs/reference/conformance.md` at the time, so `EX-6`
  could not do it even where it wanted to.
- **Landed 2026-07-29.** Where it went: §4 carries the two closed enums plus **E4a** (`Query` is
  last) and **E4b** (decided exactly once); **E7** states the engine-owned `QueryDeadline` next to
  E6; **§6.1** is the declaration with rules Q1–Q6 and a complete `external-route` manifest;
  **G7**/**G8** join §7; `hook_budget` is a profile value in §8; `HF-9` … `HF-13` join §9. The
  registry now knows the `HF` prefix and all thirteen rows are deferred to `EX-3`.
- **Five ambiguities the design left, decided here** rather than reproduced as normative
  vagueness. Each was resolved in the fail-closed direction, and the design's *What this hands to
  the spec* records them too:
  1. **`Disposition::Apply`.** The design requires `on` to be total over seven outcomes but gives
     `Answered` no disposition ("— the decision applies"). A fourth variant `Apply` makes the map
     total; G7 binds it to the `Answered` arm and to no other, so it can never mean "continue
     without an answer".
  2. **H11 is in the budget sum.** The design enumerated H3+H5+H7+H9. H11 also permits `query`
     and its deadline delays the response the caller is waiting for, so it is a term. Adding
     terms only makes the startup check stricter.
  3. **`Reject` is only declarable where the phase permits it.** H9 and H11 permit `query` but not
     `reject` (§3 table), so a `Reject` arm there would be an undeclarable effect. G8 makes that a
     startup error, and requires those queries to declare `Proceed(_)`/`ProceedWithout` — a boot
     failure rather than a runtime fallback.
  4. **`ClientError` is never cached.** The design listed four never-cached outcomes and omitted
     it. Only `Answered` and `Declined` are cacheable; the rule is now total over the enum.
  5. **`Scratch` gained `optional`.** G8 gates `ProceedWithout` on "the consuming `Scratch` is
     declared optional", a property §5's class (a) declaration did not carry. It does now,
     defaulting to `false`.
- **Vector arithmetic.** Before: 77/85 proved, 8 deferred (`HF` unknown to the gate). After:
  77/98 proved, 21 deferred — the five new rows plus `HF-1` … `HF-8`, which registering the prefix
  necessarily brought under the gate. All thirteen are startup-validation rows and none can run
  before a hook runtime exists, so all are deferred to `EX-3` with a per-row reason.
- **The `HF` grammar decision, which `CF-8` asks for.** `scripts/check-vectors.py` widened its row
  grammar to accept the two-part `HF-9` shape rather than renumbering `hook-framework` into
  families: a row id is a citation, `HF-1` … `HF-8` are quoted from other specs, designs and
  stories, and renumbering to satisfy a regex breaks all of them to buy nothing a reader can see.
  The reasoning is recorded in the script's own docstring. `CF-8`'s remaining registrations —
  `LS`, `MR`, and `AT`/`FR`, which is the other half of the two-part shape — are untouched here.

## Notes
- Source: [`EX-6`](EX-6-design-an-async-external-routing-hook.md), section *What this hands to the
  spec* in [extension-framework](../designs/extension-framework.md).
- Consumer: [`EX-3`](EX-3-implement-the-hook-runtime.md) implements the hook runtime; this spec is
  what it implements. Ordering: this story before `EX-3`'s query handling.
- `EX-5` owns the profile catalog that `hook_budget` joins.
- Adjacent, found by `EX-6` and **not** part of this story:
  `docs/specs/hook-framework.md:187` and `:287` still call the 64-byte module-fact sub-budget a
  "placeholder until AF-1 fixes the layout". `AF-1` has landed —
  [affinity-token](../specs/affinity-token.md) §3 now states the 64-byte sub-budget normatively and
  names itself the budget authority, with §5 carrying the arithmetic — so the provisional wording is
  stale.
