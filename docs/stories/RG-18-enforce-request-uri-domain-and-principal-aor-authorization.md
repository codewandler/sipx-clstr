---
id: RG-18
title: Enforce the REGISTER Request-URI domain and principal-to-AoR authorization gates
pillar: Registrar
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, auth, security, node]
note: V-09 · S1 checks the To-derived AoR with the wrong status and S4 is assumed but has no policy or implementation
---

# Enforce the REGISTER Request-URI domain and principal-to-AoR authorization gates

## Goal

Implement location-service S1 and S4 at the REGISTER admission boundary: validate the actual
Request-URI authority and prove that the authenticated principal may write the To-derived AoR before
any store read or mutation.

## Acceptance

- [ ] The parsed admission facts keep Request-URI authority distinct from the canonical To AoR;
      explicit ports, IPv6 literals, userless URIs, case rules and malformed authorities use typed URI
      accessors rather than string splitting.
- [ ] An unserved Request-URI domain returns S1's `404`; an invalid or out-of-domain To AoR returns
      the S5 result. Tests cover a served To with an unserved Request-URI and the inverse.
- [ ] An injected, tenant-scoped authorization policy decides `(principal, canonical AoR)`. A denied
      authenticated principal receives `403` before the location store is read.
- [ ] Open tenants have an explicit authorization decision for `principal: None`; they do not bypass
      S4 by falling through an absent check.
- [ ] **Failing-first security vector:** valid credentials for Alice attempt an explicit registration
      and a wildcard deregistration of Bob's AoR. Both are refused and Bob's binding/revision remain
      unchanged. They are admitted on `86e6b10` when a credential source supplies Alice.
- [ ] The authorized principal stored on every successful binding is exactly the identity the policy
      approved, preserving the existing audit record.
- [ ] Normative S1/S4 vectors, registrar tests, node wire tests and `scripts/gate.sh` are green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-09**](../reviews/00-validated-synthesis.md#v-09--registrar-domain-and-principal-authorization-gates-are-incomplete). The S4 exposure is latent until a real credential source ships; S1 is wrong in the current open registrar.
- The authorization policy is a required input, not a comparison of digest username text to the AoR
  user: aliases, shared lines and administrators make that shortcut an incorrect policy.
- **Upstream boundary:** digest verification and typed URI-authority parsing are sipx primitives;
  tenant principal-to-AoR authorization and served-domain policy stay here.
