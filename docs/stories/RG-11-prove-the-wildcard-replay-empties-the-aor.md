---
id: RG-11
title: Prove that a replayed credential empties the AoR through the wildcard path
pillar: Signalling
status: done
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: RA-R-7 is deferred to this story — it is the only unproved row in the RA family
---

# Prove that a replayed credential empties the AoR through the wildcard path

## Goal
Give `RA-R-7` a test. `RG-10` established in
[registrar-auth](../specs/registrar-auth.md) §7.2 that a captured `Authorization`, reattached to a
REGISTER carrying `Contact: *`, `Expires: 0` and a fresh `Call-ID`, authenticates and removes
**every** binding on the AoR — but that pass was documentation-only, so the row ships asserted and
unproved while its additive sibling `RA-R-6` is proved twice.

## Acceptance
- [x] A test drives the two halves together end to end: the replayed-credential admission path and
      [location-service](../specs/location-service.md) §W3's wildcard removal branch. Both halves
      have tests today — `wildcard()`'s ordering tests, and `RA-R-6`'s replayed-credential tests —
      and nothing drives them as one request.
- [x] The assertion is on **what the store holds afterwards**, not only on the status code. The
      point of the row is that the AoR ends up empty; a `200` proves nothing here. `RA-R-6`'s sim
      test is the model — it pins the resulting contact set.
- [x] `RA-R-7` moves from `deferred` to `proved` in
      [vector-scope.toml](../reference/vector-scope.toml) and
      [conformance.md](../reference/conformance.md), regenerated rather than hand-edited.
- [x] The test is failing-first in the meaningful sense: it must fail if the ordering guard were
      ever extended to cover the wildcard path, so it pins today's behaviour rather than restating
      the implementation.

## Progress

**Done.** `RA-R-7` is proved; the `RA` family has no unproved rows left. No source changed — the
behaviour was already there, and this story's job was to pin it.

- **The test** — `ra_r_7_a_reattached_credential_empties_the_aor_through_the_wildcard_path` in
  `crates/sipx-clstr-sim/tests/register_auth.rs`, alongside `RA-R-6`'s. It adds an
  `Encore::Deregister` to the existing phone: after its honest REGISTER is answered, the phone
  reattaches the same `Authorization` byte for byte to a REGISTER carrying `Contact: *`,
  `Expires: 0` and a `Call-ID` the AoR has never seen. One request drives `admit` and `apply`
  together, which is the seam nothing crossed before.
- **What it asserts** — the contact set is empty afterwards, not just that the status was `200`.
  It also pins `store.changes().len() == 2`, because an empty AoR on its own would equally be true
  of a wildcard against an AoR that never held anything (§W3's `Noop` branch); two commits say the
  honest write landed and was then removed. The admissions seam shows both REGISTERs authenticated
  under `t1:alice`, so the audit trail names the replayed principal (§7.2).
- **Failing-first, demonstrated by counterfactual.** The behaviour predates the test, so "run it
  before the fix" would prove nothing. Instead `wildcard()`'s guard in `process.rs` was temporarily
  extended to cover the wildcard path — `binding.call_id != cmd.call_id || cmd.cseq <= binding.cseq`,
  so a fresh `Call-ID` no longer exempts a binding — and the test failed with
  `left: ["sip:alice@10.0.0.9"], right: []`, the edge answering `500`. Reverted immediately;
  `process.rs` is untouched in the diff.
- **Proved once, not twice** — unlike `RA-R-6`. A companion in
  `tests/vectors_register_auth.rs` was considered and left out: `TenantAuth::decide` never reads
  `Contact`, `Expires` or `Call-ID`, so a unit-level RA-R-7 would restate `RA-R-6`'s unit test with
  different field values and would prove nothing about the store — which is the entire claim of the
  row. The distinctive half only exists end to end.
- **Considered for upstream: no.** The kernel already owns both primitives this exercises — digest
  verification and the replay window (`sipx-ua`), and §19.1.4 contact equivalence (`sipx-sip`).
  What is proved here is how *this* platform composes them with its own location-service policy, so
  it is cluster-specific and stays. No entry in [upstream.md](../upstream.md) is owed.

## Notes
- Filed because `RG-10` registered `RA-R-7`'s deferral against this story before it existed, and
  said so rather than leaving the row unaccounted for. That is the right failure mode — a deferral
  naming a real story is the whole mechanism — but it leaves this ID owing a file, which this is.
- The behaviour is deliberate and accepted (§7.3), so this story proves an accepted exposure rather
  than fixing a defect. That is worth stating in the test's own name: someone reading it later
  should not mistake it for a regression test guarding a bug.
- Relevant: `crates/sipx-clstr-registrar/src/process.rs:56-76` — `wildcard()` rejects only when
  `binding.call_id == cmd.call_id && cmd.cseq <= binding.cseq`, which a fresh `Call-ID` makes false
  for every stored binding.
