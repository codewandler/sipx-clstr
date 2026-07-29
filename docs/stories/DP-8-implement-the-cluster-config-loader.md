---
id: DP-8
title: Implement the cluster config loader as a pure function
pillar: Cluster
status: ready
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

- [ ] `load(bytes, identity, env) -> Result<Config, Vec<ConfigError>>` is a **pure function**: no
      socket, no clock, no filesystem, no second file. `env` is an argument, including for
      `${VAR}` interpolation (D1, V4).
- [ ] YAML 1.2 core schema **and** JSON both parse to the same data model (D3), with
      `lowerCamelCase` keys and `kebab-case` enum values (D4).
- [ ] **Closed world**: an unknown key is an error, not a warning (V2).
- [ ] **Every** error is reported, ordered by path — not just the first (V1). A failing-first test
      supplies a document with at least three independent faults and asserts all three come back
      in path order.
- [ ] Refusing to start is the only failure mode: no partial apply, no degraded mode (V10).
- [ ] Where an RFC fixes a value the schema offers no knob — `maxForwards` is 70 and not tunable
      downward (V6).
- [ ] No secrets in the document: `dsnRef`, `secretRef`, `keyRef` only (V9).
- [ ] The same bytes projected through two different `NodeIdentity` values yield two different
      configs, and neither branches on role inside the loader (D2, P1, R3).
- [ ] `cargo test -p sipx-clstr-node --all-features` green; the loader crate stays sans-IO and the
      existing layering test still passes.

## Progress

- (running log)

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
