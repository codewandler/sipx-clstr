---
id: RG-8
title: Settle B4 idempotency so a retransmission is a retry
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: found by RG-2's harness scenario — an ordinary UDP retransmission is answered 500
---

# Settle B4 idempotency so a retransmission is a retry

## Goal
Decide what [location-service](../specs/location-service.md) §5.3 B4's "same granted expiry base"
means, and make an ordinary retransmitted REGISTER a no-op `200` instead of a `500`. Today every
retransmission that is not delivered within the same nanosecond as the original falls through to B5.

## Acceptance
- [x] §5.3 says which of the two readings is normative, in the spec itself rather than in a story:
      the granted **duration**, or the originating `now` carried with the command. The rejected
      option is recorded with its reason, because the next reader will ask.
      → new [§5.3.1](../specs/location-service.md) rows B4.1/B4.2, with the rejected option and
      its reason under **Rejected: comparing an originating `now`…**
- [x] The `LS-R-3` vector row states the elapsed time between the original and the retry, so
      "identical outcome" stops being satisfiable only by a zero-latency retry.
      → LS-R-3 now reads "**500 ms after** LS-R-2"; `ls_r_3_an_identical_retry_writes_nothing` and
      the `LocationStore` contract suite both delay the retry by that amount.
- [x] `process::already_holds` implements the chosen reading, and a REGISTER retransmitted after a
      delay is a `Noop` with the current set and an unchanged revision.
      → `crates/sipx-clstr-registrar/src/process.rs` `already_holds`, now
      `stored.refreshed_at.until(stored.expires_at) == granted`.
- [x] `sipx-clstr-sim/tests/register_auth.rs` — `ra_r_1_a_retransmitted_register_authenticates_again`
      regains its `phone.answers == [401, 200, 200]` assertion, and
      `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule` is deleted rather
      than inverted: it exists only to pin the defect.
- [x] `a_re_presentation_at_a_later_instant_is_not_a_retry_and_is_refused` in
      `tests/vectors_register.rs` is updated or removed, and the decision it pinned is superseded
      explicitly in [`RG-3`](RG-3-implement-register-processing-on-the-in-memory-store.md)'s open
      question rather than left contradicting the code.
      → rewritten as `ls_r_22_a_re_presentation_asking_for_a_different_duration_is_not_a_retry`,
      the half of the carve-out that survives; `RG-3`'s section is now headed "settled by `RG-8`".
- [x] A binding is never **extended** by a retry. B4's remedy is *no mutation*; a retry that
      refreshed the deadline would make the ordering token spendable more than once.
      → spec rule B4.2; asserted on `expires_at` in `ls_r_3_an_identical_retry_writes_nothing` and
      in the contract suite's `LS-R-3` check.

## Progress
- **Filed 2026-07-29 by `RG-2`.** Not new — `RG-3` recorded it as an open question and deferred it
  to `AF-*`/`RG-5` — but `RG-2`'s harness scenario changed what it is. It was framed as a *cluster*
  concern: a re-presentation at a second node stamps its own `now`, so the expiry base differs and
  B4 does not apply. The scenario reaches it with **one node, one phone and no cluster at all**: the
  phone retransmits its authenticated REGISTER half a second later, which over UDP is what a lost
  `200` produces every day, and the edge answers `500`.
- The cause is one function. `process::already_holds` compares
  `stored.expires_at == cmd.now + granted` — an absolute deadline — so B4 is true only for a retry
  arriving at the very nanosecond of the original.
- `RG-3` already names the two options, and this story is to choose between them:
  1. compare the granted **duration** (`stored.refreshed_at.until(stored.expires_at) == granted`),
     which is a change to this crate alone; or
  2. carry the originating `now` with the re-presented command, which changes `RegisterCommand` and
     whatever re-presents it.
  Option 1 also answers the cluster case, since a second node computing the same duration agrees.
  Option 2 is the more faithful reading of "base" and survives a policy whose granted lifetime
  changes between attempts. **Spec first** either way — §5.3 is normative and the code follows it,
  not the other way round.
