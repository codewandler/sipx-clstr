---
id: CF-19
title: The documented version string is not checked against the binary
pillar: Foundation
status: ready
priority: 3
epic: conformance-harness
areas: [gate, docs]
note: check-site.py reads every documented command's flags and never its output — three pages shipped a stale version through 0.11.0
---

# The documented version string is not checked against the binary

## Goal
Hold the version strings the published site prints to what the binary actually reports, the same way
`DX-12` already holds documented *flags* to what the binary accepts.

## Acceptance
- [ ] `scripts/check-site.py` fails when a documented `sipx-clstr --version` output line does not
      match the binary's actual output, byte for byte.
- [ ] The check covers every fenced block on the site and in `README.md`, not a named list of files —
      a page added later is covered without anyone remembering to add it.
- [ ] The kernel half of the string (`sipx kernel <v>`) is checked too, so a kernel pin bump that
      leaves the docs behind is red rather than silent.
- [ ] Demonstrated red first: with the site one version behind, the check fails and names the file,
      the line, the documented string and the actual one.
- [ ] The static-reading path (the `docs` workflow, which has no Rust toolchain) either performs the
      equivalent check against `Cargo.toml`'s `version`, or says on every run that it could not — per
      the script's existing rule that a check which silently narrows what it looks at is a lie.

## Progress
- (not started)

## Notes
- Found while cutting `0.12.0`. `website/docs/whats-new.md`, `website/docs/reference/cli.md` and
  `website/docs/getting-started.md` all printed `sipx-clstr 0.11.0 (sipx kernel 0.10.0)` after the
  workspace version had moved to `0.12.0`, and the **full gate stayed green** — `check-site.py`
  reports "8 sipx-clstr command(s) verified", but verifying an invocation means checking the *flags*
  it names against `--help`; nothing reads what a documented command is shown as *printing*.
- This is the same shape as the defect `DX-12` was written to close, one level down. `DX-12` closed
  "the docs name flags the binary stopped accepting"; this is "the docs name output the binary
  stopped producing". The `0.11.0` cut moved these three strings by hand, which is why it worked that
  time and is exactly why it should not be held by hand.
- It is a **release-time** defect specifically: the version only goes stale at a cut, so nothing
  between releases would ever surface it, and the site deploys *on release* — so the first reader of
  the wrong number is a public one.
- Scope note — considered for upstream: no. This checks this repository's own published docs against
  this repository's own binary; there is nothing protocol-generic in it.
