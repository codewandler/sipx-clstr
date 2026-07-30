# Spec: The `SipxCluster` custom resource

**Status:** normative · **Crate:** _future — lands with the operator (`KO-3`)_ ·
**Stories:** KO-1 · **Design:** [k8s-deployment-operator](../designs/k8s-deployment-operator.md)

[cluster-config](cluster-config.md) fixes one document that describes a cluster. This spec fixes the
**one desired-state resource** that carries it on Kubernetes, and it carries it *whole*: a
`SipxCluster`'s `spec` is that document's `cluster:` tree, verbatim, beside a small closed set of
deployment fields the document deliberately cannot express. There is no second configuration dialect,
no derived field and no translation layer, and §4 is the mechanism that keeps that true rather than
merely intended.

What this spec adds to [cluster-config](cluster-config.md) is everything Kubernetes contributes and a
configuration file cannot: the resource's identity and version, the **cluster-scope** validation that
no single node's projection can perform, and the `status` that makes reconciliation auditable. It
deliberately owns **no configuration field at all** — §5's table names sections and never their
contents.

## 1. Normative references

- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- [cluster-config](cluster-config.md), which this resource carries and never restates:
  §2 D3 (one data model, three encodings — the resource is the JSON one), D4 (`lowerCamelCase` keys,
  `kebab-case` values, chosen because this resource exists); §3 D6–D11 (the two version numbers,
  additive compatibility, the configuration version the operator stamps, and the condition D11 names
  `ShardMapConverged`); §4 R1/R2/R6 (the closed role set and the combinations refused); §5 P1
  (identity arrives from outside the document — on Kubernetes from the downward API), P3, P4;
  §6 I1/I2 (names, ids and uniqueness); **§7 (the section registry — the field set of `spec`'s
  configuration half, and the sole definition of it)**; §8 (validation, which §6 here performs in
  full rather than reimplementing); §9 (the reload classes that decide whether a change is pushed or
  rolled).
- [e2e-probe](e2e-probe.md) §6 and §10 (the `Pass` / `Fail { step, … }` verdict `status` reports),
  §9 (`echo` is a role), §11 (the role combination refused).
- [hook-framework](hook-framework.md) §8 (a profile is a named module set plus role flags plus
  profile values) and **EX-5** (the shipped catalog and its compatibility semantics) — the two
  things `ProfileCompatible` is a statement about.
- [affinity-token](affinity-token.md) §6 K1–K4 (distribute before mint, and the verify window
  `max(L, E_max) + S`) — what `KeysDistributed` asserts; §3 (the `policy version` the configuration
  version becomes).
- [location-service](location-service.md) §8 (the shard key space `observedShardMap` reports
  ownership of).