- **Decided 2026-07-29: option 1, the granted duration.** Written into the spec first, as
  §5.3.1 — B4.1 makes `refreshed_at → expires_at` the base, B4.2 says the remedy is no mutation.
  Option 2 is rejected in the spec with its reason, and the reason is not a preference: the case
  this story exists for is a **UA retransmission**, which arrives as fresh bytes over the wire and
  carries no field of ours, so the edge stamps its own `now` and the two deliveries differ again —
  option 2 would have left the defect standing while changing `RegisterCommand` and every
  re-presenter. It would also make an ordering decision depend on a value one node accepts from
  another, where a duration is recomputed from local policy and stored state. The policy-drift case
  option 2 was credited with is handled by B4.1 anyway: a changed granted lifetime makes the
  durations differ, so B5 refuses — the conservative answer, and now `LS-R-22`.
- **The carve-out did not widen past its point.** B4 still compares the granted duration, the
  contact-set effect and the Path vector; a spent token asking for anything else is `500`. What
  `already_holds` does *not* compare is the §4 Contact projections (`q`, `+sip.instance`, `reg-id`,
  `pn-*`) — pre-existing, unchanged here, and now recorded in §5.3.1 rather than left silent,
  because widening B4 made it reachable in practice for the first time. It is not a double-spend:
  B4 mutates nothing.
- **Failing-first, both halves.** `ra_r_1_a_retransmitted_register_authenticates_again` with its
  restored assertion failed `[401, 200, 500]` vs `[401, 200, 200]`; `ls_r_3_…` with the 500 ms
  delay failed `left: 500, right: 200`. Both pass on the one-line change to `already_holds`.
- **Review round 2 — B4.3, because the first draft of §5.3.1 over-promised.** "The carve-out stays
  narrow" and §5.3's "replaying the same token a no-op rather than a second write" both read as if a
  spent token could never produce a commit. It can: `process::explicit` reaches the ordering check
  only for a **matched** binding, so a request that replays a spent token *and* carries an unbound
  contact adds it via B1 and commits. Measured on this branch: `status=200 commits=true
  rev=Revision(2)`, CA's deadline untouched. It is a wording and vector gap rather than a hole — the
  same UA can register that contact in a request carrying only it, which B1 accepts identically at
  the merge base, so the ordering token never gated the addition; the principal (S3/S4) and the
  quota (§5.5) do. Fixed by stating it: rule **B4.3**, the qualified sentence in §5.3, vector
  `LS-R-23`, and `ls_r_23_a_replayed_token_still_adds_a_contact_it_never_bound`.
- **Review round 2 — the cross-backend suite's `LS-R-22` asserted only the status**, where its
  sibling B5 row `LS-R-4` also asserts the store did not move, so a backend that aborted *and*
  committed would have passed. Revision check added. Both new suite checks were falsified
  deliberately before being left in — flipped, observed to report
  `in-memory: LS-R-23 — expected CA untouched beside a new CB` and
  `in-memory: LS-R-22 — the store moved under an aborted request`, then restored.

## Notes
- Found by: [`RG-2`](RG-2-implement-server-side-digest-authentication.md); the defect was pinned by
  `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule`, now deleted —
  `RG-2`'s own story text still names it and is left to the integration pass, being outside this
  story's fence.
- Supersedes the open question in
  [`RG-3`](RG-3-implement-register-processing-on-the-in-memory-store.md).
- Relevant: `crates/sipx-clstr-registrar/src/process.rs` (`already_holds`),
  `docs/specs/location-service.md` §5.3 and row `LS-R-3`.
- RFC 3261 §10.3 step 7 aborts on a CSeq that is *not higher*, so B4 is a deliberate
  **[sipx-clstr]** carve-out for retransmissions. Widening it is widening our own rule, not
  deviating from the RFC — but the carve-out must stay narrow enough that a *second write* under a
  spent token is still refused.
