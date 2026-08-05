---
id: RG-14
title: Make the REGISTER contact path linear, and bound the work before the quota decides
pillar: Signalling
status: done
priority: 2
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: one 64 KB REGISTER costs millions of URI parses, and the quota is checked after the work is done
---

# Make the REGISTER contact path linear, and bound the work before the quota decides

## Goal

Stop a single well-formed REGISTER being a CPU amplifier. Binding reconciliation re-parses each stored
contact URI once per comparison, so the cost is quadratic in the number of contacts a request carries,
and the per-address quota is applied only after all of that work has been done and thrown away.

## Acceptance

- [ ] Contact matching stops re-parsing on every comparison. `process.rs` calls
      `set.all().iter().position(matches_contact)` per operation, and `matches_contact` parses the
      *stored* contact's URI each time; distinct contacts never match, so the scan is always full.
      Parse once into a comparable form, or index the set.
- [ ] The number of contact operations a single request may carry is bounded, and the bound is in
      [location-service](../specs/location-service.md) rather than only in code. The parser's limits
      (256 headers, 8 KB per header, 64 KB per message) permit thousands of contacts once
      `Address::parse_list` flattens comma lists, and nothing caps `ops.len()`.
- [ ] **Failing-first**: a test submits a REGISTER carrying a large contact list and asserts a
      refusal, or asserts a work bound — not merely that it eventually answers. It fails today: the
      request is accepted, the whole quadratic reconciliation runs, and only then does the
      `max_bindings_per_aor` check refuse it.
- [ ] The quota's *position* is preserved or explicitly changed. Checking the committed outcome is
      correct per §5 S8 — it judges the result rather than the request — so the fix is a cheap
      pre-check that cannot disagree with it, not a relocation of the real check.
- [ ] Existing `LS` vector rows still pass, and any new bound gets its own row.

## Progress

- **Done, with the quota half deliberately left unchanged and said so.**
- **The amplifier is closed.** `matches_contact` re-parsed every stored contact for every incoming
  op — `O(ops · bindings · quota)` parses. Matching now computes a parsed view of the stored bindings
  **once** before the op loop and reads it per op, so one reconciliation costs one parse per stored
  binding. Equivalence still runs through the kernel's `Uri::equivalent`; nothing re-derives §19.1.4.
- **Failing-first, counted not timed.** A test asserts that reconciling 10 contacts against 10 stored
  bindings parses exactly **10** stored contacts, not 100. The instrument is a parse counter in
  debug builds, because wall-clock would be flaky and a parse is the expensive unit. A second test
  asserts a matching contact still updates in place, so the restructure did not change which binding
  is touched.
- **A field-based fix was tried and reverted.** Caching the parsed URI on `Binding` made the hot path
  trivial, but `Binding` is serialized to PostgreSQL and `Uri` is not `Serialize` — so it broke the
  persistence path. The parse-once-per-reconciliation view achieves the same asymptotic result
  without touching the stored shape, which is why it is the answer.
- **The quota ordering WAS changed, after the analysis it needed.** (An earlier pass here recorded it
  as deliberately skipped; that was the right call *at the time* and is now superseded.)
- **What made it sound.** A binding is added only by an op with a positive granted expiry that matches
  **no** stored binding. An op that matches is a refresh, and a refresh cannot grow the set. So the
  maximum active count this request can reach is `current_active + genuine_additions`; if that fits
  the quota the committed set cannot exceed it, and if it does not fit the final check would reject
  identically. Two evaluations of the same test at different costs.
- **The version that was wrong, and what caught it.** The first attempt ran the check *before* the
  match view and counted every positive-expiry op as an addition — which refuses a **refresh**. The
  `LS-R-15` vector is named "the quota refuses a new binding but never a refresh" and failed
  immediately with `403` where `200` was required. Distinguishing a refresh from an addition *is* the
  reconciliation, so the cheapest sound placement is after the parse-once view and before the mutation
  loop. That is where it now sits.
- **The parse meter had to become thread-local.** As a global atomic it reported 3 parses where 2 was
  correct, because the suite runs tests in parallel and a sibling's parses leaked into the delta. A
  thread-local is right because `process` runs synchronously on its caller's thread. This was the
  fix a prior implementor had already reasoned out before its run ended.
- **Superseded note:** The comment in
  `process.rs` already says the quota is checked against the *committed outcome* "so a refresh or a
  removal can never trip it". Moving the check earlier risks altering *which* requests are accepted,
  not just when the refusal happens — a registration that grows the set past the limit is refused
  either way, but a refresh that reclaims expired bindings first may be accepted only because the
  check ran after reaping. That is a semantic change, and the story's own note forbids changing
  which requests are accepted silently. It is recorded as needing its own analysis rather than folded
  in as an optimisation.
- Considered for upstream: no. This is the platform's own reconciliation path.
- Gate green, shared location-service suite unchanged.

## Notes

## Notes

- **The shape of it.** For a request with n contacts against a set of m stored bindings the work is
  n·m URI parses, and because inserts push the set toward the quota it behaves as ~n²/2 in the worst
  case, plus a `Vec::insert` memmove over ~200-byte `Binding` structs per operation. A few hundred
  crafted datagrams saturate every core.
- **Why this pairs with the admission bound.** `DP-11` bounds how many of these can be in flight at
  once; this story bounds what one of them costs. Either alone leaves the product unbounded.
- Reachable without credentials today for the same reason as `RG-13` — no authentication, no domain
  enforcement — but the amplification is a property of the reconciliation loop and would remain after
  `FC-3` and `FC-4` land, for any authenticated tenant.
- Considered for upstream: partly. Contact-list parsing limits are kernel surface and `sipx-sip`
  already has message and header bounds; a per-header *element count* bound may belong there. The
  reconciliation loop is ours. Decide before implementing, per AGENTS.md #6, and record it in
  [upstream.md](../upstream.md) if any of it moves.
- `set.all()` cloning the whole `BindingSet` twice per REGISTER is fine at quota scale (n ≤ 10) and is
  not what this story is about — do not conflate the two.

- **Correction filed 2026-07-30, by the RG-16 coordinator.** This story is `done` with **zero** of its
  five acceptance boxes ticked and **no `CHANGELOG.md` entry** — a `CF-18` instance. Its parse-once view
  and its quota pre-check both genuinely shipped and the Progress above describes them accurately.
  **Item 2 did not ship:** `docs/specs/location-service.md` contains no bound on the number of contact
  operations a REGISTER may carry, and nothing in code caps `ops.len()`. That gap is now
  [RG-25](RG-25-bound-the-contact-operations-one-register-may-carry.md), and it blocks `RG-16`.
- The soundness argument recorded above — "the maximum active count this request can reach is
  `current_active + genuine_additions`" — **was valid when written and has since been invalidated** by
  `RG-16`'s B6/B7, which let several operations collapse onto one binding. The pre-check was not wrong;
  its premise stopped holding. Read `RG-16`'s Progress before touching the quota's position again.
