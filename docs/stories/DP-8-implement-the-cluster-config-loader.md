---
id: DP-8
title: Implement the cluster config loader as a pure function
pillar: Cluster
status: done
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, build]
note: DP-1 specified the schema and nothing loads it — the binary still has three provisional flags
---

# Implement the cluster config loader as a pure function

## Goal

Make `docs/specs/cluster-config.md` executable: one cluster-scoped document, projected through a
node identity, parsed by a pure function that either returns a config or refuses to start. This is
the keystone the whole deployment story waits on — roles, listeners, the location store and tenants
are all unreachable until a node can read a document.

## Acceptance

- [x] `load(bytes, identity, env) -> Result<Config, Vec<ConfigError>>` is a **pure function**: no
      socket, no clock, no filesystem, no second file. `env` is an argument, including for
      `${VAR}` interpolation (D1, V4).
- [x] YAML 1.2 core schema **and** JSON both parse to the same data model (D3), with
      `lowerCamelCase` keys and `kebab-case` enum values (D4).
- [x] **Closed world**: an unknown key is an error, not a warning (V2).
- [x] **Every** error is reported, ordered by path — not just the first (V1). A failing-first test
      supplies a document with at least three independent faults and asserts all three come back
      in path order.
- [x] Refusing to start is the only failure mode: no partial apply, no degraded mode (V10).
- [x] Where an RFC fixes a value the schema offers no knob — `maxForwards` is 70 and not tunable
      downward (V6).
- [x] No secrets in the document: `dsnRef`, `secretRef`, `keyRef` only (V9).
- [x] The same bytes projected through two different `NodeIdentity` values yield two different
      configs, and neither branches on role inside the loader (D2, P1, R3).
- [x] `cargo test -p sipx-clstr-node --all-features` green; the loader crate stays sans-IO and the
      existing layering test still passes.

## Progress

- **Done, with a stated scope.** `crates/sipx-clstr-node/src/config/` — `load` and `project`, plus
  the `ConfigError`/`Path`/`RuleId` shapes §8 fixes. 24 tests, each named after the rule it proves.
- **Not a `derive(Deserialize)`, deliberately.** V1 wants *every* error ordered by path, and serde
  stops at the first and reports it as a message rather than a path plus a rule id. V2's closed
  world needs the same shape — you cannot ask serde which keys it did *not* recognise. So the
  document is parsed to a generic value tree and walked by hand, accumulating errors.
- **The failing-first test is `cc_v1_reports_every_error_ordered_by_path`**: one document with four
  independent faults must return all four, sorted, and two runs must be byte-identical. It also
  asserts the ordering property directly rather than trusting the sort.
- **One reader, two encodings.** `cc_d3_json_and_yaml_produce_the_same_config` loads the same
  cluster as YAML and as JSON and asserts the two `Config` values are equal — D3 satisfied by one
  parser rather than by two code paths that could disagree.
- **Sections implemented:** `apiVersion`, `version`, and under `cluster` — `name`, `environment`,
  `zones`, `listener[]`, `membership`, `locationStore`, `tenant[]`, `security`, `timers`.
- **Sections deferred:** the other seventeen of §7's registry. They are *recognised* — naming one is
  legal, a typo in the name is still a V2 error — and reported in `Config::deferred` rather than
  silently dropped. Field-level validation inside them is not implemented. A section quietly ignored
  would be configuration nobody applies with nothing saying so, which is V2's own failure one level
  up, so the boundary is a value a caller can read.
- Rules with tests: D1, D2, D3, D4 (via role/key spelling), R1, R4, R6, I2, I4, P1, P3, P4, V1, V2,
  V4, V6, V7, V8, V9, V10, and §3's version check.
- **`maxForwards` gets its own refusal** rather than falling through to the generic closed-world
  message. It is not in the recognised key set, so V2 would already reject it — but "unknown key"
  teaches the wrong lesson about a value an RFC fixes, so V6 says so in its own words.
- Gate green end to end: fmt, clippy `-D warnings`, tests, features, MSRV 1.94, provenance, vectors,
  docs.
- Considered for upstream: no. This is this platform's own configuration schema; the kernel has no
  opinion about roles, zones or shards.

### The dependency, flagged rather than assumed

`serde_yaml_ng` brings **`unsafe-libyaml`** into the tree. `AGENTS.md` non-negotiable #3 forbids
`unsafe`, and `[workspace.lints.rust] unsafe_code = "forbid"` enforces it — but only for crates in
this workspace, so this passed the gate without comment. That is worth a decision rather than a
shrug: the loader parses an operator-supplied document, not network input, which is the weaker
exposure of the two, but "no unsafe" is stated as a property of the platform.

Options, if the answer is that it should go: a pure-safe-Rust YAML parser (`saphyr`/`yaml-rust2`
lineage) with a hand-written bridge to the value tree this module already walks — which is cheaper
here than usual, because the module deliberately does not depend on serde's derive machinery. Left
open rather than decided unilaterally.

## Notes

- Spec: `docs/specs/cluster-config.md` — normative, 425 lines, with the section/reload-class
  registry at §7 and the validation rules at §9. Cite rule IDs in the tests.
- **Scope discipline for this story: the loader only.** Do not wire it into `driver.rs` or replace
  the command-line flags — `RG-12` and its successors do that, and keeping them apart is what lets
  this land while other work touches the driver.
- `main.rs` says the argument surface is provisional and that this schema **replaces** rather than
  extends it. Do not add flags to bridge the gap; that is the improvisation the spec exists to
  prevent.
- `deploy/helm/values.yaml` disagrees with the schema in four places; `KO-14` owns reconciling it.
  Do not treat the chart as the schema.
