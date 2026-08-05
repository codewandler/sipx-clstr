---
id: FC-7
title: Expose the contact-operation bound beside the quota it must not contradict
pillar: Cluster
status: ready
priority: 2
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, registrar]
note: RG-25 made max_contact_ops a per-tenant policy field with no document key — raise maxBindingsPerAor past it and whole-set refreshes start answering 403
---

# Expose the contact-operation bound beside the quota it must not contradict

## Goal
Let a deployment set the contact-operation bound from the configuration document, and refuse a
document whose bound contradicts the binding quota it sits next to.

## Acceptance
- [ ] `max_contact_ops` is reachable from the cluster document as a `tenant[]` field, beside
      `maxBindingsPerAor`, and reloadable on the same terms.
- [ ] `location-service` §5.5.1's consistency rule — `max_contact_ops >= max_bindings_per_aor` — is
      **enforced at load**, not merely stated. A document that violates it is refused with an error
      naming both values and both paths, in the "apply or refuse" shape `FC-1`/`FC-3` established.
- [ ] `cluster-config` §7's registry, the `SipxCluster` CRD spec and the `values.yaml` mapping all
      carry the new key, and `scripts/check-crd-drift.py` stays green — the drift checker is what
      makes those three one definition, so this story either satisfies it or is wrong.
- [ ] The default is declared once, in the owning spec, per `cluster-config` §8 `V3` — it must not be
      spelled a second time in the schema.
- [ ] **Failing-first test:** a document setting `maxBindingsPerAor: 100` without raising the bound is
      refused at load. Today it starts, and every whole-set refresh above 64 operations is answered
      `403` at call time instead.
- [ ] A vector row for the refusal, registered with its test name in the same commit.

## Progress
- (not started)

## Notes
- Found by `RG-25`, which correctly declined to fix it: adding a document key touches
  `cluster-config` §7, the CRD and the published site, which is a different story's blast radius. It
  took the library default explicitly in `startup.rs` with a comment instead, per §8 `V3`'s rule that a
  number spelled twice drifts.
- **The footgun is the pairing, not the bound.** `RG-25` chose `64` — comfortably above the default
  quota of `10` and above any plausible UA. But `maxBindingsPerAor` *is* per-tenant configurable, so an
  operator who raises it past `64` silently loses the ability to refresh the whole set in one REGISTER
  and gets a `403` no message explains. The bound and the quota have to move together or be refused
  together.
- `RG-25` deliberately stated the consistency rule in the spec rather than enforcing it, on the
  grounds that enforcement belongs to a configuration surface rather than to a pure decision function.
  That is the right split, and this story is the other half of it.
- Note the two refusals share `Rejection::Forbidden` and differ only by message string, so anything
  matching on the variant cannot tell "too many contacts" from "quota exceeded". Worth deciding here
  whether the configuration error should make that distinction unnecessary.
- Considered for upstream: **no.** A tenant policy key and its load-time validation are this
  platform's configuration surface; the kernel has no notion of tenants.
