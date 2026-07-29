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
