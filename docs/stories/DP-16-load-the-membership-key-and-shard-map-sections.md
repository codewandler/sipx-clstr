---
id: DP-16
title: Load the membership, key and shard-map sections the config loader still refuses
pillar: Cluster
status: ready
priority: 1
design: docs/designs/deployment.md
epic: fail-closed-config
areas: [config, node, affinity]
note: AF-6 specified them and DP-8 is closed, so nothing owns loading them — a document written to cluster-membership.md will not start a node
---

# Load the membership, key and shard-map sections the config loader still refuses

## Goal
Make the cluster document's `membership[]`, `keys[]` and `shardMap` sections load and apply, so a
document written to `cluster-membership.md` starts a node instead of being refused.

## Acceptance
- [ ] `membership[]` accepts the fields `cluster-membership` §3 defines — the loader's closed world is
      `["node", "name", "zone", "roles"]` today (`crates/sipx-clstr-node/src/config/mod.rs:1141`), so
      `rpc` and `incarnationSource` are `V2` errors.
- [ ] `keys[]` and `shardMap` leave `DEFERRED_SECTIONS` (`config/mod.rs:412`) and are validated per
      §4 `KY1`–`KY9` and §5 `SM1`–`SM5`, including `KY3`'s reserved-and-always-refused `secret`.
- [ ] Reload without restart holds as §6 `RD1` states it: no listener rebound, no connection closed,
      no registration expired, no token or reference invalidated, no dialog or in-flight transaction
      disturbed.
- [ ] The `cluster-config` §12 rows that execute these rules are **proved, and their deferrals
      re-pointed here from `DP-8`** — `CC-K-1`…`CC-K-6` (key reload), `CC-S-1`…`CC-S-9` (shard-map
      handoff), `CC-V-4`/`CC-V-10`, `CC-R-7`/`CC-R-8`, `CC-I-1`/`CC-I-4`.
- [ ] **Failing-first:** a document containing a `cluster-membership` §3 `rpc` entry is refused today
      and accepted after. Demonstrate the refusal at the merge base.
- [ ] `deploy/devspace/manifests/node.yaml`, `website/docs/reference/configuration.md`,
      `scripts/two-node-call.sh` and `scripts/e2e-call.sh` all declare `edge`+`registrar` members with
      no `rpc`; `MB5` will invalidate every one once enforced. Update them, or state why `MB5` should
      not apply to them.

## Progress
- (not started)

## Notes
- **Filed because nothing owned this.** `AF-6` wrote `cluster-membership.md` §3–§5 and its §12 says the
  loader gains the fields "until the implementing story adds the fields" — **without naming one** — and
  `DP-8`, which owns every relevant vector row, is `status: done`. That is the shape `CF-24` was filed
  for, and filing this story is how it is not repeated.
- **This is spec-before-code working as intended, and it has a sharp edge.** A document written from
  `cluster-membership` §3 today will not start a node: `rpc` and `incarnationSource` are refused by the
  closed world, and `keys`/`shardMap` sit in `DEFERRED_SECTIONS`. `AF-6` records this in §12 and it is
  correct — but it means the published spec currently describes a document the binary rejects. First
  thing to check on a report of "config refused".
- **The `MB5` blast radius is the part most likely to bite.** `MB5` requires `rpc` when `roles`
  intersects `{edge, registrar, inbound-proxy, outbound-proxy}`. Four in-tree documents declare exactly
  that with no `rpc`, including two proof scripts and the published configuration reference. They are
  harmless while the closed world refuses the field; the day it accepts it, `MB5` makes all four
  invalid. Land the fix and the documents together.
- `MB5` is a deliberate over-approximation: the precise property is "this node may accept a
  connection-oriented transport", which is a *listener* fact rather than a role fact, so a UDP-only
  proxy is made to declare an endpoint it never uses. Reviewed as erring safe — `affinity-token` §11.4
  `FM6` means a UDP-only edge owns no flows — but if this story finds the approximation costly, `MB5`
  is the rule to revisit rather than the loader.
- **`AF-4` has landed the token library**, so `keys[]` has a real consumer: `KY1` freezes the six
  attributes and binds a change to a new `apiVersion`. Do not alter that interface here.
- Considered for upstream: **no.** Loading this platform's cluster document is orchestration; the
  kernel has no notion of our membership.
