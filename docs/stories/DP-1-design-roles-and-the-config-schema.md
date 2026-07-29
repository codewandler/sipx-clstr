---
id: DP-1
title: Design roles and the config schema
pillar: Cluster
status: done
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [deploy]
note: 
---

# Design roles and the config schema

## Goal
Design the typed, versioned configuration: role selection (edge / registrar / inbound-proxy / outbound-proxy), membership, shard map, token keys, trunks — with a defined reloadable subset.

## Acceptance
- [x] The schema validates at startup with precise errors; one binary boots into any role combination.
- [x] The reloadable subset (trunks, keys, shard map) is defined with drain-then-switch semantics for the shard map.
- [x] AF-6's membership/key sections are integrated, not duplicated.

## Progress
- The schema is [cluster-config](../specs/cluster-config.md), normative, with `CC-*` vector
  tables. `deployment.md` carries the decision record and the full reconciliation table against
  `deploy/helm/values.yaml`.
- **Precise errors**: §8. `ConfigError { path, rule, found, expected }`; every error reported, not
  the first (V1); closed world, so an unrecognised key is an error and not a shrug (V2); the only
  substitution is `${NAME}` and an undefined one names the *variable*, not a downstream address
  failure (V4); cross-section rules are ordinary rules carrying the owner's rule id (V5);
  refusal is the only failure mode (V10).
- **Any role combination**: six roles (`echo` included — [e2e-probe](../specs/e2e-probe.md) §9),
  and R3 is what makes combination safe — a role selects which decision paths are *wired*, never
  what a request decides, so `inbound-proxy` + `outbound-proxy` have nothing to disagree about.
  The one refused combination is a probe role beside a call-path role, which e2e-probe §11
  requires the schema to refuse.
- **Reloadable subset**: §9, three mechanisms with three names — trunks *stamp and retire*, keys
  *distribute then activate*, the shard map *drains then switches* (§9.4 has the state table and
  the transitions). Each says what happens to work in flight; no established dialog is disturbed
  by any of them.
- **AF-6**: §10 is the seam — where the three sections live, that they are versioned with the
  document and reloadable, what `membership`/`keys`/`shardMap` must each provide, and A5's list of
  what is deliberately left to AF-6. No field of theirs is restated.
- Vector registration is **deferred to `CF-8`**, recorded in §12 the way `number-normalisation`
  defers its `NN` rows. The registry was fenced this wave.
- Gate green on this branch (`77/85` rows proved — the pre-`EX-8` count for this merge base).
- Open, and named rather than silently carried: `deploy/helm/**` must follow the reconciliation
  table (KO-2 owns it); `Listeners::receiving` keys arrival on the transport and must key on the
  receiving local address before per-role port separation is expressible (§5 P6 refuses that
  projection until then); `nat:` has a home and a reload class but no owning spec.

## Notes
- Design: [deployment](../designs/deployment.md).
- The role set also carries `e2e-tester` ([ET-1](ET-1-specify-the-e2e-tester-role-and-probe-contract.md)) — a probe role, never on the call path.
- This schema is the single source for the operator's `SipxCluster` spec ([KO-1](KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md)); a second dialect is a defect.
