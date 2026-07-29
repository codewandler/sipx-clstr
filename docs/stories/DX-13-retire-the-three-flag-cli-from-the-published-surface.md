---
id: DX-13
title: Retire the three-flag CLI from the published surface and from the M1 proof script
pillar: Foundation
status: ready
priority: 1
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, deploy]
note: DP-10 deferred the docs pass on purpose; e2e-call.sh was on its must-move list and did not move
---

# Retire the three-flag CLI from the published surface and from the M1 proof script

## Goal

Make every command the repository shows a reader actually run. `DP-10` replaced
`--listen`/`--advertise`/`--tenant` with `--config`, deferring the documentation pass to be cut with a
release — a defensible call. Two things escaped it: the script the README offers as its proof, and the
disclosure pages whose warnings are now aimed at a CLI that no longer exists.

## Acceptance

- [ ] `scripts/e2e-call.sh` starts a node with a configuration document. It still passes
      `--listen`/`--advertise` and therefore fails, which means the artefact `README.md` cites as
      "a scripted, repeatable proof, not a claim" does not run. It is on `DP-10`'s own blast-radius
      table as a file that "has to move in the same change"; `deploy/devspace/manifests/node.yaml`
      and `scripts/two-node-call.sh` did move.
- [ ] The README quick start runs as written, end to end, against a clean checkout. It currently
      exits 2 with `error: unexpected argument '--listen' found`, on the first command of
      "Five minutes to a forwarded call".
- [ ] Every command on the six affected site pages runs: `getting-started.md`,
      `guides/run-a-node.md`, `guides/addressing.md`, `guides/docker-and-k3d.md`,
      `reference/cli.md`, `reference/configuration.md`. `reference/cli.md` needs the most care — its
      selling point is that every flag and error message was produced by running the binary, and it
      now documents a hand-rolled parser's messages against a `clap` one.
- [ ] The five pages asserting there is no configuration file stop asserting it. A document is now
      the *only* way to start a node, and `run` refuses without `--config`. `reference/configuration.md`
      is the one to rewrite rather than patch: its central thesis is that no such file exists.
- [ ] The "three command-line flags" framing is removed wherever it is given as the *cause* of the
      open registrar and the volatile store — `README.md`, `intro.md`, `whats-new.md`,
      `reference/configuration.md`, `guides/run-a-node.md`. The conclusion is still true; the stated
      mechanism is gone, and the real one is `FC-3`.
- [ ] `guides/run-a-node.md` gains the warning it lacks. It is the only page that leads with a public
      bind (`0.0.0.0:5060` advertised to a routable address) and never says the node is an open
      registrar or that it should not be exposed. Getting-started shows loopback and carries the
      caution box; the page a reader opens *in order to deploy* carries none.
- [ ] `operate/deploy.md`'s exposure table stops advertising `TLS 5061 | public`. Coordinate with
      `FC-1`, which owns whether TLS is refused or served — one edit to the page, not two.
- [ ] The status rows are re-derived rather than copy-edited. Several are now wrong in the
      *understating* direction: the README's "Reachable but not wired" says neither digest auth nor
      the PostgreSQL store "can be switched on from the binary", and the store now can be — selected
      from the document, needing the non-default `postgres` cargo feature, and refusing to start
      without it, which is a better story than the one being told. `whats-new.md`'s "there is no
      second node" and `migrate/from-kamailio.md`'s "there is no clustering in the binary you can
      build today" are contradicted by `scripts/two-node-call.sh` and by the binary's own `--help`.
- [ ] `CHANGELOG.md`'s `[Unreleased]` **Known gaps** entry "Nothing reads a cluster document at
      startup yet (`DP-10`)" is corrected, and `DP-9`/`DP-10` get their `### Added` entries. Per
      `AGENTS.md` closed stories roll up here, and the ledger currently says the work has not landed —
      which is why nothing downstream was reconciled.
- [ ] `Dockerfile`'s `CMD ["--help"]` rationale is rewritten. It argues from `--listen 0.0.0.0:5060`
      and ends "Every real invocation passes `--advertise`, as the manifests do." No manifest does.
      While there: `docker-and-k3d.md` says the image is built without the database driver because
      "the binary cannot use anyway" — the Dockerfile defaults `CARGO_FEATURES=postgres`, so the
      claim is inverted.

## Progress

- (running log)

## Notes

- **Scale, measured.** Roughly thirty occurrences across `README.md` and six site pages, plus
  `scripts/e2e-call.sh`. `reference/cli.md` alone carries ten.
- **`DP-10` was right to defer, and this is the pass it deferred to.** Its story says the eight doc
  files "are deliberately left for one pass together with a release, because the site deploys from a
  tag: editing them now would make the site describe a binary nobody can download yet," and it names
  the consequence it accepted — "the site and the binary disagree between the merge and the next tag
  unless the two are cut together." So this story is release-coupled by design: it lands with the tag,
  not before it. What was *not* deferred deliberately is `e2e-call.sh`, which is a script rather than
  a page and is on the same table.
- **Stale disclosure that reads as resolved is worse than absent disclosure.** `reference/configuration.md`
  tells the reader to keep the node on loopback "**until the configuration schema lands**". The schema
  has landed. A reader who finds `--config`, writes the `tenant[].auth` block that
  [cluster-config](../specs/cluster-config.md) §5 S2/S6 specifies, and gets a clean load has satisfied
  every condition the page set for the danger to be over — and is running an open registrar (`FC-3`).
- **`addressing.md` does not survive a mechanical edit.** `DP-10` flagged this and it is still true:
  the page is an argument about bind versus advertise, and the document form has to make that argument
  in its own shape rather than rename flags in place.
- The gate cannot catch any of this today — `check-docs.py` strips fenced code blocks by design
  ("Code is not prose"), so no documented command has ever been executed by CI. `DX-12` owns fixing
  that, and doing this story without `DX-12` means the next CLI change rots the docs again.
- Credit where it is due, so the rewrite does not lose it: the migration pages, the `(preview)`
  banners on all nine future-capability pages, and the conformance page's refusal to duplicate the
  vector table are genuinely good and should survive unchanged.
