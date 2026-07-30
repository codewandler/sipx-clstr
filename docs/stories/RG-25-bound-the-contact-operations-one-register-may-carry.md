---
id: RG-25
title: Bound the contact operations one REGISTER may carry, in the spec and not only in code
pillar: Registrar
status: done
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: RG-14 item 2, never landed — one 64 KB datagram costs ~0.2 s of a core, and RG-16 cannot be finished without it
---

# Bound the contact operations one REGISTER may carry, in the spec and not only in code

## Goal
Cap the number of contact operations a single REGISTER may carry, normatively, so the cost of
reconciling one request is bounded before any per-request work is done.

## Acceptance
- [x] `location-service` states the bound as a normative rule with an ID, not only as a constant in
      code, and states what a request exceeding it receives.
- [x] The refusal happens **before** reconciliation — at parse or immediately after — so an
      over-limit request costs O(1) work, not a full reconciliation.
- [x] **Failing-first, measured rather than timed:** a REGISTER carrying a contact list above the
      bound is refused, and the assertion is a work bound or an operation count, not "it eventually
      answers". `parse_meter` cannot see this class — it counts stored-contact parses at
      `Reconciling::new` only — so a new instrument is needed.
- [x] The bound gets its own vector row, registered with its test name in the same commit.
- [x] Both the in-memory and PostgreSQL conformance suites cover it (the shared suite in
      `crates/sipx-clstr-registrar/src/conformance.rs`).
- [x] `RG-16` can then measure the quota on the committed outcome alone, per §5.5, without
      reintroducing a pre-check — record that consequence here so the two stories cannot drift apart
      again.

## Progress

- **Done.** The bound is `location-service` **§5.5.1**, rules `Q1`–`Q5`, anchored into §5.1's step
  order as **S6.1** and proved by `LS-R-24` / `LS-R-25`. Gate green, including the PostgreSQL
  conformance run against a real database.
- **The upstream question, decided before implementing (AGENTS.md #6): the whole of it stays here,
  and no kernel row is filed.** The story's Notes flagged a parser-level element-count cap as
  possibly kernel-shaped. It is — and it would buy nothing this bound does not already buy, so
  filing it would have the [upstream ledger](../upstream.md) claim a dependency that does not exist.
  Flattening a Contact header set is **linear** and the kernel already bounds its own input (64 KB
  per message, 8 KB per header, 256 headers), so the worst a request can spend there is the ~1 ms
  that bounds the whole 64 KB datagram — the story's own measurement column confirms it (3500
  contacts, 1.15 ms, parse included). The amplification is entirely in *our* reconciliation, which
  is quadratic in the request's operation count. There is a second, independent reason the refusal
  cannot live in the parser: §5.7's `BeforeRegistrarUpdate` may **adjust the contact operations**
  after parsing, so a parser-only bound is one a module could walk past. Recorded in the spec's
  §1 "Upstream considerations" as well, where the next reader will look.
- **Where it is enforced, and why exactly there.** `process()`, before the `Wildcard`/`Explicit`
  match — after S2 and §5.6, before S6.1's own step and everything below it. `explicit` is never
  entered, so no stored binding is read, cloned, parsed or compared. Position is the rule, not an
  optimisation: a bound applied after reconciliation refuses the same requests and prevents nothing.
- **Failing-first, and it fails at the merge base on assertions rather than on compilation.**
  `rg25_a_register_above_the_contact_bound_is_refused_before_reconciliation` uses only API that
  exists at `a02cd7c`: at the base it gets `Noop`/`200` and 10 stored-contact parses, at the tip
  `Forbidden`/`403` and 0. The operations it carries are **removals**, deliberately — a removal
  never grows the set, so §5.5's quota cannot refuse the request however long it is. That is the
  whole reason a second bound has to exist, and it is why `RG-14`'s pre-check could never have
  covered this class no matter where it sat.
