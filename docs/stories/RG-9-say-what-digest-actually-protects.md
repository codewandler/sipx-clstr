---
id: RG-9
title: Say what digest actually protects, and decide whether that is enough
pillar: Signalling
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: found reviewing RG-8 — RA-R-2 reads stronger than the mechanism delivers
---

# Say what digest actually protects, and decide whether that is enough

## Goal
Close the gap between what [registrar-auth](../specs/registrar-auth.md) §5's `RA-R` rows *imply*
digest authentication protects and what it actually covers, and decide explicitly whether the
platform accepts that or narrows it. A specification that promises more integrity than the
mechanism delivers is worse than one that promises less, because it stops the next reader looking.

## Acceptance
- [ ] `RA-R-2`'s "Given" and "Expect" are restated in terms of what the digest actually covers.
      The row currently reads *"a captured credential reused against a different request"* is
      refused; what is refused is a **different digest at the same `nc`**, which is not the same
      claim.
- [ ] §5 states normatively which parts of a REGISTER the digest binds — method and Request-URI —
      and which it does **not**: `Contact`, `CSeq`, `Call-ID`, `Expires` and the body. Cited to
      RFC 7616 §3.4.2 (and RFC 2617 §3.2.2.1 for the historical form), not asserted.
- [ ] The decision is recorded either way, with its reason: accept the exposure and bound it by the
      nonce lifetime (§6), or narrow it — `qop=auth-int`, binding the nonce to more of the request,
      or a registrar-side check. If it is accepted, say what the residual risk is in one sentence an
      operator can act on.
- [ ] A vector pins the actual behaviour: a captured `Authorization` reattached to a REGISTER whose
      `Contact`, `CSeq` and `Expires` differ but whose method and Request-URI match produces an
      **identical** digest and is accepted today. Whatever the decision, the vector states the truth.
- [ ] If the decision is to narrow, the change lands with a failing-first test; if it is to accept,
      no code changes and the story closes on the spec and the vector alone.

## Progress
- **Filed 2026-07-29, from the independent review of [`RG-8`](RG-8-settle-b4-idempotency-so-a-retransmission-is-a-retry.md).**
  Not a defect in `RG-8`'s diff — pre-existing, and the reviewer said so — but `RG-8` widened B4,
  which made it worth asking what else stands between a replayed REGISTER and a write.
- Verified in the pinned kernel (`v0.7.0`, `67c21039`) rather than inferred:
  - `crates/sipx-ua/src/challenge.rs:356-363` — the expected response is computed over the
    challenge, credentials, **`method`**, **`presented.uri`**, `nonce_count` and `cnonce`. No header
    of the request and no body enter it.
  - `crates/sipx-ua/src/challenge.rs:400-402` — the replay window returns `Ok(())` when
    `count == entry.1 && entry.2 == presented.response`. Accepting an identical `(nc, response)`
    indefinitely is deliberate and correct: it is what makes `RA-R-1` — an ordinary UDP
    retransmission — authenticate. `Reason::Replay` is returned only for a *different* digest at
    the same `nc`.
- The consequence: a captured `Authorization` reattached to a REGISTER with a different `Contact`,
  `CSeq` or `Expires` — but the same method and Request-URI — hashes to the same value, so it takes
  the accept branch. `RA-R-2` at `docs/specs/registrar-auth.md:160` reads as though that case is
  refused.
- This is inherent to `qop=auth`, not a bug in the kernel or in `parse::admit`. The story is to say
  so in the spec and decide, not to treat it as a hole to patch quietly.

## Notes
- Bounded by the nonce lifetime (§6, `RA-R-5`), so the exposure window is not unbounded — that bound
  is part of what the decision should weigh.
- Interacts with [`RG-8`](RG-8-settle-b4-idempotency-so-a-retransmission-is-a-retry.md): B4's
  no-mutation carve-out is *not* a security boundary and should not be read as one. `RG-8`'s
  §5.3.1 says so for B4; this story says it for the layer above.
- Relevant: `docs/specs/registrar-auth.md` §5 (`RA-R` rows) and §6 (the nonce window),
  `crates/sipx-clstr-registrar/src/parse.rs` (`admit`).
- **Considered for upstream:** the *mechanism* is the kernel's (`sipx-ua`'s digest is a faithful
  RFC 7616 implementation and this is what that RFC specifies), so any change to what is covered
  would be a kernel conversation via [upstream.md](../upstream.md). The **decision** and the
  specification of it are ours.
