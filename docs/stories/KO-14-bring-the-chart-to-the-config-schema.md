---
id: KO-14
title: Bring the chart's values to the config schema, starting with the media block that cannot boot
pillar: Cluster
status: done
priority: 1
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: DP-1 found the shipped default set declares a media policy G-M6 refuses to start on
---

# Bring the chart's values to the config schema, starting with the media block that cannot boot

## Goal
Make `deploy/helm/values.yaml` express [cluster-config](../specs/cluster-config.md), which is now
the normative schema. The chart's header calls its `cluster:` tree "DP-1's config schema, verbatim —
the same tree a node reads"; `DP-1` has now written that schema, and the two do not agree.

## Acceptance
- [x] **`cluster.media` is fixed first, because as shipped the default deployment set cannot start.**
      It declares `codecs.offer: [PCMA, PCMU, telephone-event]` with `mask: all`, which is not the
      identity policy `{ AsReceived, None, Disabled }`, and
      [media-relay](../specs/media-relay.md) `G-M6` refuses to start on any other policy while
      `MP12` holds. Media policy is also **per trunk** (`MP6`), not a global block, and
      `offer`/`strip`/`mask` are §13.3's **NG wire keys** — protocol in a configuration file.
- [x] Every other row of the reconciliation table in
      [deployment](../designs/deployment.md) is applied or explicitly rejected with a reason:
      `numbering` → named `normalisation` profiles bound per scope and per trunk; per-tenant
      registrar values → `tenant[]`; `shards` → `shardMap`; top-level `quirkProfile` → bindings on
      `trunk[]`/`domain[]`; the `management` listener out of `listener[]`; `limit`/`limits` →
      `rateLimit[]`/`timers`.
- [x] `security.maxForwards` is **70**, not 10. RFC 3261 §16.6 step 3 makes it the value inserted
      when a request carries none — not a hop budget, so 10 silently shortens every call path that
      arrives without the header.
- [x] The chart comment claiming Timer C's "180s default silently caps long calls" is corrected or
      removed. Timer C cannot cap an established call: it is cancelled by the final response
      (RFC 3261 §16.6 step 11). The cap belongs to `maxCallDuration`.
- [x] `helm lint` and `helm template` still pass, and the rendered `SipxCluster` validates against
      the schema — or the story records why it cannot yet, naming `KO-1`. **Both, split:** the
      configuration document inside the resource is validated by the real loader
      (`deploy/helm/check-values.sh`, below); the resource's *Kubernetes* surface is not, because
      `KO-1` has not pinned the CRD and there is no schema to validate it against.

## Progress

**Done (KO-14).** The `cluster:` tree is the document `cluster-config` specifies, and the claim is
now mechanical rather than asserted.

- **The check that makes it mechanical:** `deploy/helm/check-values.sh` runs `helm lint`, renders the
  chart, turns the rendered `SipxCluster` spec back into a node document
  (`deploy/helm/node-document.py` — subtract the operator's four keys, add §3's two version fields)
  and loads it through `sipx-clstr run` once per role the default set deploys. Before this story it
  reported **22 problems** and every one of the six roles was refused; after it, none is refused and
  an `edge` node reaches `node listening … tenant=default store="in-memory" auth="open"` and
  `admission bound max_in_flight_transactions=1024`.
