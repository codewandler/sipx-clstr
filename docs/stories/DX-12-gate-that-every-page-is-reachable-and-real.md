---
id: DX-12
title: Gate that every site page is reachable and every command shown is real
pillar: Foundation
status: ready
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

- [ ] A check fails when a file under `website/docs/` is not reachable from `website/sidebars.js`
      — proved by adding an unlisted page and watching it go red.
- [ ] A check fails when `sidebars.js` names a doc id that does not exist.
- [ ] The check runs in `scripts/gate.sh` and in the `docs` workflow.
- [ ] The CLI surface quoted in `reference/cli.md` is verified against the binary rather than
      trusted — at minimum, that every flag named on the page is accepted and every flag the
      binary accepts is named.
- [ ] **Failing-first, and it is already red**: point the check at `HEAD` as it stands and it must
      fail. Roughly thirty documented commands across `README.md` and six site pages invoke
      `--listen`/`--advertise`/`--tenant`, which no longer exist; `reference/cli.md` alone carries ten,
      plus an error-message table produced by a parser that has been replaced. `DX-13` is the content
      fix; this story is what stops it happening again.
- [ ] `scripts/e2e-call.sh` runs in CI, or its absence is a deliberate recorded decision. It is the
      artefact `README.md` offers as proof of the one working feature, it is broken at `HEAD`, and
      `gate.sh` does not reference it. It needs the external `sipx` CLI, so it may not belong in the
      default gate — but "the proof is unverified in CI" should be a decision rather than an accident.

## Progress

- (running log)

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
