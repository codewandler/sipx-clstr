---
id: FC-4
title: Apply or refuse the per-tenant policy fields — domains, expiry and maxBindingsPerAor
pillar: Cluster
status: ready
priority: 4
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [registrar, location, deploy]
note: domains parses into a struct field nothing reads — a REGISTER for an undeclared domain is accepted
---

# Apply or refuse the per-tenant policy fields — `domains`, `expiry` and `maxBindingsPerAor`

## Goal

Make the rest of `tenant[]` mean something. `domains` is parsed into a field nothing reads, and
`expiry` and `maxBindingsPerAor` are accepted and dropped, so the registrar runs on
`TenantPolicy::default()` no matter what the document says.

## Acceptance

- [ ] `domains` is enforced or refused. **Failing-first**: a test registers an address-of-record in a
      domain the document does not declare and requires a rejection. It fails today — verified by
      hand: with `domains: [example.test]`, a REGISTER for `alice@attacker.invalid` was answered
      `200 OK`.
- [ ] `maxBindingsPerAor` from the document reaches the quota check. The check itself is real and
      correct — `process.rs` tests `set.active_count(cmd.now) > policy.max_bindings_per_aor` — but
      `driver.rs` builds `TenantPolicy::default()`, so the effective cap is always 10.
      **Failing-first**: a document setting `maxBindingsPerAor: 3` is proved to refuse the fourth
      binding.
- [ ] `expiry` reaches the granted-duration logic
      ([location-service](../specs/location-service.md) §5.2's defaults, minima and maxima), or the
      key is refused. Note that §5.2's defaults and `RG-8`'s granted-duration settlement are already
      correct; what is missing is only the path from the document to them.
- [ ] `TenantSpec` stops being a three-field struct that the allow-list implies is a six-field one.
      Whatever is not plumbed is refused, per the epic's rule 1.
- [ ] Multi-tenancy is not accidentally introduced. `startup.rs` takes `tenants.first()` today and
      says why — the driver's shape is one tenant per node (`RG-12`'s note). This story keeps that
      and does not invent a selection rule the schema does not have; if the document declares two
      tenants, the existing behaviour or an explicit refusal, decided and recorded.
- [ ] `cargo test -p sipx-clstr-node -p sipx-clstr-registrar` green.

## Progress

- (running log)

## Notes

- **The three keys are not equally severe, and the story says so.** `domains` is an access-control
  boundary: without it, an open registrar accepts any address-of-record in any domain, which is also
  what makes the store's unbounded row growth reachable by anyone who can send a datagram (`RG-13`).
  `maxBindingsPerAor` is the only per-AoR resource cap in the system and it silently ignores a
  tighter value than its default. `expiry` is the mildest — a wrong-but-sane default rather than an
  absent control.
- **Why `domains` is the interesting one.** It is the clearest instance of the epic's rule 1: the key
  is validated, projected into `TenantSpec.domains`, and then read by nothing anywhere in the
  workspace. To an operator reading the document there is no difference between that and enforcement.
- `location-service` §5.2/§5.5 and §5.1 S1 are the normative homes for expiry and quota, and
  [cluster-config](../specs/cluster-config.md) §5 S2 is why they are per-tenant rather than under
  `registrar`. Derive the tests from those rows rather than inventing thresholds here.
- Check whether a domain check has a rule id already. `Rejection::NotFound` exists but is constructed
  nowhere outside its own status test, which suggests the S1/S5 domain path was specified and never
  implemented — if so, the spec row exists and this story is implementing it, not writing it.
- Upstream? No. Tenancy and per-tenant policy are platform concepts; the kernel has no tenant.
