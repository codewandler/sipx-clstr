---
id: RG-20
title: Reject malformed present registration fields instead of treating them as absent
pillar: Registrar
status: ready
priority: 1
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, parsing, upstream]
note: V-13 · CX-7 confirmed no kernel gap: v0.10.0 has fallible Expires; consume it and reject malformed Contact/Path atomically
---

# Reject malformed present registration fields instead of treating them as absent

## Goal

Preserve the distinction between an absent registration field and a present malformed field, so bad
Expires or Path syntax is rejected atomically rather than acquiring defaults or disappearing.

## Acceptance

- [ ] REGISTER admission consumes the pinned sipx v0.10.0
      `sipx_sip::headers::Expires` typed/fallible surface. This repository does not add a second raw
      parser for generic Expires grammar.
- [ ] A missing Expires header/parameter remains `None` and follows E1–E3; a present non-numeric,
      negative, overflowed or otherwise invalid value becomes `400 Bad Request`.
- [ ] A malformed value in any Contact or Path header rejects the entire REGISTER. Valid siblings are
      neither committed nor retained as a partial Path vector.
- [ ] Existing wildcard rules remain exact: `Contact: *` requires a syntactically valid explicit
      `Expires: 0`; malformed Expires is not treated as absent.
- [ ] **Failing-first byte vectors:** malformed header Expires, malformed Contact `;expires`, and one
      malformed Path among valid Path values each produce 400 and no store read/commit. On `86e6b10`
      the expiry cases default and malformed Path is filtered out.
- [ ] q parsing remains unchanged except that the shared malformed-present representation is used
      consistently where applicable.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-13**](../reviews/00-validated-synthesis.md#v-13--malformed-registration-parameters-are-treated-as-absence).
- **Upstream boundary:** no missing generic primitive was found: sipx v0.10.0 already distinguishes
  absent from malformed Expires. Local REGISTER admission must consume it and reject malformed
  Contact/Path presence atomically; if that work exposes a concrete missing typed primitive, file it
  upstream rather than adding a shadow parser.
