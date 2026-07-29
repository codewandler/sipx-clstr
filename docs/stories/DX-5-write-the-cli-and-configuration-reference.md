---
id: DX-5
title: Write the CLI and configuration reference
pillar: Foundation
status: ready
priority: 1
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: the chart's values are NOT the config schema — KO-14 is open against exactly that divergence
---

# Write the CLI and configuration reference

## Goal

Give `website/docs/reference/cli.md` and `website/docs/reference/configuration.md` their content:
the complete command surface a script can rely on, and an honest account of configuration, which
today is three flags and no file.

## Acceptance

- [ ] `reference/cli.md` documents `run`, `--listen`, `--tenant`, `--advertise`, `--version`,
      `--help`, each with its default, taken from `crates/sipx-clstr-node/src/main.rs`.
- [ ] It carries the exit-code table (`0` success · `1` runtime failure · `2` usage/config error)
      and the stdout/stderr contract: `listening on` / `advertising` on stdout after the bind,
      logs on stderr, no ANSI.
- [ ] It states that the argument parser is **provisional** and that `DP-1` replaces rather than
      extends it, so nobody builds tooling on its shape.
- [ ] `reference/configuration.md` says plainly that there is **no configuration file today**, and
      that auth and the PostgreSQL store are implemented but unreachable from the binary.
- [ ] It describes the forthcoming cluster-config schema — one cluster-scoped document, YAML or
      JSON, `lowerCamelCase` keys, closed world, refusing to start as the only failure mode — and
      links the spec on GitHub rather than restating it normatively.
- [ ] It does **not** present `deploy/helm/values.yaml` as the schema, and says why.
- [ ] Every command shown has been executed and its real output pasted.

## Progress

- (running log)

## Notes

- Spec: `docs/specs/cluster-config.md`. Link it by absolute GitHub URL — a relative link into
  `docs/` now fails `check-docs.py`.
- The chart diverges from the spec today: snake_case keys, and `security.maxForwards: 10` against
  the spec's fixed 70. `KO-14` is open on it. Do not document it as authoritative.
- Do not invent flags. If a knob is not in `main.rs`, it does not exist.
