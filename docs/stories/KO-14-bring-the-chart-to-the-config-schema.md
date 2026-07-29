---
id: KO-14
title: Bring the chart's values to the config schema, starting with the media block that cannot boot
pillar: Cluster
status: ready
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
- [ ] **`cluster.media` is fixed first, because as shipped the default deployment set cannot start.**
      It declares `codecs.offer: [PCMA, PCMU, telephone-event]` with `mask: all`, which is not the
      identity policy `{ AsReceived, None, Disabled }`, and
      [media-relay](../specs/media-relay.md) `G-M6` refuses to start on any other policy while
      `MP12` holds. Media policy is also **per trunk** (`MP6`), not a global block, and
      `offer`/`strip`/`mask` are §13.3's **NG wire keys** — protocol in a configuration file.
- [ ] Every other row of the reconciliation table in
      [deployment](../designs/deployment.md) is applied or explicitly rejected with a reason:
      `numbering` → named `normalisation` profiles bound per scope and per trunk; per-tenant
      registrar values → `tenant[]`; `shards` → `shardMap`; top-level `quirkProfile` → bindings on
      `trunk[]`/`domain[]`; the `management` listener out of `listener[]`; `limit`/`limits` →
      `rateLimit[]`/`timers`.
- [ ] `security.maxForwards` is **70**, not 10. RFC 3261 §16.6 step 3 makes it the value inserted
      when a request carries none — not a hop budget, so 10 silently shortens every call path that
      arrives without the header.
- [ ] The chart comment claiming Timer C's "180s default silently caps long calls" is corrected or
      removed. Timer C cannot cap an established call: it is cancelled by the final response
      (RFC 3261 §16.6 step 11). The cap belongs to `maxCallDuration`.
- [ ] `helm lint` and `helm template` still pass, and the rendered `SipxCluster` validates against
      the schema — or the story records why it cannot yet, naming `KO-1`.

## Progress
- (not started)

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
