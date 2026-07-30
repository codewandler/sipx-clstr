---
id: KO-1
title: Specify the SipxCluster CRD and the values.yaml contract
pillar: Cluster
status: done
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: the CR spec *is* the config schema, and a check holds it there
---

# Specify the SipxCluster CRD and the values.yaml contract

## Goal
Specify one desired-state document: a `SipxCluster` custom resource whose spec is DP-1's config schema, rendered 1:1 from a single `values.yaml`, plus the status conditions that make reconciliation auditable.

## Acceptance
- [x] The CRD spec covers zones, roles and replicas (`edge`, `registrar`, `inbound-proxy`, `outbound-proxy`, `e2e-tester`), listeners per transport with addresses and port ranges, deployment profile, location store, media pool, token keys and membership, trunks and route policy, and probe configuration.
- [x] `status` is specified: conditions (`Ready`, `ProfileCompatible`, `ShardMapConverged`, `KeysDistributed`), observed shard map, per-role ready counts, last probe verdict.
- [x] The single-source mechanism between DP-1's config schema and the CRD is decided and recorded — generated one from the other, or one shared definition — with a check that fails when they drift.
- [x] The `values.yaml` → CR mapping is documented field by field and is 1:1; no value is computed in the chart that the CR cannot express.
- [x] Validation rules are normative: an incompatible profile/role set, a media pool without a port range, or a zone with no edge is rejected at admission, not at call time.
- [x] CRD versioning and upgrade policy is stated.

## Progress
- **Specified 2026-07-30** in [sipx-cluster-crd](../specs/sipx-cluster-crd.md). The resource's `spec`
  is `cluster-config` §7's `cluster:` tree verbatim (§2 K1) beside a closed operator half of exactly
  four fields — `image`, `roles`, `nodeSelector`, `tolerations` (K3). The spec names sections and
  restates no field, type, default or ceiling (M2), which is what makes it a single source rather
  than a second copy.
- **The group and version are pinned: `sipx.dev/v1alpha1`** (§3 G1), `kind: SipxCluster`, plural
  `sipxclusters`, scope Namespaced. This was already the value `cluster-config` §3 declares and the
  loader implements as `API_VERSION`, so pinning it removed the word *provisional* from
  `deploy/helm/values.yaml` rather than choosing a new string — the alternative would have broken the
  site, both node proofs and the loader constant to buy nothing a reader can see.
- **Single-source mechanism: one shared definition with a declared inclusion, not generation**
  (§4, with the reasoning). Neither artefact is machine-readable today — §7 is a prose registry whose
  owner column is the load-bearing part, and there is no CRD manifest until `KO-3` — so either
  direction of generation would have made the file contributors actually read into derived output.
  Because the inclusion is total and verbatim, the only thing that can drift is *which sections
  exist*, and that is what the check compares. §4 records when this is revisited: `KO-3`'s structural
  schema.
- **The drift check is `scripts/check-crd-drift.py`**, wired into `scripts/gate.sh` after the vectors
  step **and into `.github/workflows/docs.yml`** — that workflow enumerates its checks rather than
  calling the gate, so a step added only to `gate.sh` would never run on a pull request. Five axes,
  each demonstrated red before being made green: the schema version's four spellings
  (spec pin, `cluster-config` §3, `API_VERSION`, chart) must be byte-identical; §7's section set and
  the spec's config-half rows must match in both directions; the operator half must agree across the
  spec, `node-document.py`'s `OPERATOR_KEYS` and the keys `templates/sipxcluster.yaml` writes into
  `spec` itself; and the `values.yaml` mapping must be 1:1 both ways. It reads **names only** —
  contents remain `deploy/helm/check-values.sh`'s, which feeds the rendered tree to the real loader.
  It self-tests on every run.
- Dropping the `admission` row (the section `DP-11` added to §7) reproduces exactly the drift class
  this check exists for and is reported with the fix in the message.
- **Landed on integration:** the sixteen `SC-*` deferrals to `KO-3` are in
  `docs/reference/vector-scope.toml` and `docs/reference/conformance.md` is regenerated with them.
  This story was fenced out of that file, so the rows landed registered-and-undeferred and the
  integrator filed them — with a reason per row rather than sixteen copies of one, because the rows
  do not defer for one reason. The vectors step reports `125/549 rows proved, 19 covered for shape
  only, 405 deferred with a reason`, exactly what the scratch mirror predicted.
- **Closed 2026-07-30.** Gate green at the merge tip, `crd drift` included.
- Two findings recorded in §11 rather than fixed here: `deployment.rtpengine.enabled` is a second
  spelling of `cluster.mediaPool[].mode: managed` and belongs to `KO-7`/`KO-2`; the template's
  `apiVersion` may become a constant now that G1 pins it, which is `KO-2`'s.

## Notes
- Design: [k8s-deployment-operator](../designs/k8s-deployment-operator.md). Config schema: [DP-1](DP-1-design-roles-and-the-config-schema.md). Profile compatibility: [EX-5](EX-5-implement-deployment-profiles-with-compatibility-checking.md).