- **The new instrument is `process::op_meter`, and `parse_meter` really is blind to this class.**
  `parse_meter` counts *stored* contacts, so it measures work proportional to the binding set:
  against an empty address-of-record it reads `0` whether a REGISTER carries one operation or three
  thousand. `op_meter` counts the request's own operations — `0` for an over-limit request, exactly
  its length for a conforming one. `rg25_the_bound_is_counted_in_contact_operations_not_in_stored_parses`
  is the case that separates them: empty set, both meters, and only one of them can see the cost.
  Thread-local for `parse_meter`'s reason. Compiled under `test-suite` as well as `test`, which is
  what lets the **shared** suite assert the *cost* on both backends rather than only the status — a
  backend that answered `403` after reconciling would pass a status-only row.
- **Both suites, and the rows are load-bearing on both.** Verified by mutation rather than asserted:
  neutering the check fails `LS-R-24` on `in-memory` *and* on `postgres` with
  "expected 403, got 200" and "no contact operation is examined; got 65". Postgres was run against
  `postgres:16-alpine` on `127.0.0.1:55432`; all 7 tests green with the check restored.
- **`RG-16`'s consequence, recorded here so the two stories cannot drift apart again.** With §5.5.1
  in place, **`RG-16` must measure the quota on the committed outcome alone (§5.5) and must not
  reintroduce a pre-check.** The triangle is broken from the third side: the pre-check existed only
  to bound work, its soundness premise (`current_active + genuine_additions`) was invalidated by
  `RG-16`'s B6/B7, and there is no sound *lower* bound to replace it with — so the work it was
  buying is now bought by bounding the input instead, where no premise about reconciliation is
  needed. The single post-reconciliation check `RG-16` round 3 arrived at is therefore **correct and
  complete**, and the amplifier it was accused of restoring is closed here: with §5.5.1, one 64 KB
  datagram carries at most 64 operations rather than ~3500, so the quadratic term is bounded by the
  policy rather than by the message size. Round 4 should delete the pre-check without replacement
  and cite this. `RG-25` does **not** remove the pre-check itself — it still holds while `RG-16`'s
  B6/B7 do not exist, and removing it here would leave `main` with neither bound during the window
  between the two stories.
- Considered for upstream: no — see the second bullet, decided before implementing rather than after.

## Notes
- **This is `RG-14`'s Acceptance item 2, and it never landed.** `RG-14` is `status: done` with **zero**
  of its five acceptance boxes ticked and **no `CHANGELOG.md` entry**. Its parse-once view genuinely
  shipped and its Progress describes it well; the contact bound did not, and `docs/specs/location-service.md`
  contains no rule for it. A `done` story whose record does not say what did not land is exactly
  `CF-18`'s defect class — this story is a data point for that sweep.
- **Measured cost of its absence**, release build, 9 bindings held against a quota of 10, N distinct
  new contacts, all answered `403`:

  | N | with `RG-14`'s pre-check | without it |
  |---|---|---|
  | 500 | 151 µs | 4.2 ms |
  | 1000 | 280 µs | 16.1 ms |
  | 2000 | 577 µs | 71.6 ms |
  | 3500 | 1.15 ms | 211.5 ms |

  Linear with the pre-check, **quadratic** without. Roughly 3500 contacts fit inside the 64 KB message
  limit once `Address::parse_list` flattens comma lists, so a single datagram buys ~0.2 s of one core.
- **Why this blocks `RG-16` rather than merely relating to it.** `RG-14`'s pre-check was sound on a
  stated argument: the most active bindings a request can reach is `current_active + genuine_additions`,
  so a request that fits cannot exceed the quota and one that does not would be refused identically by
  the final check. `RG-16`'s B6/B7 **invalidated that premise** by letting several operations collapse
  onto one binding, at which point the pre-check began over-refusing requests the committed outcome
  permits — which `location-service` §5.5 forbids. There is no sound *lower* bound to replace it with,
  and removing it restores the amplifier above. Bounding the input is the way out of that triangle.
- **`REGISTER` is exempt from the node's admission bound** (`DP-11`, deliberately — a registration
  storm *is* the overload and shedding refreshes turns a spike into an outage), so nothing upstream
  limits this either. That exemption is right and it is also why this bound has to exist here.
- Considered for upstream: **partly.** A parser-level cap on how many contacts one header set may
  flatten to is protocol-generic and would be a kernel row; the *policy* bound on operations per
  REGISTER, and what a UA receives when it exceeds it, is this platform's location-service semantics.
  Decide and record which half goes where before implementing.
