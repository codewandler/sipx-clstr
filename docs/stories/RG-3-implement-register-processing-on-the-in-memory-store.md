---
id: RG-3
title: Implement REGISTER processing on the in-memory store
pillar: Signalling
status: done
priority:
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: M1 #4 · the location service on the in-memory store; runs the LS-* vectors in the harness
---

# Implement REGISTER processing on the in-memory store

## Goal
Implement RG-1's REGISTER semantics against the in-memory `LocationStore`, proving the CAS contract before any database is involved.

## Acceptance
- [x] RG-1's vectors pass: add/replace/remove, wildcard deregistration, Call-ID/CSeq rules, `Min-Expires`/423, expiry, complete-set responses.
- [x] Retried REGISTERs are idempotent; concurrent updates to one AoR serialize via CAS conflict-and-retry under the harness.

## Progress
`sipx-clstr-registrar`, 78 tests, plus 5 harness scenarios in `sipx-clstr-sim`. **Every vector
table in the spec is executable**: all 22 `LS-C` canonicalization rows, all 21 `LS-R` REGISTER
rows, all 6 `LS-K` consistency rows, all 8 `LS-L` lookup rows, and the 3 `LS-H` shard-key rows.

**Canonicalization is a total function; §19.1.4 stays a comparison.** `CanonicalAor` implements
§3.2's N1–N13 as an injective printable encoding of §10.3 step 5's canonical URI. The kernel keeps
§19.1.4 — which is non-transitive by the RFC's own example and therefore cannot key a hash — and
this crate uses it only where it belongs: matching an incoming contact against stored bindings,
which is a linear scan. All 22 rows passed on the first run.

**The whole request commits or none of it does.** Expiry selection (E1–E6) runs for every contact
*before* the first mutation, because E6 fails the entire REGISTER; deciding per contact as you go
would leave the first one committed when the second is too brief. `LS-R-16` is that vector, and it
asserts the store is untouched.

**The quota is checked against the outcome, not the request** (S8), so a refresh or a removal can
never trip it — which is what `LS-R-15`'s second half asserts.

**CAS serialization is proved under the harness, not asserted at a store.** A synchronous
read-then-commit inside one function cannot race: the discrete-event scheduler runs one node input
to completion before the next. So the scenario's registrar edge is **two-phase** — a REGISTER reads
and arms a timer for the store round trip; the timer commits — which is what a real driver awaiting
a database does. Two edges handed the same address-of-record at the same virtual instant therefore
both read revision *n*, exactly one wins, and the loser re-reads and commits at *n+2*. The test
asserts the race **happened** (`conflicts == 1`), not merely that the result looks right: a
serialization test that silently never raced would pass for the wrong reason.

It also asserts the ordering nobody thinks to: **the `200` follows the commit.** A UA told its
registration succeeded before the write landed has been told it is reachable when it is not.

**`Applied` reports the conflict count** rather than swallowing it, which is what lets that
assertion exist at all.

**Deviation from the spec's shape, recorded:** the spec writes the outcome as `Commit | Noop` with
refusals carried inside a `Noop`'s response. Here it is `Commit | Noop | Reject`, so "did this
write?" is a question the type answers. Equivalent — a `Reject` is a `Noop` whose response is not a
`200`, and neither commits.

## Open question for RG-1

**A cluster-level re-presentation of one REGISTER is refused with `500`.** §5.3 defines an
idempotent retry as the same `(Call-ID, CSeq)` *and* the stored state already equalling the
command's requested outcome, "same granted expiry base". A CAS retry satisfies that, because `now`
is a field of the command and does not move across attempts. A re-presentation at a **different
node** does not: it stamps its own `now`, the expiry base differs, so B4 does not apply and B5
refuses it.

That is the spec as written, and `a_re_presentation_at_a_later_instant_is_not_a_retry_and_is_refused`
pins it so the behaviour is a decision on the record. Whether it is the behaviour we want is a
question for the cluster stories: §5.3's own prose says the rule exists so that "cluster-level
retries re-present a command safely", and under the current definition they cannot. Options are to
compare the granted *duration* rather than the absolute deadline, or to carry the originating `now`
with the re-presented command. `AF-*`/`RG-5` will need one of them.

**Update 2026-07-29 (`RG-2`): it is not only a cluster concern, and it is reachable in M1.** The
deterministic harness now runs a phone that retransmits an authenticated REGISTER
(`sipx-clstr-sim/tests/register_auth.rs`). One node, one phone, no cluster: the retransmission
authenticates and is then refused `500` by B5, because it arrived a few hundred milliseconds after
the original and B4 compares deadlines. A lost `200` over UDP produces exactly this, so the option
chosen above is no longer only about re-presentation across nodes — it decides whether an ordinary
retransmission is answered correctly. `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule`
pins the current answer and will fail when it changes.

The question is therefore no longer deferred to `AF-*`/`RG-5`: it is
[`RG-8`](RG-8-settle-b4-idempotency-so-a-retransmission-is-a-retry.md), which owns choosing between
the two options above and changing §5.3 to say which one is normative.

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
- `Timestamp` lives in this crate because it is the only crate that needs one. `std::time::Instant`
  cannot be built from a number, which makes it the wrong type for logic whose time is an input.
  When the proxy engine needs one it moves somewhere shared rather than being defined twice.
- `RG-4` runs `tests/vectors_register.rs` unchanged against `PostgreSQL`; the store is reached only
  through the `LocationStore` trait there, so a backend that needs its own version of a row has
  broken the contract rather than implemented it.
- `clippy.toml` was added so `doc_markdown` stops asking for backticks around domain words like
  `AoR`. The lint still catches bare identifiers, which is what it is for.
