---
id: DP-16
title: Load the membership, key and shard-map sections the config loader still refuses
pillar: Cluster
status: done
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
- [x] `membership[]` accepts the fields `cluster-membership` §3 defines — the loader's closed world is
      `["node", "name", "zone", "roles"]` today (`crates/sipx-clstr-node/src/config/mod.rs:1141`), so
      `rpc` and `incarnationSource` are `V2` errors.
      → all seven fields of §3, with `MB4`–`MB6` and `MB8` enforced (`config/mod.rs:read_membership`,
      `read_rpc`, `read_incarnation`, `check_member_is_unique`).
- [x] `keys[]` and `shardMap` leave `DEFERRED_SECTIONS` (`config/mod.rs:412`) and are validated per
      §4 `KY1`–`KY9` and §5 `SM1`–`SM5`, including `KY3`'s reserved-and-always-refused `secret`.
      → `read_keys`/`read_key_entry`/`read_shard_map`/`check_shard_owners`. `KY2` and `KY8` are
      start-up rules over a *resolved* secret and are handed on — see Progress.
- [x] Reload without restart holds as §6 `RD1` states it: no listener rebound, no connection closed,
      no registration expired, no token or reference invalidated, no dialog or in-flight transaction
      disturbed.
      → `config::reload` returns a `ReloadPlan` only when every `rollout`-class section is unchanged
      (`RL1`/`RL2`), the version advances (`D10`), no id is re-pointed (`I3`) and `RL10`/`RL11` hold.
      `RL11` is **both** of its clauses: the retiring key's `verifyUntil` is not brought forward, and
      the incoming mint key's window covers `max(L, E_max) + S` computed from the incoming document
      (`overlap_window`, `CC-K-7`). The first round shipped only the first clause and ticked this box
      anyway — see Progress.
- [x] The `cluster-config` §12 rows that execute these rules are **proved, and their deferrals
      re-pointed here from `DP-8`** — `CC-K-1`…`CC-K-6` (key reload), `CC-S-1`…`CC-S-9` (shard-map
      handoff), `CC-V-4`/`CC-V-10`, `CC-R-7`/`CC-R-8`, `CC-I-1`/`CC-I-4`.
      → all proved except `CC-S-1`…`CC-S-6`, `CC-S-8`, `CC-S-9`, which are the handoff **runtime**
      §9.4 `DS7` assigns to `RG-5` and are re-pointed there with a reason. See Progress.
- [x] **Failing-first:** a document containing a `cluster-membership` §3 `rpc` entry is refused today
      and accepted after. Demonstrate the refusal at the merge base.
      → `cc_r_11_a_member_declares_the_rpc_endpoint_and_incarnation_source_of_section_3`, red at
      `43594b1` with two `CC-V2` errors naming `rpc` and `incarnationSource`.
- [x] `deploy/devspace/manifests/node.yaml`, `website/docs/reference/configuration.md`,
      `scripts/two-node-call.sh` and `scripts/e2e-call.sh` all declare `edge`+`registrar` members with
      no `rpc`; `MB5` will invalidate every one once enforced. Update them, or state why `MB5` should
      not apply to them.
      → all four updated, plus two the story did not name (`README.md`,
      `website/docs/getting-started.md`) and five node test fixtures.

## Progress

**Done.** The loader reads all three sections; `scripts/gate.sh` is green.

- **`membership[]` carries §3's seven fields.** `MB5` (`rpc` required on the call path, refused off
  it), `MB6` (unique, and the form is §5 `P7`'s — `Advertised::parse`, inherited rather than
  restated, plus the required port), `MB4` (unique `name`) and `MB8` (`incarnationSource` /
  `incarnationRef`).
- **`keys[]` and `shardMap` are validated in full.** `KY1` (the six attributes, closed), `KY3` (an
  inline `secret` refused citing `V9` and never echoed), `KY4` (RFC 3339 UTC instants, `Z` only,
  parsed to UNIX seconds without a date dependency), `KY5`, `KY6`; `SM1` totality, `SM2` owner
  resolution, `SM3` registrar-owner, `DS4`'s `drainTimeout` range.
- **`config::reload(active, bytes, identity, env)`** judges §9 between two documents: `D10`, `RL1`
  /`RL2` from a §7 class table, `I3`/`RD4`, `RL10`, `RL11`. `RD3` needs no rule of its own — §5 `P3`'s
  cross-check already refuses a reload that changes this node's own zone or roles.

**Handed to the next story — the driver side.** The write-set for this story excluded `driver.rs`
(`RG-17`'s this wave), and everything in these three sections that would *act* needs a `NodeConfig`
field:

- `keys[]` reaching `AF-4`'s mint/verify library, and with it `KY2` (refuse to start on a partially
  resolved key set) and `KY8` (refuse the `affinity-token` §10 test keys) — both are start-up rules
  over a **resolved** secret, so they cannot be written before a consumer exists to resolve for.
- `membership[].rpc` reaching the connection-owner RPC (`AF-3`/`AF-7`).
- `shardMap` reaching the handoff (`RG-5`).

Until then every one of those paths is reported by `Config::unapplied`, and `MB5` makes `rpc`
mandatory, so **every** document now carries at least one unapplied path. That is `FC-2` working:
the alternative is a warning that is quiet about the one key every document was just made to carry.

