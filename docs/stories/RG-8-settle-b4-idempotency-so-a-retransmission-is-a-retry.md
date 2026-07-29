---
id: RG-8
title: Settle B4 idempotency so a retransmission is a retry
pillar: Signalling
status: ready
priority: 1
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
- [ ] §5.3 says which of the two readings is normative, in the spec itself rather than in a story:
      the granted **duration**, or the originating `now` carried with the command. The rejected
      option is recorded with its reason, because the next reader will ask.
- [ ] The `LS-R-3` vector row states the elapsed time between the original and the retry, so
      "identical outcome" stops being satisfiable only by a zero-latency retry.
- [ ] `process::already_holds` implements the chosen reading, and a REGISTER retransmitted after a
      delay is a `Noop` with the current set and an unchanged revision.
- [ ] `sipx-clstr-sim/tests/register_auth.rs` — `ra_r_1_a_retransmitted_register_authenticates_again`
      regains its `phone.answers == [401, 200, 200]` assertion, and
      `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule` is deleted rather
      than inverted: it exists only to pin the defect.
- [ ] `a_re_presentation_at_a_later_instant_is_not_a_retry_and_is_refused` in
      `tests/vectors_register.rs` is updated or removed, and the decision it pinned is superseded
      explicitly in [`RG-3`](RG-3-implement-register-processing-on-the-in-memory-store.md)'s open
      question rather than left contradicting the code.
- [ ] A binding is never **extended** by a retry. B4's remedy is *no mutation*; a retry that
      refreshed the deadline would make the ordering token spendable more than once.

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

## Notes
- Found by: [`RG-2`](RG-2-implement-server-side-digest-authentication.md); the defect is pinned by
  `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule`.
- Supersedes the open question in
  [`RG-3`](RG-3-implement-register-processing-on-the-in-memory-store.md).
- Relevant: `crates/sipx-clstr-registrar/src/process.rs` (`already_holds`),
  `docs/specs/location-service.md` §5.3 and row `LS-R-3`.
- RFC 3261 §10.3 step 7 aborts on a CSeq that is *not higher*, so B4 is a deliberate
  **[sipx-clstr]** carve-out for retransmissions. Widening it is widening our own rule, not
  deviating from the RFC — but the carve-out must stay narrow enough that a *second write* under a
  spent token is still refused.
