---
id: RG-25
title: Bound the contact operations one REGISTER may carry, in the spec and not only in code
pillar: Registrar
status: ready
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
- [ ] `location-service` states the bound as a normative rule with an ID, not only as a constant in
      code, and states what a request exceeding it receives.
- [ ] The refusal happens **before** reconciliation — at parse or immediately after — so an
      over-limit request costs O(1) work, not a full reconciliation.
- [ ] **Failing-first, measured rather than timed:** a REGISTER carrying a contact list above the
      bound is refused, and the assertion is a work bound or an operation count, not "it eventually
      answers". `parse_meter` cannot see this class — it counts stored-contact parses at
      `Reconciling::new` only — so a new instrument is needed.
- [ ] The bound gets its own vector row, registered with its test name in the same commit.
- [ ] Both the in-memory and PostgreSQL conformance suites cover it (the shared suite in
      `crates/sipx-clstr-registrar/src/conformance.rs`).
- [ ] `RG-16` can then measure the quota on the committed outcome alone, per §5.5, without
      reintroducing a pre-check — record that consequence here so the two stories cannot drift apart
      again.

## Progress
- (not started)

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
