---
id: CF-19
title: The documented version string is not checked against the binary
pillar: Foundation
status: in-progress
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
- [x] `scripts/check-site.py` fails when a documented `sipx-clstr --version` output line does not
      match the binary's actual output, byte for byte.
- [x] The check covers every fenced block on the site and in `README.md`, not a named list of files —
      a page added later is covered without anyone remembering to add it.
- [x] The kernel half of the string (`sipx kernel <v>`) is checked too, so a kernel pin bump that
      leaves the docs behind is red rather than silent.
- [x] Demonstrated red first: with the site one version behind, the check fails and names the file,
      the line, the documented string and the actual one.
- [x] The static-reading path (the `docs` workflow, which has no Rust toolchain) either performs the
      equivalent check against `Cargo.toml`'s `version`, or says on every run that it could not — per
      the script's existing rule that a check which silently narrows what it looks at is a lie.

## Progress
- Done in `scripts/check-site.py`; nothing else changed. No workflow wiring was needed — the `docs`
  job already runs the script, and the static path lives inside it.
- `documented_banners` reads **every** fenced block of every tracked `README.md` / `website/docs`
  page, whatever the info string, because the banner is *output* and so lives in exactly the `text`
  blocks the command check skips. A banner is recognised by shape (`sipx-clstr` then something
  version-shaped), so a stale one — or one that has lost its `(sipx kernel …)` half — is still
  recognised and still compared; the comparison is the whole line, byte for byte.
- Two readings, the same discipline `cli_surface` already had: `--version` from a built binary, and
  the banner composed from `Cargo.toml`'s `[workspace.package] version` plus the `sipx-sip` `tag`
  pin (read the way `crates/sipx-clstr-node/tests/kernel_pin.rs` reads it). Both present and
  disagreeing is a failure — a binary built before a bump, or a drifted `KERNEL_VERSION`. Neither
  available and there being banners to check is also a failure, rather than a green run that read
  nothing. Every run prints which reading it used.
- `self_test` grew ten cases around the recogniser, which is where all the risk is: a refusal
  message (`sipx-clstr: cluster.yaml was refused …`), an invocation (`sipx-clstr --version`) and an
  image tag (`sipx-clstr:dev`) are not banners; a stale one and a half-deleted one are.
- Red first, at the merge base, by setting `website/docs/getting-started.md:26` back to `0.11.0`:
  the check as it then stood reported `site: clean` and exited 0. With this change the same edit
  fails, naming the file, the line, the documented banner and the one the binary prints — and fails
  the same way through the `Cargo.toml` path with `SIPX_CLSTR_BIN` pointed at nothing. The kernel
  half was proved separately by moving the documented pin to `0.9.0`. The page was restored; no
  site page is touched by this diff.

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
