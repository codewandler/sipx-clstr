---
id: RG-26
title: A removals-only REGISTER must not trip the binding quota
pillar: Registrar
status: ready
priority: 3
design:
epic:
areas: [registrar]
note: found by RG-16 round 4 — with 12 bindings held against a quota of 10, a REGISTER that only removes one is refused 403, while location-service §5.5's prose says removals never trip the quota
---

# A removals-only REGISTER must not trip the binding quota

## Goal

Make the quota check honest about direction: a REGISTER whose every contact operation removes or
shrinks the binding set cannot push the set over `maxBindingsPerAor`, so refusing it as
over-quota contradicts [location-service](../specs/location-service.md) §5.5's own prose and
strands an over-quota AoR — the one state where the user most needs removals to work.

## Acceptance

- [ ] The spec settles the rule as a numbered row first: whether the quota is judged on the
      **post-reconcile** set size (the direction §5.5's prose implies) or per-operation with
      removals exempt — and what a mixed add+remove request does. The row cites §5.5 and the
      B-rules RG-16 renumbered (LS-R-26…35), and registers in the vector table.
- [ ] Failing-first vector: an AoR holding quota+2 bindings receives a REGISTER that only removes
      one; at the current tree it is refused `403`, after the change it is `200` and the response
      enumerates the remaining set (§5.6). Both conformance suites (in-memory and PostgreSQL) run
      it.
- [ ] The refusal that remains — a request whose post-state would exceed the quota — still
      carries its existing status and reason unchanged.

## Progress

- Filed at RG-16 round 4's integration; the implementor found the over-refusal pre-existing at
  both its base and tip and deliberately did not widen its diff to fix it.

## Notes

- RG-25's `max_contact_ops` cost bound is a different axis (work per request, not set size) and
  is untouched by this story.
