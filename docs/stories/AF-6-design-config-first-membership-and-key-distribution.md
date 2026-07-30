---
id: AF-6
title: Design config-first membership and key distribution
pillar: Cluster
status: done
priority: 
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, deploy]
note: specified; the ten cluster-config §12 rows that test reload are DP-16's, re-pointed off the closed DP-8
---

# Design config-first membership and key distribution

## Goal
Keep v1 free of consensus: the node set, shard map and token keys come from validated, reloadable configuration.

## Acceptance
- [x] This story owns the membership/key schema sections; DP-1 integrates them unchanged into 
the full config schema (AF-6 first, DP-1 second — the circular reference is resolved this way).
- [x] Reload without restart is specified and tested; the key-rotation runbook (overlap window, cutover) is documented.
- [x] The design records what a future dynamic membership service would replace, so nothing here paints it out.

## Progress
- **Closed 2026-07-30 with item 2's split recorded rather than left open.** "Specified **and tested**"
  spans two stories: the specification is §6 `RD1`–`RD8` and is done here; the ten `cluster-config`
  §12 rows that *execute* those rules — `CC-K-1`…`CC-K-6`, `CC-S-1`…`CC-S-9`, `CC-V-4`/`CC-V-10`,
  `CC-R-7`/`CC-R-8`, `CC-I-1`/`CC-I-4` — were every one deferred to `DP-8`, which is **closed**.
  Chasing that is what surfaced `CF-24`: **239 of 428 deferred rows name a story that has already
  closed.** `DP-16` now owns the loading work and those deferrals are re-pointed at it.
- **Independent review: `PASS`**, with all eight flagged judgement calls checked and upheld. `KY3` is
  *forced* rather than merely defensible — `CC-V-10` already expects
  `E(cluster.keys[0].secret, CC-V9)`, so a closed world not recognising `secret` would have to cite
  `CC-V2` and fail an existing normative row. Table integrity verified across all 18 pipe-blocks
  (`CF-23`'s trap), fence clean, and the vectors line byte-identical to `main` — this story adds no
  coverage and does not appear to.
- **Two corrections applied at review, and the second was in three places, not one.** The `L` row
  restated `affinity-token` §7 `M5`'s floor *narrower* than its owner — a `cluster-config` §8 `V3`
  violation — and now cites M5 rather than paraphrasing it. The claim that `cluster-config` §1–§9
  were unchanged was false and appeared in the preamble, in §12's own consequences row, and in the
  §10 lead-in this story wrote; all three now say *no rule* of §1–§9 moved and name the three
  pointers that did.
- **`KY3` gained a redaction line, and the finding behind it made `FC-8` much smaller.** The loader
  **already** redacts, twice, against no written rule — `config/mod.rs:1278` emits
  `Some("an inline DSN")` and `:1431` `Some("an inline nonce secret")`. So the rule exists in code
  and in no spec; `FC-8` documents it in `cluster-config` §8 beside `V9`, where a property of
  `ConfigError` belongs, and must permit "describe **or say nothing**" because `:1288` correctly
  passes `None` for the *missing* case.


**Pass 1 — the specification is in; the "tested" half of the second item is not.**

- **What landed.** [cluster-membership](../specs/cluster-membership.md), normative: §3 `membership`,
  §4 `keys`, §5 `shardMap` (fields, forms, requiredness, defaults and the rules that refuse a bad
  one), §6 reload, §7 the runbook, §8 uniqueness, §9 the dynamic-membership successor, §10 what is
  not expressible, §11 vectors, §12 consequences. [cluster-affinity](../designs/cluster-affinity.md)
  gained a *Settled in AF-6* block and two risk rows moved from open to settled.
- **The circular reference, resolved as the story asks.** `cluster-config` §10 A6 said AF-6 writes
  the sections' fields into §7's rows and its own document, and that nothing in §1–§9 changes when
  it does. That is exactly what happened: three §7 owner cells and the §10 lead-in now name
  `cluster-membership`, the §1 out-of-scope sentence points at it, and **no rule of §1–§9 moved**.
  `check-crd-drift.py` is green, which is the mechanical half of "the registry did not change" — it
  reads §7's first cells, and those are untouched.
- **Reload without restart is specified** — §6 RD1 states the property as one testable sentence
  (no listener rebound, no connection closed, no registration expired, no token or reference
  invalidated, no dialog or in-flight transaction disturbed), and RD2–RD8 carry the transitions
  that are this spec's. §11 names the ten `cluster-config` §12 rows that execute these rules today.
- **Not tested, and why the box stays unticked.** Every one of those rows is deferred to `DP-8` in
  [vector-scope.toml](../reference/vector-scope.toml) because the loader's reload half has no test.
  Covering one needs a Rust test under `crates/**` or a new deferral entry, and **both files were
  fenced off this pass** — so the honest state is "specified, with the rows registered and
  deferred", not "tested". The next agent's job is one test file: `CC-K-1`/`CC-K-2` (a key reload is
  accepted and disturbs nothing) and `CC-K-3`/`CC-K-4` (the two transition refusals) are the four
  rows that would make RD1 and RD6 real, and they are `reload`-shaped — bytes in, `ReloadPlan` or
  `Vec<ConfigError>` out, no clock.