- [media-relay](media-relay.md) §13.1 (media policy is a property of the trunk) and **KO-7** (the
  pool's managed/external modes) — the reason §6 A6 is a pool rule and not a policy rule.
- **Kubernetes** as a named integration and interop target (AGENTS.md #1's carve-out): the
  CustomResourceDefinition, `metadata.generation`, the `status` subresource, the standard condition
  shape (`type`, `status`, `reason`, `message`, `lastTransitionTime`, `observedGeneration`),
  validating admission webhooks, conversion webhooks, additional printer columns, and the downward
  API. They are named as the surface this resource is expressed in, never as behavioural precedent.

**Out of scope.** The *contents* of every section §7 registers — this spec names sections and never
their fields, which is the whole of §4. Also out of scope: the CRD manifest, its RBAC and the
webhook's deployment
([KO-2](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-2-ship-the-helm-chart-for-a-local-k3s-environment.md));
the projection of this resource onto workloads, Services, ConfigMaps and Secrets, and the naming of
those objects
([KO-3](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-3-implement-the-operator-reconcile-loop.md),
[KO-10](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-10-specify-the-generated-object-naming-contract.md));
replica placement
([KO-11](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-11-place-same-role-replicas-one-per-node.md));
the staging of a change across roles and zones
([KO-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md))
and the drain machinery it stages
([KO-4](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-4-implement-sip-aware-rollout-and-shard-handoff.md));
and autoscaling
([KO-5](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-5-design-metric-driven-autoscaling.md)).

**Upstream considerations** (AGENTS.md #6): **no — none of this is the kernel's.** A custom resource
and its reconciliation are platform orchestration: the resource names roles, zones, shards, tenants,
trunks, media pools and deployment profiles, and the kernel has no opinion about any of them. Nothing
here touches header syntax, transaction or dialog semantics, resolver capabilities or auth
primitives, so nothing joins [the ledger](../upstream.md). The one adjacent question — whether a
configuration *schema* belongs upstream — [cluster-config](cluster-config.md) §1 already answered no,
and this spec adds only the Kubernetes surface around it.

## 2. What this is: one resource, two halves

```yaml
apiVersion: sipx.dev/v1alpha1        # §3 — the pin, and the document's schema version
kind: SipxCluster
metadata:
  name: sipx                         # the Helm release name (K5), never cluster.name
  generation: 7                      # the API server's; becomes the document's `version` (§3 G4)
spec:
  # ── the operator half: four fields, closed (§5) ────────────────────────────
  image: { repository: ghcr.io/sipx/sipx-clstr, tag: "0.1.0", pullPolicy: IfNotPresent }
  roles:
    edge: { replicas: 2, hostNetwork: true }
    registrar: { replicas: 1, hostNetwork: true }
  nodeSelector: {}
  tolerations: []
  # ── the configuration half: cluster-config §7's sections, verbatim ─────────
  name: sipx-local
  environment: local
  zones: [local]
  listener:
    - roles: [edge]
      transport: udp
      bind: "0.0.0.0:5060"
      advertise: "${POD_IP}:5060"
  # … every other §7 section, spelled exactly as §7 spells it
status: { … }                        # §7 here
```

| # | Rule |
|---|---|
| K1 | **`spec`'s configuration half is [cluster-config](cluster-config.md)'s `cluster:` mapping, verbatim.** Same key names, same value spellings, same nesting, same closed world. Not a subset, not a superset, not a renaming — so that "the config file" and "the desired state" are one document read two ways, which is the property the whole epic rests on. A reader who knows §7 knows `spec`. |
| K2 | **The resource adds no configuration field of its own.** Every key in the configuration half is a section §7 registers, and its content is that section's owner's. Where this spec appears to add a field, it does not: §5's table names sections, §6 names *cluster-scope validation*, and §7 names *observed state*. A field invented here would be the second dialect the design forbids, and §4's check is what makes that a red gate rather than a discovery. |
| K3 | **`spec`'s operator half is closed and is exactly four fields** — `image`, `roles`, `nodeSelector`, `tolerations` (§5). They are here because [cluster-config](cluster-config.md) cannot express them and must not: an image tag and a replica count are not read by a node, they *decide which nodes exist*, and §5 P1 puts a node's identity outside the document precisely so that the document is the same bytes everywhere. Adding a fifth is a change to this spec and to §4's three lists at once. |
| K4 | **The two halves never overlap.** No key is in both, and no operator-half field is derived from a configuration field or vice versa. The `roles` key is the sharp case and it is safe: §7 registers no `roles` section, because a *node's* role set is its identity (P1) while `spec.roles` is the *workload set* the operator creates. They are different facts with one name, and the name belongs to the half that is not the document. |
| K5 | **`metadata.name` is the resource's identity and is not `spec.name`.** `spec.name` is the cluster's own name in the configuration document (§7); `metadata.name` follows the Helm release, so re-applying an overlay upgrades the same resource in place. Deriving `metadata.name` from a `spec` field turns an overlay change into create-new-then-delete-old — two `SipxCluster`s claiming the same node-global ports, indefinitely if the delete fails. |
| K6 | **At most one `SipxCluster` per namespace**, refused at admission (§6 A3). Two resources in one namespace would each believe they own the node-global ports of every host-networked role, and the second one to reconcile would fight the first forever. One resource per namespace is the only shape in which "the desired state" is singular. |
| K7 | **The resource is the *only* input.** No annotation, label or ConfigMap outside it changes what the operator deploys. A deployment knob that lives beside the resource is a knob that no `kubectl get -o yaml` shows and no admission rule can validate. |

## 3. The pin: one API group, one version, one string

The resource's `apiVersion` **is** the schema version of the document it carries. Not a
correspondence to be maintained — the same string, in one place per artefact, checked by §4.

| # | Rule |
|---|---|
| G1 | **The API group is `sipx.dev` and the version is `v1alpha1`**; `apiVersion: sipx.dev/v1alpha1`, `kind: SipxCluster`, listed (plural) `sipxclusters`, scope **Namespaced** (K6). This was already the schema version [cluster-config](cluster-config.md) §3 declares and the loader implements as `API_VERSION`; pinning it is the act of removing the word *provisional* from the chart, not of choosing a new value. Choosing a new one would have broken every document in the repository — the site's getting-started page, the two-node proofs, the loader's own constant — to buy nothing a reader could see. |
| G2 | **One version field on the thing being versioned.** There is no `apiGroup`/`apiVersion` pair and no `crd:` block spelling the string in halves: a group and a version that can be configured separately can be configured into a combination that does not exist. |
| G3 | **The resource's `apiVersion` and the document's `apiVersion` are the same string, always.** A node reading a document extracted from this resource sees the resource's own `apiVersion` as the document's (§5's Kubernetes-native table), so D6's refusal — a node MUST refuse a schema version it does not implement, naming the ones it does — is what a version mismatch produces, and it is a refusal to start rather than a best-effort parse. |
| G4 | **`spec` carries no `version` key.** The document's *configuration* version (§3 D9, a `u32`, strictly increasing) is `metadata.generation`, which only the API server knows and which increments on every accepted `spec` change. The operator stamps it into the document it writes for a node. A `version` in `spec` would be a second answer to "which configuration is this node running", and D9's monotonicity would then be a property of whatever a values file happened to say. |
| G5 | **`spec` carries no `apiVersion` key either**, for G3's reason: it would be a copy of the resource's own, free to disagree with it. |

## 4. The single source, and the check that fails when they drift

**Decision: one shared definition with a declared inclusion — not generation in either direction.**
[cluster-config](cluster-config.md) §7 remains the sole place a configuration section is defined.
This spec *names* each section and restates none of its fields, and
[`scripts/check-crd-drift.py`](../../scripts/check-crd-drift.py) fails when the naming is not total
in both directions.

**Why not generate one from the other.** Generation needs a machine-readable schema, and neither
artefact is one today: §7 is a prose registry whose column of owners is the load-bearing part, and
there is no CRD manifest until `KO-3`. Whichever direction a generator ran, the *other* file would
become derived output — and the derived one here is the one contributors actually read. A generated
§7 would put the registry's reasoning (who owns a section, why `tenant[]` and not `registrar`, what
is unowned and says so) into a comment on a code generator; a generated CRD schema would need the
Rust types to be complete first, and [cluster-config](cluster-config.md)'s own header says the loader
implements ten sections of twenty-seven. A mechanism that only works once the work is finished is not
a mechanism for keeping the work honest while it is being done.

**Why naming is enough to be single-source.** Because the inclusion is *total and verbatim* (K1),
there is nothing to keep in step except the section list itself. §5's table is one row per section
with no field, type or default in it — so the only thing that can drift is *which sections exist*,
and that is exactly what the check compares. A field added inside a section needs no change here at
all, which is the property that makes this cheap enough to hold.

**When the mechanism is revisited.** When `KO-3` writes the CRD manifest it will need a structural
schema, and at that point one of the two becomes generated for real. G1's pin, §5's table and this
check are what make that a mechanical change rather than a reconciliation.

| # | Rule |
|---|---|
| M1 | **§7 is the definition; this spec is a declared inclusion of it.** A new configuration section is added to §7, given a row in §5, and nowhere else. Adding it here and not there, or there and not here, is a gate failure. |
| M2 | **This spec MUST NOT restate a field, a type, a default, a unit or a ceiling** from any section. §8 V3's "nothing is defaulted twice" applies to this document as strictly as to the schema, and the way to obey it is to have nowhere to write a default. |
| M3 | **The schema version has four spellings and they MUST be byte-identical**: the pin in §2, [cluster-config](cluster-config.md) §3's document, the loader's `API_VERSION`, and the chart's `apiVersion` value. The check compares all four. Four spellings is three too many and none of them is removable — a spec states it, a Rust constant enforces it, a chart renders it — so the answer is not to reduce them but to make disagreement impossible to commit. |
| M4 | **The operator half has three spellings and they MUST agree**: K3's four fields, `node-document.py`'s `OPERATOR_KEYS`, and the keys `templates/sipxcluster.yaml` writes into `spec` itself. This one has teeth beyond tidiness: those three lists decide *which half of `spec` a node reads*, so a key the template starts writing and the others do not know about is subtracted into the configuration document, where §8 V2's closed world refuses it by name and a node does not start. |
| M5 | **The `values.yaml` mapping is 1:1 and checked in both directions** (§5): every path the table names resolves in the chart, every top-level key the chart writes under `cluster:` has a row, and a row that says the default set omits a section is checked to have omitted it. |
| M6 | **The check reads names, never contents.** Whether a section's *content* is the document [cluster-config](cluster-config.md) specifies is `deploy/helm/check-values.sh`'s question, and it answers it the only way that means anything: by feeding the rendered tree to the real loader. The two checks are deliberately disjoint, and neither is a substitute for the other. |
| M7 | **The `deployment:` half is closed and declared too.** Every path the chart writes under `deployment:` is either an operator-half row of §5 or a row of §5's chart-local table, and §4's check holds both directions **one level below each key** — so a switch added inside an already-declared block is as visible as a new block. §5 has said since `KO-1` that a `values.yaml` key with no `SipxCluster` field is either a chart-managed dependency or a defect; nothing read that sentence, and `deployment.rtpengine.enabled` sat under it for two stories as a second spelling of `cluster.mediaPool[].mode: managed`, recorded in §11 and green in the gate (`KO-15`). A key the chart grows is now declared with a reason in the same commit, or it is red. |

## 5. `spec`, and the `values.yaml` mapping field by field

One row per field of `spec`. The `Half` column is `config` for a section
[cluster-config](cluster-config.md) §7 registers and `operator` for one of K3's four; `—` in the
first column means the **default deployment set** does not write the section, which is a property of
the chart's defaults and not of the resource. No row carries a field, a type or a default (M2).

| `values.yaml` path | `SipxCluster` field | Half | Content owned by |
|---|---|---|---|
| `deployment.image` | `spec.image` | operator | this spec (K3) |
| `deployment.roles` | `spec.roles` | operator | this spec (K3, K4); placement is `KO-11`'s |
| `deployment.nodeSelector` | `spec.nodeSelector` | operator | this spec (K3) |
| `deployment.tolerations` | `spec.tolerations` | operator | this spec (K3) |
| `cluster.name` | `spec.name` | config | [cluster-config](cluster-config.md) §7 |
| `cluster.environment` | `spec.environment` | config | [cluster-config](cluster-config.md) §7 |
| `cluster.zones` | `spec.zones` | config | [cluster-config](cluster-config.md) §7 |
| — | `spec.profile` | config | [hook-framework](hook-framework.md) §8, EX-5 |
| `cluster.listener` | `spec.listener` | config | `DP-5`, [cluster-config](cluster-config.md) §5 |
| `cluster.management` | `spec.management` | config | [cluster-config](cluster-config.md) §7 |
| — | `spec.membership` | config | **AF-6** ([cluster-config](cluster-config.md) §10 A2) |
| — | `spec.keys` | config | **AF-6**; attributes by [affinity-token](affinity-token.md) §6 |
| — | `spec.shardMap` | config | **AF-6** ([cluster-config](cluster-config.md) §10 A4) |
| `cluster.locationStore` | `spec.locationStore` | config | [location-service](location-service.md) §6.2/§6.3 |
| `cluster.registrar` | `spec.registrar` | config | [location-service](location-service.md), [registrar-auth](registrar-auth.md) |
| `cluster.tenant` | `spec.tenant` | config | [registrar-auth](registrar-auth.md) §2/§4, [location-service](location-service.md) §5 |
| — | `spec.normalisation` | config | [number-normalisation](number-normalisation.md) §3 |
| `cluster.trunk` | `spec.trunk` | config | RT-2, [media-relay](media-relay.md) §13 |
| — | `spec.domain` | config | [extension-framework](../designs/extension-framework.md) G13 |
| `cluster.destinationSet` | `spec.destinationSet` | config | RT-8/RT-9 |
| `cluster.routeRule` | `spec.routeRule` | config | RT-8/RT-9 |
| — | `spec.ingress` | config | RT-8/RT-9 |
| `cluster.rateLimit` | `spec.rateLimit` | config | RT-3 |
| `cluster.timers` | `spec.timers` | config | [proxy-behavior](proxy-behavior.md), RFC 3261 §17 |
| `cluster.security` | `spec.security` | config | [cluster-config](cluster-config.md) §8 V6 + RT-3 |
| — | `spec.admission` | config | [deployment](../designs/deployment.md), `DP-11` |
| `cluster.nat` | `spec.nat` | config | **unowned** — see [deployment](../designs/deployment.md) |
| `cluster.mediaPool` | `spec.mediaPool` | config | KO-7; NG timers by [media-relay](media-relay.md) §8 K6 |
| `cluster.observability` | `spec.observability` | config | DP-3, DP-6, DP-7 |
| `cluster.probe` | `spec.probe` | config | [e2e-probe](e2e-probe.md) §5 |
| — | `spec.echo` | config | [e2e-probe](e2e-probe.md) §9 |

**The Kubernetes-native fields** — the three places the mapping is deliberately *not* a copy, and the
only three:

| `values.yaml` | Where it goes | Why |
|---|---|---|
| `apiVersion` | the resource's own `apiVersion` | G2, G3 — one version field on the thing being versioned, so it is not inside `spec` |
| — | `metadata.name`, from the Helm release name | K5 — the resource's identity follows the release, so an overlay upgrades in place |
| — | the document's `version`, from `metadata.generation` | G4 — only the API server can make it monotonic |

**The chart-local `deployment:` keys** — everything under `deployment:` that reaches no field of this
resource, producing objects Helm creates directly. A `values.yaml` key with no `SipxCluster` field is
either one of these or a defect, and until `KO-15` that sentence was the whole of the mechanism: the
list below is what §4's check reads, one level below each key, in both directions (M7).

| `values.yaml` path | What the chart does with it | Why it is not a field of this resource |
|---|---|---|
| `deployment.operator.replicas` | sizes the operator's own Deployment | the operator is not a cluster node — no section of the document describes it, and it is the thing that reads the document |
| `deployment.operator.resources` | that Deployment's requests and limits | as above |
| `deployment.affinity` | pod affinity for the workloads the chart creates | K3's operator half is closed at four fields, and `nodeSelector`/`tolerations` reached the resource while this did not; whether the operator needs a fifth field is `KO-2`'s question, and adding one changes this spec and §4's three lists together |
| `deployment.postgresql.enabled` | whether the chart stands up a PostgreSQL for the local set | not `cluster.locationStore` in a second spelling: the document says which store a node uses and names its DSN by reference, this says whether the chart creates one for that reference to point at. A deployment with its own database sets it `false` and the document does not change. |
| `deployment.postgresql.storage` | the volume claim size for that PostgreSQL | a property of an object Helm creates, invisible to a node |
| `deployment.rtpengine.image` | the image the managed rtpengine pods run | the pool's *shape*, which the document does not carry: `cluster.mediaPool[]` has no image field and a node never reads one |
| `deployment.rtpengine.replicas` | how many managed rtpengine pods exist | in `mode: managed` the operator owns the workload and therefore the node list (KO-7), so the count has no spelling in the document to disagree with |
| `deployment.rtpengine.hostNetwork` | whether those pods run host-networked | as above — a pod spec field, not a configuration fact |
| `deployment.serviceAccount.create` | whether Helm creates the ServiceAccount and its RBAC | Kubernetes objects the operator runs as, not configuration a node reads |
| `deployment.serviceAccount.name` | the account to use when the chart does not create one | as above |

There is deliberately **no `deployment.rtpengine.enabled`** (`KO-15`). Whether this deployment runs
its own rtpengine is `cluster.mediaPool[].mode: managed` — the configuration document's own answer,
carried verbatim into `spec.mediaPool` — and the chart derives the workload's existence from it in
one place, `templates/_helpers.tpl`'s `sipx-clstr.mediaPool.managed`. The direction is K1 and K4's:
the document decides, the chart reads. The `enabled` boolean was the other direction and was never
derived from anything, so an operator who flipped the mode got a pool the chart did not create and no
error anywhere. `cluster.probe` has had this shape since `KO-14` for the same reason — the probe runs
iff a node runs the `e2e-tester` role, and a second switch would be a way for the two to disagree.

## 6. Admission: the whole document, plus what one node cannot see

| # | Rule |
|---|---|
| A1 | **Admission runs [cluster-config](cluster-config.md) §8 in full, unchanged, and adds the cluster-scope rules below.** It is not a second validator with its own opinions: the same `ConfigError` set, the same `path` spellings, the same rule ids, the same "every error, not the first" and the same `path` ordering (V1). A rule that admission checked differently from the loader would be a cluster that installs and does not start, which is the failure mode an operator cannot debug. |
| A2 | **Rejection is total.** A rejected create produces no object; a rejected update changes nothing at all — not `status`, not one managed object — and the last accepted generation keeps running. This is §8 V10 one level up: refusing is the only failure mode, and there is no partial application. |
| A3 | **A second `SipxCluster` in a namespace is rejected**, naming the existing resource (K6). |
| A4 | **A zone with no `edge` is rejected**, naming the zone. Every zone in `spec.zones` MUST have at least one `edge` workload — a zone whose declared listeners no `edge` serves has no front door, and its calls fail at the first request rather than at install. **This is why admission exists**: §8 validates one *projection* (P2), and a node in zone `a` cannot see that zone `b` has no edge. The rule is unrepresentable in the loader, not merely unimplemented there. |
| A5 | **A declared role with no listener is rejected**, naming the role. Every role with `replicas` ≥ 1 in `spec.roles` MUST be named by at least one `spec.listener[].roles`, or, for `e2e-tester`/`echo`, by the listener that role needs. §5 P4 refuses exactly this at start-up — but only once the workload exists, as a pod that will not boot; admission is where it is a message about a field. The chart's proxy, `echo` and `e2e-tester` workloads were all in this state at one point, and nothing found it until a document was fed to a loader by hand. |
| A6 | **A media pool that cannot be operated is rejected**, naming the pool: `mode: managed` with no `portRange` (the operator would create rtpengine pods with no RTP range to advertise), and `mode: external` with no `nodes` (a pool that is declared-not-managed and declares nothing). The pool's fields are KO-7's; what is here is only that both modes need enough to *be* a pool. |
| A7 | **A profile incompatible with the declared role set is rejected**, naming the profile and the role. A profile is a module set plus role flags ([hook-framework](hook-framework.md) §8), so a profile whose flags do not cover a role in `spec.roles` is a workload whose decision paths would not be wired. EX-5 owns which combinations are compatible; admission owns that the question is asked before a pod exists. |
| A8 | **A profile the operator cannot resolve is rejected**, naming the profile and the catalog it was looked up in. [hook-framework](hook-framework.md) §7's G1–G6 refuse an unresolvable profile at node startup; the same refusal at admission is the difference between a named field and a crash loop. |
| A9 | **`spec.roles` is checked against the closed role set and R6**: a key outside [cluster-config](cluster-config.md) §4 R1's six names is rejected listing the six (`proxy` is the chart's word, not a role), and a workload set that would put `echo` in one process with a call-path role is rejected citing [e2e-probe](e2e-probe.md) §11. R6 is per *process*, so at admission it is a statement about how workloads are composed rather than about the document. |
| A10 | **Admission never reads cluster state.** It validates the resource, not the fleet: node counts, image availability, whether a `nodeSelector` matches anything, whether the PostgreSQL DSN resolves. Those become `status` conditions (§7), because they are observations that can change after the resource is accepted and a webhook that failed on them would make an unrelated node outage look like an invalid configuration. Replica-versus-node arithmetic is `KO-11`'s, at reconcile, for exactly this reason. |

## 7. `status`: what is observed, never what was asked for

| # | Rule |
|---|---|
| S1 | **`status` reports observation only.** Every field is derived from what the operator has seen — pods, node reports, probe runs — and never from `spec`. The failure this forbids is the one that makes status worthless: reading a ready count back out of the declared replica count, so that a cluster which deployed nothing reports exactly what was asked for. |
| S2 | **Conditions carry the standard shape** — `type`, `status` (`True`/`False`/`Unknown`), `reason` (a `CamelCase` token), `message` (naming the offending field or node), `lastTransitionTime`, `observedGeneration` — and the four types below are the complete set. `readyReplicas` per role sits beside them as counts, because a count is not a condition. |
| S3 | **`Ready` is the conjunction and it is not a summary.** `Ready: True` requires every other condition `True`, every role at its declared replica count, and the last probe verdict `Pass`. Anything else is `False` or `Unknown` with the first unmet reason named. A `Ready` that could be `True` while a probe fails would be the one field an operator checks and the one field that lies. |
| S4 | **Silence is `Unknown`, never `True`.** A condition whose inputs have not been observed within the operator's own reporting interval is `Unknown` with the node or object named, because "we have not heard" and "it is fine" are different facts and only one of them is a reason not to page. |
| S5 | **`observedGeneration` is the generation `status` describes**, and it advances only when a change has fully applied. For a staged change ([cluster-config](cluster-config.md) §3 D11 — the shard-map handoff is the only one) the previous generation stays observed until the last shard settles, which is precisely what `ShardMapConverged` reports. `observedShardMap` is the map the fleet is *using*, so during a handoff it is still map `n` while `spec.shardMap` is `n+1`. |
| S6 | **`KeysDistributed` asserts distribution, not intent** ([affinity-token](affinity-token.md) §6 K1, [cluster-config](cluster-config.md) §9.3 RL10): `True` only when every node that verifies tokens holds every key in `spec.keys`, naming the key id and the lagging nodes while it is `False`. It is the condition that makes two-phase rotation observable: the operator MUST NOT activate a mint key while this is `False`, so a token can never be minted under a key some healthy node cannot verify. |
| S7 | **`ProfileCompatible` is reported from the nodes, not from admission.** §6 A7/A8 refuse an incompatible or unresolvable profile before anything is created; this condition is the running fleet's agreement — `True` when every node that consumes the profile has resolved it, `False` naming the node and the rule that refused it. The two are not redundant: admission judges the resource, this judges the image actually running, and a profile that validates in the webhook and fails in a pod is an image-version skew nothing else would show. |
| S8 | **`lastProbe` carries the verdict and its step** — [e2e-probe](e2e-probe.md) §6's `Pass` or `Fail { step, … }`, with the run id and time, so a verdict can be traced to a run record rather than believed. A failing probe makes `Ready` `False` (S3) and **nothing else happens**: no rollback, no restart, no scale change. Reacting to a probe verdict is `KO-8`'s pause and `KO-5`'s invariant gate, and an operator that rolled back on a red probe would turn a downstream outage into a deployment loop. |
| S9 | **Printer columns are the conditions an operator reads first**: `Ready`, per-role ready counts, `observedGeneration`, last probe verdict. Named here so that they are part of the contract rather than a manifest detail — a resource whose `kubectl get` output omits the verdict is one whose health has to be asked for. |

| Condition | `True` when | `False` / `Unknown` when |
|---|---|---|
| `Ready` | every condition below is `True`, every role is at its declared count, and `lastProbe` is `Pass` (S3) | the first unmet reason, named; `Unknown` while any input is unobserved (S4) |
| `ProfileCompatible` | every node consuming `spec.profile` has resolved it (S7) | a node refused it — the node and the rule are named |
| `ShardMapConverged` | `observedShardMap` equals `spec.shardMap` and no shard is draining ([cluster-config](cluster-config.md) §3 D11, §9.4) | a handoff is in flight, naming the shards; a forced switch is reported and counted (DS5) |
| `KeysDistributed` | every verifying node holds every key in `spec.keys` (S6) | a key is missing somewhere — the key id and the nodes are named |

## 8. Versioning, conversion and the upgrade policy

| # | Rule |
|---|---|
| U1 | **The CRD's served versions are exactly the schema versions the operator implements**, and the storage version is the newest of them. Because G3 makes them one string, "which schema versions does this build speak" has a single answer that `kubectl get crd` shows. |
| U2 | **Within one version, change is additive only** ([cluster-config](cluster-config.md) §3 D7): a new field with a declared default may be added. Removing, renaming, narrowing a type, changing a default, or changing what a value means requires a **new** version. This binds the CRD schema and the document schema together, because they are the same schema. |
| U3 | **`v1alpha1` carries no compatibility promise** (D8). It may break in any release, and this spec says so in the same breath as pinning it, because an alpha treated as stable is how a schema acquires fields nobody can remove. What *is* promised even in alpha is the refusal: a node never best-effort-parses a version it does not implement (G3, D6). |
| U4 | **A second served version requires a conversion webhook**, and `conversionStrategy: None` is correct only while exactly one version is served. Conversion is the operator's (D7 says so and names this story): it is a total function in both directions between served versions, and a field a conversion cannot round-trip is a field that needed a new version rather than a converter. |
| U5 | **The operator and the CRD upgrade in one direction: schema first.** The CRD's new version is served before any node runs an image that writes it, and the old version stays served until no node reads it — the same two-phase shape as key rotation (§7 S6), for the same reason. A cluster mid-upgrade holds two images; a schema that arrived after the image that needs it makes half of them refuse to start. |
| U6 | **Deleting the resource does not drain anything.** Deletion is not a lifecycle event this spec sequences: `KO-4` owns drain, and a `SipxCluster` removed is a deployment removed. Said out loud because the opposite is a reasonable expectation, and an operator who expects a graceful teardown from `kubectl delete` should get the answer from a spec rather than from an incident. |

## 9. What is deliberately not expressible

| Not expressible | Why, and where it belongs |
|---|---|
| A configuration field that is not a §7 section | K2, M1. The moment the resource has a field of its own, there are two schemas and the epic's premise is gone. |
| A per-node or per-zone override of a configuration section | [cluster-config](cluster-config.md) P1/R3 and §11. The document is the same bytes everywhere; a per-zone override would make "the configuration version" stop naming a thing. Zone-shaped differences are expressed as zone-scoped *data* inside a section, by that section's owner. |
| A `version` or `apiVersion` inside `spec` | G4, G5. Both are Kubernetes-native here, and a copy of either is free to disagree with the original. |
| Templating, `${}` substitution over `spec`, or any expression | [cluster-config](cluster-config.md) D5. §8 V4's `${NAME}` is resolved by the *node*, from `load`'s `env`, against the downward API — it is a value in the document, not a computation the operator performs. |
| A field the chart computes that the resource cannot hold | Acceptance of this story, and §5's last paragraph. A chart that derives a value is a chart with an opinion, and the opinion is then unreviewable in `kubectl get -o yaml`. |
| An admission rule that reads cluster state | A10. A webhook that fails on an observation makes an outage look like an invalid document. |
| A `status` field derived from `spec` | S1. It is the one class of status defect that survives every test, because it always agrees with what was asked for. |
| A knob for what admission checks | A1. Admission is §8 plus §6; a deployment that could relax it would be a deployment that installs a document its nodes refuse. |

## 10. Test vectors

Every row is normative and executes against the operator's validating webhook or its status
reconciliation, given a `SipxCluster` and — for the `SC-S` family — an observed fleet. Both families
need code that does not exist yet: the webhook and the reconcile loop are
[KO-3](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/KO-3-implement-the-operator-reconcile-loop.md)'s,
and every row below is deferred to it in
[vector-scope.toml](../reference/vector-scope.toml) with that reason. Deferral rather than a weaker
row: a vector that asserted what a *document* says would be a test of this file, and the rows exist
to test the operator.

**Admission (`SC-A`).** The verdict on a resource, before any object exists.

| # | Given | Expect |
|---|---|---|
| SC-A-1 | `spec.registrar.usePathh: true` | Rejected with the errors §8 itself produces — `E(cluster.registrar.usePathh, CC-V2)` naming the recognised keys — in `path` order, and nothing is created (A1, A2) |
| SC-A-2 | `spec.zones: [a, b]`; `spec.roles` places `edge` only in zone `a` | Rejected, naming zone `b` (A4). No projection could have found this |
| SC-A-3 | `spec.roles` declares `inbound-proxy`, and no `spec.listener[]` entry names that role | Rejected, naming the role (A5) — not a pod that fails `P4` after being created |
| SC-A-4 | `spec.mediaPool[0]` with `mode: managed` and no `portRange` | Rejected, naming the pool (A6) |
| SC-A-5 | `spec.profile: modern-registrar` while `spec.roles` declares `outbound-proxy` | Rejected, naming the profile and the role (A7, [hook-framework](hook-framework.md) §8's role flags) |
| SC-A-6 | `spec.profile` naming no profile in the operator's catalog | Rejected, naming the profile and the catalog (A8) |
| SC-A-7 | `spec.roles` carries a key `proxy` | Rejected, listing [cluster-config](cluster-config.md) §4 R1's six role names (A9) |
| SC-A-8 | A second `SipxCluster` created in a namespace that already holds one | Rejected, naming the existing resource (A3, K6) |
| SC-A-9 | An update that A4 rejects, against a resource already reconciling | Rejected; `status` and every managed object are untouched, and the accepted generation keeps running (A2) |

**Status (`SC-S`).** What the fleet is observed to be, never what was asked for.

| # | Given | Expect |
|---|---|---|
| SC-S-1 | A resource just accepted; no pod ready yet | `Ready: False`; every role's ready count is the observed one and not the declared one (S1, S3) |
| SC-S-2 | Every role at its declared count, last probe `Pass` | `Ready: True`; `lastProbe.verdict` is `Pass` with its run id (S3, S8) |
| SC-S-3 | A pod running but reporting nothing within the operator's interval | The condition it feeds is `Unknown`, naming the node — never `True` by omission (S4) |
| SC-S-4 | `spec.shardMap` advanced to `n+1`; one shard still draining | `ShardMapConverged: False` naming the shard; `observedShardMap` is still `n`, and `observedGeneration` stays behind `metadata.generation` (S5, [cluster-config](cluster-config.md) §3 D11) |
| SC-S-5 | A key added with `mint: false`, not yet held by every verifying node | `KeysDistributed: False` naming the key id and the lagging nodes; `True` only once every one holds it (S6, [affinity-token](affinity-token.md) §6 K1) |
| SC-S-6 | The last probe run is `Fail { step: Invite }` | `lastProbe.verdict` is `Fail` with the step; `Ready: False`; no rollback, no restart, no scale change (S8) |
| SC-S-7 | A node refuses `spec.profile` that admission accepted | `ProfileCompatible: False` naming the node and the refusing rule — an image skew nothing else surfaces (S7) |

## 11. Consequences for documents this spec does not own

Named here so that they are tracked rather than discovered. None is performed by this story.

| Where | What must change, and why |
|---|---|
| `deploy/helm/templates/sipxcluster.yaml` (`KO-2`) | `apiVersion` may become a template constant now that G1 pins it. It stays a values key today because that is the spelling `scripts/check-crd-drift.py` compares against the loader's constant; constantising it means moving the comparison, not deleting it. |
| `deploy/helm/values.yaml` (`KO-2`, `KO-7`) | **Done — `KO-15`.** `deployment.rtpengine.enabled` was a second spelling of `cluster.mediaPool[].mode: managed`: its own comment said it mirrored that field, nothing made it do so, and setting it `false` beside a `managed` pool passed the whole gate. It is gone rather than reconciled — a rule that refuses disagreement still leaves two places to write one fact. The mode is now the only switch, the chart derives the workload from it in `templates/_helpers.tpl` (`sipx-clstr.mediaPool.managed`, which `KO-2`'s workload guards on), and §5's chart-local table declares what is left of `deployment.rtpengine` — image, replicas, host networking — none of which the document carries. `deployment.postgresql.enabled` stayed, declared in that table with its reason: which store a node uses and whether Helm creates one to point at are separable facts, and a deployment with an external database changes only the second. |
| [k8s-deployment-operator](../designs/k8s-deployment-operator.md) | Its `SipxCluster` field list predates seven §7 sections (`security`, `rateLimit`, `normalisation`, `domain`, `ingress`, `admission`, `observability`) and its role list omits `echo`, which [cluster-config](cluster-config.md) §13 already records. §5 here is the current list; the design's paragraph is narrative and should cite rather than enumerate. |
| `KO-2`'s story record | Its Progress says the API version is "held in `values.crd`". `KO-14` removed that block — it is the top-level `apiVersion` key — and the group and version are no longer provisional. |
| `docs/reference/vector-scope.toml` | The sixteen `SC-*` rows of §10 need their deferrals to `KO-3`; until they are there, `scripts/check-vectors.py` reports every one as a spec row nothing covers. |