**Ledger — for the integrator.** `docs/reference/vector-scope.toml` and the regenerated
`docs/reference/conformance.md` are in this branch. Of the 35 `CC` rows `CF-24` pointed at `DP-16`,
25 are proved and 10 are re-pointed: `CC-S-1`…`CC-S-6`, `CC-S-8`, `CC-S-9` → `RG-5` (the handoff
runtime, §9.4 `DS7`), `CC-D-3` → `RG-22` (the `registrar` section's closed world), `CC-V-5` → `ET-6`
(the `probe` section). No row names `DP-16` any more, so closing it leaves no dead letter.
`CHANGELOG.md`, the board and `docs/roadmap.md` are untouched and are the integrator's.

### Round 2 — the two blocking findings from review

**1. The instant reader failed open.** `parse_rfc3339_utc` parsed the year with `str::parse::<i64>`
— unbounded — and then multiplied. Through `config::load`, `verifyFrom:
"300000000000-01-01T00:00:00Z"` panicked under `debug-assertions` and, under `-O`, **wrapped** to
`Some(-8979658535876770816)` and loaded. A wrapped instant is the worse half: it is a verify window
nobody wrote, and every rotation rule downstream is then judged against it silently. Fixed by
reading RFC 3339 §5.6's grammar as it is written — `4DIGIT`, `2DIGIT`, `"." 1*DIGIT`, via
`fixed_digits` — which bounds the arithmetic, *and* by making the arithmetic `checked_*` anyway,
because a totality that rests on an argument about the grammar is one a later widening removes
without anyone noticing. The same pass closed the lenient spellings: `2026-07-28T-1:-5:-9Z` loaded
and named an instant **twenty-six hours** from the one written; `+2026-…`, `2026-7-8T1:2:3Z`,
`…T+1:00:00Z`, `…00.abcZ`, `…00.Z` and `…00.1.2Z` all loaded. `CC-V-23` and `CC-V-24` execute this.

**2. `RL11`'s second clause was not implemented, and the box was ticked.** §9.3 RL11 requires the
retiring key's `verifyUntil` not be brought forward **and** that the incoming mint key's window cover
the same bound; `check_key_transition` did only the first, so a reload flipping `mint` to a key with
a 60 s window was accepted against a `W` of 86 430 s. Nothing computed `E_max` — it appeared only
inside message strings. Now `overlap_window(next)` computes `max(L, E_max) + S` from the incoming
document (`RB1`'s "largest tenant expiry", `RB5`'s "at the moment of retirement"), and the clause is
enforced as a **width**, which is the strongest clock-free form of `RB2` a loader denied a clock by
§2 `D1` can state. `CC-K-7` executes it; a second test proves the bound follows the document's
largest `tenant[].expiry.max` rather than a constant.

**Also in this pass.** The two re-points review called out are corrected: `CC-D-3` → `KO-3`, which
already owns [sipx-cluster-crd](../specs/sipx-cluster-crd.md) `SC-A-1` — whose Expect is *verbatim*
this row's error — and `CC-V-5` → `ET-4`, which `DP-13`'s own Acceptance names as the story that
supplies the `e2e-tester` driver, and whose `GET /probes` cannot answer without the section being
read. And `config/tests.rs` held a raw `NUL` byte inside a byte-string literal, which compiled fine
and made the whole file **invisible to `grep`** — every search over it returned nothing, silently.
Written as `\x00` now.

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

- **Integrated 2026-08-05 after one rework round, gate green on `main` (209/613 rows proved,
  deferrals down from 410 to 385).** The review found two blocking defects, both in code this
  story added: `parse_rfc3339_utc` reached unchecked arithmetic on an operator-supplied field, so
  a `verifyFrom` year of `300000000000` **panicked in debug and silently wrapped in release** —
  the fail-closed surface failing open, accepting a verify window nobody wrote. And Acceptance
  item 3 was ticked claiming "RL10/RL11 hold" while RL11's second clause (the incoming mint key's
  window must cover the same bound) was unimplemented and undisclosed. Both fixed and proved in
  **both profiles**, which is the only way the release-mode defect is visible.
- **RL11's width bound, and why it is a width.** `overlap_window` computes `W = max(L, E_max) + S`
  from the *incoming* document. The spec states the rule against the wall clock, and §2 D1 denies
  the loader a clock — so the width is its strongest clock-free consequence, necessary rather
  than sufficient, with the wall-clock half left to `RB5`. Recorded because it is a real
  operational edge: a fleet whose mint window is narrower than `W` cannot reload *anything* until
  the window widens.
- **Two of its own tests were nearly worthless, and it caught both.** `cc_v_24`'s seven forbidden
  RFC 3339 spellings were *already* refused at the base — for the wrong reason ("a window that
  never opens") — so a test asserting only the rule id would have passed against the very defect
  it was written for, which is `CF-12`'s exact shape; it now asserts the offending text is what
  was found. And round 1's test file carried a raw `NUL` byte inside a byte-string literal: it
  compiled, its tests passed, and it made all 2 000 lines **invisible to `grep`**.
- **The "apply" half is `DP-17`'s**, filed at this story's review. Every Acceptance item here is
  worded *accepts*/*validated*/*proved* and is met; only the Goal says *apply*, and `MB5` made
  the unapplied third state universal rather than opt-in. Splitting kept that honest.

