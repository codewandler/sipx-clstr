---
id: RG-10
title: Say that a replayed credential can de-register every binding
pillar: Signalling
status: done
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth]
note: found reviewing RG-9 — §7.2 understates the exposure it exists to state
---

# Say that a replayed credential can de-register every binding

## Goal
Correct [registrar-auth](../specs/registrar-auth.md) §7.2, which describes the accepted digest
exposure as strictly **additive** — the attacker's contact joins the victim's, "the phone that owns
the AoR keeps ringing, so nothing about the victim's experience says this happened". For one variant
that sentence is false, and §7 is the one section whose stated purpose is not to understate.

## Acceptance
- [x] §7.2 names the removal case: the digest binds neither `Contact` nor `Expires` nor `Call-ID`,
      and this registrar implements wildcard removal
      ([location-service](../specs/location-service.md) §W3, "`Call-ID` differs → remove"), so the
      same replayed `Authorization` on a REGISTER carrying `Contact: *`, `Expires: 0` and a fresh
      `Call-ID` removes **every** binding for the AoR.
- [x] The operator-facing residual-risk sentence in §7.3 covers loss of service, not only forking.
      It currently describes only "have the AoR fork to them".
- [x] RFC 3261 §26.1.1 is cited for what it actually says — *"could, for example, de-register all
      existing contacts for a URI and then register their own device"* — which §7 already cites for
      the weaker claim.
- [ ] A vector pins the removal variant, as `RA-R-6` pins the additive one. **Partial** — the row
      exists (`RA-R-7`) but, unlike `RA-R-6`, is not proved by a test; see Progress.
- [x] The decision is re-affirmed or revisited **in light of the corrected impact**. Accepting a
      quiet fork and accepting total de-registration are not the same decision, and `RG-9` only
      argued the first.

## Progress
Verified the defect against the code before writing anything: `process.rs`'s `wildcard()`
(`crates/sipx-clstr-registrar/src/process.rs:56-76`) rejects only when
`binding.call_id == cmd.call_id && cmd.cseq <= binding.cseq`; for a **fresh** `Call-ID` that
condition is false for every stored binding, so nothing in the ordering guard fires and every
binding is removed unconditionally — matching `location-service` §W3 exactly (`docs/specs/
location-service.md:335`) and confirming §7.2's claim was wrong for this variant, not merely
imprecise.

Changed `docs/specs/registrar-auth.md`:
- §7.2: scoped the existing paragraph's closing sentence to the explicit-`Contact` variant
  (`RA-R-6`) rather than letting it stand as a claim about every replay, and added a new
  "The removal variant" paragraph naming the wildcard case explicitly, citing
  `location-service` §W3, the `process.rs:56-76` implementation, and RFC 3261 §26.1.1's
  "de-register all existing contacts ... and then register their own device". Also widened the
  third "what is not weakened" bullet so it reads correctly for a wildcard REGISTER, not only an
  explicit one (the `S4` gate on `To` is unchanged either way, so this is a wording fix, not a
  new claim).
- §7.3: reworded the "Accepted" lead so it re-argues the decision against the corrected impact
  rather than silently carrying the old fork-only argument forward — the reasoning is that
  `qop=auth` does not distinguish an additive REGISTER from a removing one (same unbound fields,
  §7.1; same nonce-lifetime bound), and every rejected mitigation in the table was evaluated
  against closing the exposure in general, not one variant of it. Rewrote the operator-facing
  residual-risk sentence to name both outcomes — fork *and* total removal — and to say plainly
  that the removal case is the one to weigh, since it is loss of service rather than unwanted
  company.
- §8: added `RA-R-7`, the wildcard/removal sibling of `RA-R-6`, in the same row style.

Registered `RA-R-7` in `docs/reference/vector-scope.toml` as **deferred** rather than proved: this
story is documentation-only (no crate edits, no `cargo` run, per the task's hard constraints), so
unlike `RA-R-6` the new row has no test driving it end to end yet. The two halves of the mechanism
are each covered separately today — `wildcard()`'s W3 ordering behaviour, and `RA-R-6`'s
replayed-credential tests — but nothing yet exercises them together. The deferral names a story
`RG-11` to add that test (the direct end-to-end sibling of `RA-R-6`'s), but **`RG-11` is not
actually filed** — creating a new story file was outside this pass's file mandate. This is a real
gap: the deferred entry currently points at an ID with no backing file, unlike every other row in
`vector-scope.toml`. The coordinator should run `/track:story` to file `RG-11` ("prove `RA-R-7` —
a replayed credential empties an AoR through the wildcard path") so the reference resolves to a
real story, and then have it write the test.

Regenerated `docs/reference/conformance.md` via `python3 scripts/check-vectors.py` (script-driven,
not hand-edited).

Gate: `scripts/check-docs.sh`, `scripts/check-provenance.sh` and
`scripts/check-vectors.py --check` all green — see the story implementer's report for verbatim
output. No `cargo` command was run, per the task's constraints; nothing in this change touches
Rust source, so that gate was not exercised and is unaffected.

## Notes
- Filed from the independent review of `RG-9`. That story's technical work was verified correct
  against the pinned kernel (`v0.7.0`, `67c21039`) and every RFC citation in its new §7 resolves;
  this is about the impact statement, not the mechanism.
- Separable, from the same review: §7.3 rejects the registrar-side check in its **weakest** form
  only — "remember the `Contact` first seen under a nonce and refuse a change". The narrower variant
  (accept an equal-`nc` repeat only when the binding fields are also equal, i.e. tell a byte-identical
  retransmission from a reattach) is not considered, and neither stated reason applies to it: RFC
  7616 §3.4 makes `nc` increment per request, so a client legitimately changing its `Contact`
  arrives on a *higher* `nc` and never takes the equal-`nc` branch. The accept decision may still be
  right — the accept branch is kernel-side, so narrowing it is an upstream conversation — but the
  table reads as though the option space were exhausted.
