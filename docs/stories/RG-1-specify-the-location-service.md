---
id: RG-1
title: Specify the location service
pillar: Signalling
status: done
priority: 2
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: UPSTREAM: Path header, sipx T-14 — see docs/upstream.md
---

# Specify the location service

## Goal
Write `docs/specs/location-service.md`: the binding model, AoR canonicalization, the `LocationStore` compare-and-swap contract, and REGISTER processing rules (RFC 3261 §10).

## Acceptance
- [x] AoR canonicalization is normative (it feeds the storage key and the rendezvous hash), with vectors for equivalent and distinct AoR spellings.
- [x] The binding schema covers contact, Call-ID, CSeq, expiry, Path vector (RFC 3327), received address, instance-id/reg-id, flow_ref, push metadata, authenticated principal, and revision.
- [x] `RegisterCommand`/CAS semantics: per-AoR serialization, idempotent retry, wildcard removal, Call-ID/CSeq comparison, `Min-Expires`/423 policy, per-tenant binding quotas, and the complete-set response — each with vectors.
- [x] The consistency contract is stated backend-agnostically; PostgreSQL is named as backend #1 with its mapping sketched (serializable per-AoR transactions, LISTEN/NOTIFY change stream, TTL-bounded read staleness).
- [x] The lookup contract returns a forking-ordered, routable target set (q-values, Path, flow_ref).

## Progress
- 2026-07-28 — Wrote `docs/specs/location-service.md` (normative, house format): §3 AoR
  canonicalization as an injective printable byte form of the RFC 3261 §10.3 step 5 canonical
  URI (full escape decode + deterministic re-escape over the RFC 2396 unreserved set), with
  its relationship to the kernel's non-transitive §19.1.4 `equivalent()` stated explicitly —
  comparison for contact identity, canonical bytes for keys/hashes; §4 binding schema with
  RFC 5626/8599 fields present now (M3-activated); §5 REGISTER processing order,
  expiry/423, wildcard, quotas, complete-set response, and the precise idempotency rule
  (per-binding Call-ID + CSeq per §10.3 steps 6–7, replay of an applied command is a no-op
  success); §6 backend-agnostic consistency (linearizable per-AoR CAS, atomic replacement,
  best-effort change stream, revision fencing, TTL-bounded cache staleness) with the
  PostgreSQL mapping sketched (revision-predicated per-AoR updates, LISTEN/NOTIFY,
  TTL caches); §7 lookup contract shaped for proxy-behavior §2/§7 consumption (q ordering
  with deterministic tie rules, expired exclusion, verbatim Contact + Path route set +
  opaque flow_ref); §8 rendezvous key bytes for RG-5 (`tenant 0x00 C(aor)`). Vectors
  LS-C-1…22, LS-R-1…21, LS-K-1…6, LS-L-1…8, LS-H-1…3.
- 2026-07-28 — Coordination applied: hook-framework spec (EX-1) landed mid-task; §5.7 now
  names exactly its H5/H6 registrar phases (`BeforeRegistrarUpdate`/`AfterRegistrarUpdate`)
  anchored to the §5.1 step table, with a once-per-request (not per-CAS-retry) firing rule.
  No missing registrar phase to report — the two phases bracket the CAS as required.
- 2026-07-28 — House decisions taken where RFCs are silent (flagged for review): stale-CSeq
  abort answers `500` (RFC 3261 §10.3 names no code); Path without `Supported: path` is
  rejected `421 Extension Required` (RFC 3327 leaves acceptance to local policy); quota
  breach answers `403`; absent q sorts as 1.0; canonical AoR capped at 512 bytes; hostname
  trailing-dot stripped in the key (coarser than kernel byte comparison, key-only).
- 2026-07-28 — Upstream (AGENTS.md rule 6): Path typed-header gap already ledgered in
  docs/upstream.md (referenced, not edited) — spec stores Path as verbatim bytes so the
  contract does not wait on it. Considered for upstream: AoR canonicalization — no,
  cluster-specific key policy; §10.3 REGISTER decision function — no for v1 (recorded in
  spec §1).
- 2026-07-28 — Opens for the integrator: (1) verify the exact RFC 3327 §5.3 echo/`Supported`
  response wording against the RFC text when RG-3 implements — the spec states echo-in-200 +
  `Supported: path`; (2) proxy-behavior does not yet state the empty-target-set response for
  §16.5 (the 480 case) — lookup §7 L5 returns the empty set and defers; flag for PX review
  rather than editing that spec; (3) board not regenerated (out of this task's scope) — run
  `/track:board` after status changes are merged.

## Notes
- 2026-07-28 — integrator review passed; cross-references reconciled (see CHANGELOG).
- Design: [registrar-location](../designs/registrar-location.md).
