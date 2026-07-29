---
id: RG-14
title: Make the REGISTER contact path linear, and bound the work before the quota decides
pillar: Signalling
status: ready
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

- (running log)

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