- **No new vector prefix, deliberately** (§11 says so in the spec, not only here). Registering `CM`
  would have put a dozen rows in the deferral ledger with a story attached, which is the shape
  `CF-8` and `EX-12` both paid for. The new fields get `CC` rows in the commit that implements them.
- **The three constraints the dispatch named.** `cluster-config` §8 V3 — no default is restated:
  `algorithm` adopts [affinity-token](../specs/affinity-token.md) §4's, `drainTimeout` adopts DS4's,
  `L`/`E_max`/`S` are cited to their owners, and the only defaults declared here are `mint: false`
  and `incarnationSource: boot-second`, neither of which any other spec had. `affinity-token` §6 K1
  and the CRD's `KeysDistributed` — **neither moves**: §7.1 RB3 names the condition as the
  observable K2 leaves abstract, and §6 RD8 fixes the per-node report the operator computes it from.
  `AF-4`'s interface — **frozen and stated** (§4 KY1): six attributes, a change to any of them after
  AF-4 lands is a breaking change to a surface proved against §10's vectors.
- **`CX-5`'s shape, answered rather than gestured at.** §8: no value that must be unique is a pure
  function of inputs two nodes share; key material is generated from a CSPRNG off-node and
  transported verbatim (UQ2/UQ3); UQ4 tabulates every unique-required value with its source and the
  check that refuses a collision. UQ5 notes that `boot-second` has *exactly* CX-5's shape, which is
  why CT2 waits for the next second and why `incarnationSource: persisted-counter` exists.
- **Two findings, neither fixed here.** (1) `cluster-config` §9.3 RL11 has no in-document escape, so
  emergency retirement of a compromised key is **restart-class** — written down as §7.1 RB9 rather
  than papered over with a flag, because a safety rule switchable from the document is one that gets
  switched off in an incident. (2) Changing the *number* of shards re-partitions the key space, so
  every `home shard` claim in circulation goes stale for `max(L, E_max) + S`; §5 SM5 records it and
  hands the consumer half to `RG-5`.
- **Fenced and untouched:** `CHANGELOG.md`, the board, `docs/roadmap.md`,
  [vector-scope.toml](../reference/vector-scope.toml), lockfiles, and everything under `crates/**`.
  `docs/specs/affinity-token.md` and `docs/specs/sipx-cluster-crd.md` were deliberately left alone
  too — `AF-4` is implementing against the first this wave — so their stale pointers to this story
  are recorded in §12 instead of edited.

## Notes
- Design: [cluster-affinity](../designs/cluster-affinity.md).
- Cross-link for validated synthesis finding [V-15](../reviews/00-validated-synthesis.md#v-15--the-loop-cookie-key-is-predictable-from-startup-time).
- `PX-15` owns only secure per-process loop-cookie key sourcing. This story owns distribution,
  rotation, overlap and versioning when that seam becomes cluster-wide; it must consume rather than
  duplicate the proxy's injected-key interface.
