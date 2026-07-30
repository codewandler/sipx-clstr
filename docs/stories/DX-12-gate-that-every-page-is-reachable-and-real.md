---
id: DX-12
title: Gate that every site page is reachable and every command shown is real
pillar: Foundation
status: done
priority: 2
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, ci]
note: the command half is no longer hypothetical — ~30 documented commands and the M1 proof script fail
---

# Gate that every site page is reachable and every command shown is real

## Goal

Make the two site properties that are currently held by hand into checks: every authored page is
reachable from the sidebar, and every command the site tells a reader to run still works.

## Acceptance

- [x] A check fails when a file under `website/docs/` is not reachable from `website/sidebars.js`
      — proved by adding an unlisted page and watching it go red.
- [x] A check fails when `sidebars.js` names a doc id that does not exist.
- [x] The check runs in `scripts/gate.sh` and in the `docs` workflow.
- [x] The CLI surface quoted in `reference/cli.md` is verified against the binary rather than
      trusted — at minimum, that every flag named on the page is accepted and every flag the
      binary accepts is named.
- [x] **Failing-first, and it is already red**: point the check at `HEAD` as it stands and it must
      fail. Roughly thirty documented commands across `README.md` and six site pages invoke
      `--listen`/`--advertise`/`--tenant`, which no longer exist; `reference/cli.md` alone carries ten,
      plus an error-message table produced by a parser that has been replaced. `DX-13` is the content
      fix; this story is what stops it happening again.
- [x] `scripts/e2e-call.sh` runs in CI, or its absence is a deliberate recorded decision. It is the
      artefact `README.md` offers as proof of the one working feature, it is broken at `HEAD`, and
      `gate.sh` does not reference it. It needs the external `sipx` CLI, so it may not belong in the
      default gate — but "the proof is unverified in CI" should be a decision rather than an accident.

## Progress

- Landed as `scripts/check-site.py`, wired into `scripts/gate.sh` (step `site`) and
  `.github/workflows/docs.yml`. Both, on `FC-5`'s pattern: the `docs` workflow enumerates its steps
  rather than calling `gate.sh`, so a `gate.sh`-only wiring does not run in CI at all.
- **The command half was already fixed by `DX-13`, and the story's estimate is stale.** Pointed at
  `HEAD` the check found **0** bad flags, not ~30: `--listen`/`--advertise`/`--tenant` survive only in
  `whats-new.md`, which names them as removed. Every one of the 8 `sipx-clstr` invocations across
  `README.md` and the site parses, and `reference/cli.md` matched the binary in both directions on the
  first run. What was *still* red at `HEAD` was the last acceptance item: **4 proofs**
  (`e2e-call.sh`, `two-node-call.sh`, `k8s-two-node-call.sh`, `sip_demo.py`) that the documentation
  offers as evidence, that nothing in `gate.sh` or `.github/workflows/` runs, and that recorded no
  reason. Each now carries a `not-in-ci: <reason>` comment, and the check fails on a proof with
  neither.
- The reachability half was green at `HEAD` (22 pages, 22 routed), so it was proved the way the
  acceptance prescribes — by injection. Orphan page → red; `sidebars.js` naming
  `guides/no-such-page` → red; both restored.
- **The CLI surface has two sources and they are cross-checked.** The binary's `--help` when one is
  built (the gate, which runs this after `cargo build`), a static read of the `clap` derive in
  `main.rs` otherwise (the `docs` workflow, which has no Rust toolchain and where building one is a
  different cost class). When both are present they must agree, or the check fails — that is what
  stops the static reader drifting from the parser it models. Proved by adding an
  `--undocumented-knob` to `main.rs`, rebuilding, and watching both the drift check and the
  page-omission check go red. The script prints which source it used on **every** run, so a run that
  verified less says so in its own output.
- **Coverage is bounded and the bound is logged, not hidden.** 11 documented commands need Docker,
  k3d, `kubectl`, `devspace` or the external `sipx` CLI; they are printed by name on every run rather
  than skipped. Repository paths named by commands are checked only when they carry a file
  extension — `kubectl logs deploy/sipx-clstr-node-a` names a Deployment, not a directory, and the
  two are spelled identically.
- `CF-11` was already taken off the ready queue as superseded (`4de17f5`), so nothing was silently
  repurposed from here.

## Notes

- **The root cause is a deliberate choice in the checker, and it is worth stating precisely.**
  `check-docs.py` strips fenced blocks and inline spans before looking for links, and its reasoning is
  sound for link checking — a story quoting a real "broken link …" error message must not turn the gate
  red. The consequence is that **no documented command has ever been executed by the gate**, so the
  entire operational surface of the node rotted through a release with a green gate. Whatever this
  story adds has to read the code blocks that the link checker deliberately ignores, which probably
  means a second pass rather than a change to the existing one.
- The repo already has the right instinct one file over: `check-vectors.py --check` fails when the
  generated conformance report drifts from the specs. Commands have no equivalent, and that asymmetry
  is the whole story.
- Consider extracting shell blocks and asserting only the *flags* parse — `run --help` against the
  flags named on the page — rather than executing every command. Executing a `docker run` or a k3d
  bring-up in the gate is a different cost class, and the failure this needs to catch is a flag that
  no longer exists, not a runtime error.

- `CF-11` ("gate that every published doc is reachable from the site", `ready`, priority 2) is
  the ancestor of this story. Its note says two **specs** are unreachable on the published site;
  after `DX-1` no spec is published at all, so that half of it is dissolved rather than fixed.
  Decide there whether `CF-11` is re-aimed at something else or closed as superseded — do not
  silently repurpose it from here.
- Docusaurus already fails the build on a broken link and on a broken markdown link; the gap is
  the *orphan* page, which builds fine and is reachable by nobody.
