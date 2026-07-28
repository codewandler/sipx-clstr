---
id: CX-2
title: Create the Cargo workspace
pillar: Platform
status: done
priority:
design: 
epic: 
areas: [build]
note: M1 #1 · the workspace and the gate
---

# Create the Cargo workspace

## Goal
Create the Rust workspace: crate skeleton, workspace lints (`unsafe_code = "forbid"`, no-panic warnings as errors in lib code), CI with the full gate, the provenance check adapted with the integration carve-out, and a pinned sipx dependency.

## Acceptance
- [x] The gate runs green on the empty workspace: fmt, clippy `-D warnings`, tests, provenance, feature matrix.
- [x] The provenance script enforces the AGENTS.md carve-out (integration targets allowed, SIP-stack prior art rejected).
- [x] AGENTS.md's gate section is updated by this story to the command form.

## Progress
**Five crates**, drawn along the sans-IO boundary rather than by subject matter, because that
boundary is the one the harness depends on:

| Crate | Holds | Sans-IO |
|---|---|---|
| `sipx-clstr-proxy` | RFC 3261 §16 forwarding, forking, response aggregation | yes |
| `sipx-clstr-registrar` | AoR canonicalization, bindings, REGISTER, the `LocationStore` trait | yes |
| `sipx-clstr-sim` | the deterministic harness | yes |
| `sipx-clstr-probe` | the e2e-tester probe engine | yes |
| `sipx-clstr-node` | drivers, roles, the `sipx-clstr` binary | **no** — the only crate with `tokio` |

`tokio` is a dependency of `sipx-clstr-node` and of nothing else. That is the rule made
mechanical: a `tokio` line appearing in one of the other manifests is a design failure showing up
as a dependency-graph change, where it is easy to see.

**The kernel is pinned to a tag, not a branch.** sipx is not published to crates.io — the sparse
index has no `sipx-sip` — so a git dependency is the only honest way to depend on it, and
`tag = "v0.2.1"` rather than `branch = "main"` is what makes "which kernel version is this claim
true of?" a question with an answer.

- Failing-first test: `the_reported_kernel_version_matches_the_pinned_tag`. `KERNEL_VERSION` is
  what `sipx-clstr --version` reports to an operator mid-incident; nothing else keeps it in step
  with the manifest. Verified by drift — with the constant set to a wrong value the test fails
  with `KERNEL_VERSION says 0.9.9 but the workspace pins the kernel at 0.2.1`.

**The gate** is `scripts/gate.sh`, which is what CI runs step for step: `cargo fmt --check`,
`clippy --workspace --all-targets --all-features -D warnings`, `cargo test --workspace
--all-features`, `check-features.sh`, `check-provenance.sh`, `check-docs.sh`.

- `check-provenance.sh` carries **the carve-out**: `scripts/provenance-allow.txt` lists named
  integration and interop targets — in the repository, with a reason per line, which is not a
  contradiction because a term we are willing to write down is by definition not one we refuse.
  Matching is case-insensitive and *exact*: a prefix rule would let one allowed name silently
  permit a family of denied ones. Verified both ways — with the allowlist, a denied term that
  names an integration target passes; with the allowlist removed, the same term fails on real
  repository content. The script also refuses to run if the allowlist swallows the whole denylist,
  since that leaves nothing checked.
- `check-docs.sh`/`check-docs.py` moves the documentation checks out of the CI workflow and into
  a script both the workflow and `gate.sh` call, so there is one implementation rather than two
  that drift.
- `check-features.sh` builds each crate with its optional features off. The only axis today is the
  registrar's `postgres` feature (declared for `RG-4`, empty until then) — wired from day one
  rather than after the first release that does not compile for someone.

**CI** is `.github/workflows/ci.yml`; `docs.yml` now calls the shared script. The provenance step
needs the `SIPX_DENYLIST` repository secret, and exits 2 without it rather than passing.

## Notes
- Blocked by the M0 specs — the first crates implement them.
- Versions track the repository's release tags (0.3.0) rather than the crates' own maturity, and
  everything is `publish = false`. Publishing a proxy core that has never forwarded a message
  would be a claim, not a release.
- `sipx-clstr-node`'s configuration surface is deliberately provisional and minimal; `DP-1` owns
  the real schema and replaces it rather than extending it.