- It is **not wired into `scripts/gate.sh`**, because `scripts/**` was outside this story's write
  set. That is the one thing left for the note above ("wire the render through `config::load` in the
  gate and the list above becomes self-maintaining") to be true: one `step "helm"` line calling
  `deploy/helm/check-values.sh`, guarded on `command -v helm`.
- **`cluster.media` → `trunk[].media` + `mediaPool[]`** (`values.yaml:353`, `:374`). The per-trunk
  policy is the identity policy and only that — `codecs: as-received`, `transcode: none`,
  `srtp: disabled` — which is what `media-relay` MP12/G-M6 admits while `CF-3` is open. `offer`,
  `strip`, `mask`, `rewrite`, `timeout`, `silentTimeout`, `strictSource` and `generateRtcp` are gone:
  the first three are §13.3's NG wire keys and the rest are the relay's own operation, not a platform
  surface. `ngTimeout`/`ngRetries` have no replacement — §8 K6's seven timers are adopted with their
  own defaults (§8 V3 forbids a second one) and the four-attempt retransmission schedule is not a
  knob.
- **Reconciliation-table rows applied:** `numbering` → removed (see below); `listener[].role` →
  `roles: []`; the `management` listener → its own `management:` section; `registrar.*` → `tenant[]`
  (`expiry` in seconds, `maxBindingsPerAor`), leaving §7's two keys; `registrar.shards` → removed;
  `quirkProfile` → bindings on `trunk[]`/`domain[]`; `limit[]` → `rateLimit[]`; `limits` → `timers`
  with RFC 3261 §17 names; `security.maxForwards` → gone (`CC-V6`); `security.rejectUserAgents` →
  `userAgentDenyList: []`; `crd.{apiGroup,apiVersion}` → one `apiVersion:`; `routingHook`,
  `answerOptionsKeepalive` and `anonymousCalls` → removed as `CC-V2` unknowns; `${NODE_IP}` →
  `${POD_IP}`, which is the name the working manifest already supplies from the downward API.
- **Rows deliberately *not* applied, each with its reason in the file:** `normalisation` declares no
  profile, because `number-normalisation` N27 gives the platform no default and the `ingress[]` scope
  vocabulary is RT-8/RT-9's, so the shape is a comment rather than a shipped profile; `profile:` has
  a home and no value, because `EX-5` has not shipped the catalog a name would come from and
  `hook-framework` §7 refuses a profile it cannot resolve; `membership`, `keys` and `shardMap` are
  written by nobody here, because they are `AF-6`'s fields and their node ids are assigned by
  whatever creates the workloads (§5 P1 — `KO-3`); `anonymousFrom`/`anonymousCalls` are RFC 5379
  privacy policy, which is RT-7's and has no section yet.
- **Beyond the table, and needed for the default set to be startable at all:** every role the
  deployment set runs now has a listener, because a projected node with none is refused by §5 P4 —
  the `inbound-proxy`, `outbound-proxy`, `e2e-tester` and `echo` workloads had none. `echo` is a
  `deployment.roles` entry for the same reason: it is a role (§4 R1, `e2e-probe` §9) and the echo
  trunk pointed at a workload nothing created. `proxy` → `inbound-proxy` + `outbound-proxy` and
  `e2eTester` → `e2e-tester`, since neither of the old two was a role name.
- **Two things fixed that are not load errors**, both flagged in the Notes above: the RTP range moved
  to 16384-16584, off Kubernetes' default NodePort range, which a host-networked relay was
  colliding with; and `routeRule[0].destination`/`trunk[0].pool` said `pool-echo` while the only
  declared `destinationSet` is `echo`, so both were dangling references.
- `connectionLifetime` and `maxConnections` were **dropped rather than plumbed** — plumbing them is
  `crates/` work and outside this story's write set. They are on the loader's allow-list and are
  reported as unapplied, so shipping them in the default set was configuration nobody enforces.
- `authBackend: location_store` is gone with the rest of the old `registrar:` block, and no `auth:`
  is shipped: this build refuses to start when a tenant's `secretRef` resolves, because `RG-7` has
  not landed a credential store, so the local set runs an open tenant deliberately and says so.
- **Considered for upstream: no.** A Helm chart's values are deployment orchestration for this
  platform's own concepts — roles, zones, tenants, trunks, media pools — and the kernel has no
  opinion about any of them. Nothing joins [the ledger](../upstream.md).
- **Left for other stories:** `nat:` is unchanged except its duration unit (it has no owning spec and
  wants its own story); `timers.maxCallDuration`, `locationStore.ha` and `listener[].tls` are
  accepted by the loader and read by nothing, which is `FC-3`/fail-closed-config's class rather than
  the chart's; and `cluster-config` §5 P6 (two projected listeners of one transport) is specified but
  not implemented, so a node given `--roles edge,registrar` loads a projection the spec refuses.

## Notes
- Filed by `DP-1`, which was told the chart was read-only and correctly reported rather than
  edited. The reconciliation table it produced in
  [deployment](../designs/deployment.md) is the work list; this story is applying it.
- The media item is the one that is not merely tidy-up. It is latent rather than active only
  because `ME-2` has not implemented the loader — the configuration is already wrong, and the first
  thing that reads it properly will refuse to start. That is `MP12` working as designed: it exists
  so an unrecognised media key cannot silently mean clear-text media on a leg whose policy said
  encrypted.
- `nat:` has no owning spec at all. `DP-1` gave it a home and a reload class and nothing more, and
  its field names came from somewhere other than an RFC. That wants its own story rather than being
  smuggled in here.
- **The divergence is now measurable rather than asserted, because `DP-8` shipped the loader.** A
  review diffed the chart's `cluster:` tree against `config::load`'s allow-lists and counted roughly
  eighteen hard load errors, which is worth having as a starting work list:
  - Six unknown `cluster.*` keys → `CC-V2`: `numbering`, `routingHook`, `quirkProfile`, `media`,
    `limit`, `limits`. Three of them are the registry's own concepts under a different spelling —
    `normalisation`, `mediaPool`, `rateLimit` — so they are typos against the schema rather than
    deferred sections.
  - Four in `cluster.security`, whose allow-list is `unknownSource`, `sanityCheck`,
    `userAgentDenyList`, `internalZone`: `maxForwards` hits the dedicated `CC-V6` refusal (which is
    the acceptance item above, now with a rule id), plus `rejectUserAgents`,
    `answerOptionsKeepalive`, `anonymousCalls` → `CC-V2`.
  - Eight across `listener[]`: all four entries spell it `role:` where the allow-list is `roles`, so
    each yields `CC-V2` for the unknown key **and** `CC-R2` for the missing one. `listener[3]` also
    names `role: management` and `transport: http`, and `management` is not in §4 R1's closed role set.
  - `cluster.membership` and `cluster.tenant` are absent entirely, while `probe.tenant: e2e-test`
    names a tenant no section declares.
  - Two role keys in `values.yaml` are not roles: `proxy` (the set is `inbound-proxy` /
    `outbound-proxy`) and `e2eTester` (§2 D4's spelling is `e2e-tester`). The design doc lists them
    correctly, so the chart disagrees with its own design record.
  - `${NODE_IP}` is used in four places and defined nowhere; `substitute` treats an undefined name as
    `CC-V4` deliberately rather than substituting empty, so it would refuse. The working manifest uses
    `${POD_IP}` and supplies it from the downward API.
  - `listener[].connectionLifetime` and `maxConnections` are *on* the allow-list but never read into
    `ListenerSpec` — accepted and discarded, which is the class
    [fail-closed-config](../designs/fail-closed-config.md) rule 1 exists to remove. If this story
    plumbs them, say so; if not, they should be refused rather than ignored.
- **Nothing mechanically checks any of this**, which is why it accumulated: there is no `helm` or
  `docker` job in CI, so neither `helm template` nor a load of the rendered tree has ever run. The
  last acceptance item is therefore the load-bearing one — wire the render through `config::load` in
  the gate and the list above becomes self-maintaining.
- Also worth fixing while in `values.yaml`, though neither is a load error: `registrar.authBackend:
  location_store` reads as "authentication is configured" to anyone editing the file, and `registrar`
  is a deferred section that nothing descends into (`FC-3` owns making that true or refused); and the
  default RTP `portRange: 30000-30200` under `rtpengine.hostNetwork: true` overlaps Kubernetes'
  default NodePort range 30000-32767 on the same host ports.
