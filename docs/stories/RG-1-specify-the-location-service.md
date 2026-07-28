---
id: RG-1
title: Specify the location service
pillar: Signalling
status: ready
priority: 2
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, location]
note: 
---

# Specify the location service

## Goal
Write `docs/specs/location-service.md`: the binding model, AoR canonicalization, the `LocationStore` compare-and-swap contract, and REGISTER processing rules (RFC 3261 §10).

## Acceptance
- [ ] AoR canonicalization is normative (it feeds the storage key and the rendezvous hash), with vectors for equivalent and distinct AoR spellings.
- [ ] The binding schema covers contact, Call-ID, CSeq, expiry, Path vector (RFC 3327), received address, instance-id/reg-id, flow_ref, push metadata, authenticated principal, and revision.
- [ ] `RegisterCommand`/CAS semantics: per-AoR serialization, idempotent retry, wildcard removal, Call-ID/CSeq comparison, `Min-Expires`/423 policy, and the complete-set response — each with vectors.
- [ ] The consistency contract is stated backend-agnostically; PostgreSQL is named as backend #1 with its mapping sketched (serializable per-AoR transactions, LISTEN/NOTIFY change stream, TTL-bounded read staleness).
- [ ] The lookup contract returns a forking-ordered, routable target set (q-values, Path, flow_ref).

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
