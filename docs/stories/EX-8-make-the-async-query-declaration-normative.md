---
id: EX-8
title: Make the async query declaration normative in the hook-framework spec
pillar: Platform
status: ready
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
- [ ] **§4** — `QueryOutcome` and `Disposition` are closed enums; E4 gains the two rules the design
      states: `Query` is the last effect of the invocation that emits it, and the outcome is decided
      exactly once (generation counter, late answers discarded at the input boundary).
- [ ] **§4** — `QueryDeadline` is stated as an **engine-owned** timer class, adjacent to E6 and
      explicitly not a module timer, with the reason: E6 forbids a module timer from altering a
      transaction's outcome, and this one does.
- [ ] **§6** — `QueryDecl` gains `timeout`, `retries`, `on`, `defaults`, `cache`, `limits`; the
      `external-route` module joins the §9 manifest cast.
- [ ] **§7** — G7 (the outcome map is total; per-query and per-request budgets hold) and G8
      (dispositions resolve; the closed `Reject` status set `{403, 404, 480, 500, 503}`; 6xx is
      forbidden) are normative rules with the same "fails deployment, never a call" discipline as
      G1–G6.
- [ ] **§8** — `hook_budget` is a profile value, so `EX-5`'s catalog carries it.
- [ ] **§9** — vectors `HF-9` … `HF-13` cover: a missing outcome arm, a budget exceeded, an
      unresolvable default, a 6xx disposition, and an out-of-closed-world default.
- [ ] The new rows are wired into the vector registry — `scripts/check-vectors.py` and
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
