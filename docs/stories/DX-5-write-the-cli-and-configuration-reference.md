---
id: DX-5
title: Write the CLI and configuration reference
pillar: Foundation
status: done
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

- [x] `reference/cli.md` documents `run`, `--listen`, `--tenant`, `--advertise`, `--version`,
      `--help`, each with its default, taken from `crates/sipx-clstr-node/src/main.rs`.
- [x] It carries the exit-code table (`0` success · `1` runtime failure · `2` usage/config error)
      and the stdout/stderr contract: `listening on` / `advertising` on stdout after the bind,
      logs on stderr, no ANSI.
- [x] It states that the argument parser is **provisional** and that `DP-1` replaces rather than
      extends it, so nobody builds tooling on its shape.
- [x] `reference/configuration.md` says plainly that there is **no configuration file today**, and
      that auth and the PostgreSQL store are implemented but unreachable from the binary.
- [x] It describes the forthcoming cluster-config schema — one cluster-scoped document, YAML or
      JSON, `lowerCamelCase` keys, closed world, refusing to start as the only failure mode — and
      links the spec on GitHub rather than restating it normatively.
- [x] It does **not** present `deploy/helm/values.yaml` as the schema, and says why.
- [x] Every command shown has been executed and its real output pasted.

## Progress

- **Done.** Both pages written; the placeholder bodies are replaced and the frontmatter is
  unchanged. `website/sidebars.js` already routes them, so nothing else moved.
- `cli.md` is built from `main.rs` alone. Every flag, every refusal message and every exit code on
  the page was produced by running the built binary and pasting stdout/stderr verbatim — including
  the three that only a wrong invocation reaches (`unknown argument`, `needs a value`,
  `is not an address:port`) and the two `--advertise` refusals from `listen.rs`. Exit `1` was
  provoked with a second node on a bound port (`io: Address already in use (os error 98)`).
- One correction found while writing: `run --version` prints ``--version needs a value``, not
  "unknown option", because `run` parses flag-and-value pairs and checks for the value first
  (`main.rs:71-75`). The first draft guessed and was wrong; the page now states the pairing rule.
- `DP-1` is cited on `cli.md` as a GitHub story link rather than a bare id, matching how
  `configuration.md` cites `KO-14`. No other site page names an internal story id, so this is the
  smallest form that still satisfies the acceptance item literally.
- `configuration.md` proves the PostgreSQL gap by building with `--features postgres` and showing
  the help text is byte-identical — the feature compiles the store in and gives you no way to
  point at a database. The auth gap is stated as what it is: `NodeConfig.auth` is never set by
  `main.rs`, and absent means an open tenant.
- The `values.yaml` section enumerates four divergences rather than asserting one, so a reader can
  check each against the file: `maxForwards: 10` vs 70, the cluster-wide media block and its codec
  policy, sections under names the schema does not use, and `snake_case` enumerated values.
- Considered for upstream: no. This is documentation of this platform's own binary and its own
  configuration schema; the kernel has no opinion about either.
- Gate: `python3 scripts/check-docs.py` → `docs: clean (166 markdown files checked)`;
  `npm run build` in `website/` → `[SUCCESS] Generated static files in "build".` The Rust gate was
  not run — no Rust changed. `node_modules` was symlinked from the main checkout for the build and
  removed before committing.

## Notes

- Spec: `docs/specs/cluster-config.md`. Link it by absolute GitHub URL — a relative link into
  `docs/` now fails `check-docs.py`.
- The chart diverges from the spec today: snake_case keys, and `security.maxForwards: 10` against
  the spec's fixed 70. `KO-14` is open on it. Do not document it as authoritative.
- Do not invent flags. If a knob is not in `main.rs`, it does not exist.
