---
id: RG-16
title: Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once
pillar: Registrar
status: in-progress
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar]
note: release blocker — a REGISTER carrying several contacts can commit the wrong binding set
---

# Reconcile a multi-contact REGISTER against fresh indices, not a snapshot taken once

## Goal

Make `REGISTER` processing commit the binding set the request actually describes when the request
carries more than one contact.

## Acceptance

- [x] Binding operations use a stable identity or a parsed view updated with every mutation; no
      operation applies an index computed against a different vector state.
- [x] An insertion becomes visible to later operations in the same REGISTER, and a removal cannot
      shift a later match onto another binding or off the end of the vector.
- [x] The compare-and-swap contract in [location-service](../specs/location-service.md) §6 still holds:
      a losing writer retries against fresh state rather than committing a stale set.
- [x] **Failing-first vector:** from `{A, B}`, one REGISTER with `A;expires=0, B;expires=0` commits the
      empty set. On `86e6b10` it returns `Commit` with B still present because `get(1)` follows A's
      removal.
- [x] Order-sensitive vectors also cover remove/refresh, refresh/remove, and two operations naming a
      contact first inserted by the same request; every response enumerates the actual final set.
- [x] The new normative `LS-R-*` rows and their test names are registered together.
- [x] Both the in-memory and PostgreSQL conformance suites pass unchanged, and `scripts/gate.sh` is
      green.

## Progress

- **Done.** The original mechanism was confirmed before it was fixed: `process::explicit` parsed the
  stored contacts into a `Vec<Option<Uri>>` once, matched every operation by `position` in *that*
  vector, and then used the position as an index into a `BindingSet` it was concurrently mutating.
  The two diverged at the first removal.
- Three original failure modes, all reproduced at `2cb22dd` and now pinned: a binding survives its
  own removal (the stale index resolves past the end of the shortened set); a refresh lands on
  whichever binding slid into the index, overwriting a contact the request never named and
  duplicating one it did; and on the CAS-retry path, where the re-read set is longer, a removal
  removes the *wrong* binding — the winner's fresh registration.
- The fix is structural rather than a re-parse. A `Reconciling` view owns the set and one aligned
  `Vec<Slot>` describing it, and is the only thing that mutates either, so they cannot drift.
  `BindingSet::insert_at` reports the index an insert landed at, which is what an aligned view needs
  since creation order means an insert is not an append; `insert` delegates to it.
- **Rework — the fix exposed a second, reachable defect and it needed the spec, not a patch.** Once
  operations really are resolved against the live set, a later operation can match a binding an
  earlier operation of the *same* request just wrote — two §19.1.4-equivalent contacts in one
  REGISTER is all it takes, and `CC;expires=3600, CC;line=7;expires=0` is an ordinary thing for a UA
  to send. That binding carries this request's own `Call-ID`/`CSeq`, so `process.rs` read B4/B5
  against this command's own token and aborted the whole request `500`: at `2cb22dd` the request was
  a `200`, on the first cut of this branch it was a `500` with the UA left unregistered and every
  retry failing identically.
- Settled in the spec first (AGENTS.md #4): location-service §5.3.2 gains **B7** — B2–B5 compare
  against the token of the request that *last wrote* the matched binding, and a binding this request
  wrote has no ordering question left for them to decide, so the operation applies. B6's own text
  already said as much ("present for them, with the value it was given"). The distinction is **who
  wrote it, not what token it carries**: a retransmission is token-identical and must stay B4's, so
  the fact is recorded per slot at the moment of the write rather than inferred.
- Spec: §5.3.2 gains B7 beside B6, with the reasoning for why the two cases cannot be told apart by
  comparison, why atomicity (§6 K2) and the CAS loop are untouched, and why B7 widens nothing.
- Vectors: `LS-R-24` … `LS-R-29`, all six **proved** in `docs/reference/conformance.md`, none
  deferred — two removals and an addition, remove/refresh, the empty-set case, refresh/remove, and
  both B7 shapes (a removal of what this request added, and a second operation replacing it).
- Both halves of Acceptance's last item are real: the six rows were added to
  `registrar/src/conformance.rs`, the **shared two-backend** suite, so they run against the in-memory
  and the PostgreSQL backend rather than only the in-memory vector file. Dropping the B7 guard was
  confirmed to fail `LS-R-28`/`LS-R-29` on *both* backends, so the rows are live, not decorative.
- `RG-14`'s parse budget is unchanged and still asserted: one parse per stored binding for the whole
  reconciliation, none per operation. B7's per-slot flag and the quota check's duplicate-addition
  comparison both work on already-parsed URIs.
- The S8 quota pre-check needed one adjustment B7 forced: two operations naming one contact are one
  addition, so counting both would refuse at the quota boundary a request the mutation loop then
  commits within it. Candidates are compared against the additions already *counted* and never
  against skipped ones, because §19.1.4 equivalence is non-transitive.

## Notes

- Validated synthesis finding [**V-05**](../reviews/00-validated-synthesis.md#v-05--multi-contact-register-reconciliation-uses-stale-indices), reproduced by the protocol reviewer in an isolated executable.
- The defect is index invalidation: the original binding vector is parsed once, every operation is
  looked up against that snapshot, and an operation that removes or reorders bindings leaves the
  later operations pointing at the wrong entries.
- Single-contact `REGISTER` — which every existing proof and both node proofs use — cannot expose
  this, because there is only ever one operation.
- **Upstream boundary:** no; binding-set reconciliation is this platform's location-service state.
  URI equivalence remains the kernel primitive and must not be copied locally.
