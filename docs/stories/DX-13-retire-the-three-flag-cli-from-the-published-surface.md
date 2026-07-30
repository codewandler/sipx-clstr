---
id: DX-13
title: Retire the three-flag CLI from the published surface and from the M1 proof script
pillar: Foundation
status: in-progress
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

- [x] `scripts/e2e-call.sh` starts a node with a configuration document. **Done in `2a7aeeb`** — it
      writes a cluster document and runs from that, and the proof passes again: audio heard, 24000
      samples, one socket on the node, transaction store drained. It had been on `DP-10`'s own
      blast-radius table as a file that "has to move in the same change", while
      `deploy/devspace/manifests/node.yaml` and `scripts/two-node-call.sh` did move.
- [x] The README quick start runs as written, end to end, against a clean checkout. The `--listen`
      form was replaced in the `0.11.0` docs pass; what was still broken is that the document it
      writes declared `domains: [example.test]` while `sip_demo.py` registers `alice@127.0.0.1`, and
      `FC-4` made `domains` enforceable — so both REGISTERs came back **`403`** and the page's own
      output block was unreachable. The document now declares `domains: [127.0.0.1]`, matching
      `scripts/e2e-call.sh`, and the heredoc extracted verbatim from `README.md` and run in an empty
      directory reaches `RESULT: PASS`.
- [ ] Every command on the six affected site pages runs: `getting-started.md`,
      `guides/run-a-node.md`, `guides/addressing.md`, `guides/docker-and-k3d.md`,
      `reference/cli.md`, `reference/configuration.md`. `reference/cli.md` needs the most care — its
      selling point is that every flag and error message was produced by running the binary, and it
      now documents a hand-rolled parser's messages against a `clap` one.

      **Every command on five of the six ran here**; one is blocked on a fenced file. `cli.md` was
      re-verified line by line against the binary — the two help texts, `--version`, and each quoted
      refusal (`no id`, `cannot read`, the two-problem document refusal, the unresolved `dsnRef`, the
      role refusals) reproduce exactly. Two defects found and fixed by running rather than reading:
      `addressing.md` quoted the `0.0.0.0` refusal without its `cluster.listener:` path prefix and
      with a second line the binary does not print, and `cli.md`'s opening block is `-h`/no-argument
      output rather than `--help`, which is now labelled. `configuration.md`'s documents were all
      loaded by the binary. The blocked one is `getting-started.md` §3–4: see `## Progress`.
- [x] The five pages asserting there is no configuration file stop asserting it. Done in the
      `0.11.0` pass and re-checked; the one survivor was `whats-new.md`'s 0.10.0 entry advertising
      "a configuration page that states plainly there is no configuration file yet", which a reader
      would click through and find contradicted. It is now scoped to the release it describes.
- [x] The "three command-line flags" framing is removed wherever it is given as the *cause* of the
      open registrar and the volatile store — `README.md`, `intro.md`, `whats-new.md`,
      `reference/configuration.md`, `guides/run-a-node.md`. The conclusion is still true; the stated
      mechanism is gone, and the real one is `FC-3`. **And `FC-3` has since landed, so the
      replacement mechanism is not "no configuration path" either** — it is "no credential store".
      Proved by running: a document declaring `tenant[].auth` whose `secretRef` resolves exits `2`
      naming `RG-7`; one whose reference does not resolve starts, logs `auth="required"`, and
      answers every REGISTER `401` with a `WWW-Authenticate` nobody can satisfy. Both states are now
      written down, on every page that made the claim, plus `guides/registrations-and-calls.md`,
      `guides/does-this-fit.md`, `migrate/from-kamailio.md` and `clustering/registrar-shards.md`,
      which made it in the same words.
- [x] `guides/run-a-node.md` gains the warning it lacks. It is the only page that leads with a public
      bind (`0.0.0.0:5060` advertised to a routable address) and never says the node is an open
      registrar or that it should not be exposed. Getting-started shows loopback and carries the
      caution box; the page a reader opens *in order to deploy* carries none. It now carries a
      `:::danger` box directly under the public listener, and it states the three auth outcomes
      rather than the obsolete "no path turns it on".
- [x] `operate/deploy.md`'s exposure table stops advertising `TLS 5061 | public`. Already done by
      `FC-1`'s own pass and verified unchanged here: the row reads "**designed only**: a
      `transport: tls` listener is refused at load today, not served". One edit, as asked.
- [x] The status rows are re-derived rather than copy-edited. Several are now wrong in the
      *understating* direction: the README's "Reachable but not wired" says neither digest auth nor
      the PostgreSQL store "can be switched on from the binary", and the store now can be — selected
      from the document, needing the non-default `postgres` cargo feature, and refusing to start
      without it, which is a better story than the one being told. `whats-new.md`'s "there is no
      second node" and `migrate/from-kamailio.md`'s "there is no clustering in the binary you can
      build today" are contradicted by `scripts/two-node-call.sh` and by the binary's own `--help`.

      Re-derived on nine pages, from the scripts and the binary rather than from the sentences:
      `README.md` (Working / Accepted-but-not-applied / Not-yet), `intro.md` ("the cluster is not
      built yet" and the capability table), `whats-new.md`, `guides/does-this-fit.md` — whose "not
      reachable from the binary" store row is the understatement this item names —
      `migrate/from-kamailio.md`'s floor, `guides/docker-and-k3d.md` ("no clustering", one
      Deployment), `operate/deploy.md`'s what-runs-today, `clustering/registrar-shards.md` and
      `clustering/affinity-and-flows.md`. `operate/high-availability.md` kept its argument — the
      "Today there is none" heading and the replica-count paragraph are untouched — but "There is one
      node" was replaced by what the shared store does and does not buy: registration survival, and
      nothing else on that table.
- [ ] `CHANGELOG.md`'s `[Unreleased]` **Known gaps** entry "Nothing reads a cluster document at
      startup yet (`DP-10`)" is corrected, and `DP-9`/`DP-10` get their `### Added` entries. Per
      `AGENTS.md` closed stories roll up here, and the ledger currently says the work has not landed —
      which is why nothing downstream was reconciled.

      **Not done here, and deliberately: `CHANGELOG.md` is a fenced ledger this story may not edit.**
      Most of it has since been overtaken — the `0.11.0` release wrote the `DP-9`/`DP-10` entries and
      the stale `[Unreleased]` gap is gone. What is left for whoever writes the next release entry is
      that `0.11.0`'s **Known gaps** still opens with "Authentication is accepted and not applied",
      which was true at that tag and is not true now.
- [x] `Dockerfile`'s `CMD ["--help"]` rationale is rewritten. It argues from `--listen 0.0.0.0:5060`
      and ends "Every real invocation passes `--advertise`, as the manifests do." No manifest does.
      While there: `docker-and-k3d.md` says the image is built without the database driver because
      "the binary cannot use anyway" — the Dockerfile defaults `CARGO_FEATURES=postgres`, so the
      claim is inverted.

      Both rewritten and both proved against a built image. The real reason for `--help` is that
      `run` requires `--config` and the image ships no document — which one it is belongs to the
      deployment — so a default `run` exits `2` on `error: the following required arguments were not
      provided: --config <PATH>`. The manifests are cited as what they actually do,
      `args: [run, --config, /etc/sipx-clstr/cluster.yaml]` with identity from the environment.

## Progress

- **One acceptance item is blocked on a fenced file, and it is the interesting one.** `FC-4` made a
  tenant's `domains` enforceable, and **the repository's own two-node proofs do not declare the domain
  they register in**. Run here, on this branch, against a clean build:

  ```text
  ── alice registers through node A, bob through node B
  {"status":"unauthorized","error":"rejected: 403 Forbidden"}
  [FAIL] alice could not register through node A
  ```

  `scripts/two-node-call.sh` sets `DOMAIN="127.0.0.1"` and writes a document declaring
  `domains: [example.test]`. `scripts/k8s-two-node-call.sh` sets `DOMAIN="$NODE_A"` — a Service IP —
  against `deploy/devspace/manifests/node.yaml`'s `domains: [cluster.local]`, and the greeting pod in
  that manifest registers `sip:hello@$A` for the same reason. All three are fenced for this story
  (`scripts/**`, `deploy/**`), so they are left alone rather than worked around. The fix is one word
  in each: declare the domain the phones actually use, or drop `domains` to serve any.

  Until that lands, `getting-started.md` §3–4 and the `two-node-call.sh` line it offers cannot be
  executed, so that acceptance item stays open. **`scripts/e2e-call.sh` is unaffected** — it declares
  `domains: [127.0.0.1]`, which is why it still passes and why the README quick start was pointed at
  the same spelling.

- **The failing-first, in the form a docs story has one.** The README quick start's own document made
  its own output block unreachable: `[FAIL] REGISTER alice -> 403`, twice, with the node logging
  `refused a REGISTER for a domain this tenant does not serve`. After the change, the heredoc
  extracted verbatim from `README.md` and run in an empty directory reaches `RESULT: PASS`.

- **What was executed, since the gate cannot execute any of it** (`DX-12` still owns that):
  `cargo build --bin sipx-clstr --features postgres`; `--version`, `--help`, `-h`, `run --help`; the
  refusals for a missing `--config`, an unknown flag, a missing id, a missing zone, an unreadable
  document, a two-problem document, an unresolved `dsnRef`, `echo` beside a proxy role, an empty role
  set, and a wildcard `advertise`; documents declaring `admission.maxInFlightTransactions`,
  `maxBindingsPerAor`, `expiry` and `auth` (both secret-resolves and secret-absent); `sip_demo.py`
  against a host node and against the container; `scripts/two-node-call.sh`; `docker build` and the
  documented `docker run`, including the image's default `CMD`.

- **A trap found by running the two documented Docker commands in sequence**, now written down on
  `docker-and-k3d.md`: driving `sip_demo.py` from the host at a published UDP port registers fine and
  then never forwards, because the userland proxy rewrites the source address and the registrar stores
  a contact that only exists inside the container's namespace. The page said nothing, and the reader
  who copies both commands gets a `FAIL` with everything behaving correctly.

- **`operate/high-availability.md` survived on purpose**, as this story's earlier note asked: heading,
  failure-mode table and the "a replica count is not evidence" paragraph are untouched. Only "There is
  one node" changed, into what a shared store does buy (registrations outlive the process) and what it
  does not (anything else on that table, because nothing mints an affinity token).

- Considered for upstream: no. This is documentation of this platform's own configuration surface and
  deployment story; there is nothing protocol-generic in it.

- **The proof script is repaired** (`2a7aeeb`), so the one acceptance item that was an outright broken
  artefact rather than a stale page is closed. The README quick start, the six site pages and the
  status rows are all still open.
- **The status rows moved again while this story sat, and in the direction that matters.** `62ba577`
  closed `DP-9`: two nodes over one PostgreSQL location store, proved twice — `scripts/two-node-call.sh`
  as two local processes and `scripts/k8s-two-node-call.sh` as two pods in k3d — ending in a completed
  call with 24000 samples of real audio, alice registered through node-a and bob through node-b, and
  node-a forwarding to a callee whose REGISTER it never saw. So the pages saying "there is no second
  node", "a second node cooperating with the first — specified, not shipped" and "there is no
  clustering in the binary you can build today" are now wrong by a wider margin than when this story
  was written, and the README's `status-one node · no cluster yet` badge is the headline instance.
  Re-derive these rows from the scripts rather than editing the sentences.
- **Do not overcorrect.** A cross-node call proved on one machine and in one k3d cluster is not the
  HA guarantee, and `operate/high-availability.md` is careful about exactly that distinction — its
  "Today there is none" section and the "a replica count is not evidence that node loss is survivable"
  paragraph should survive this story intact. What changed is "no cluster exists" → "a two-node
  registrar-sharing cluster is proved"; what did not change is affinity tokens, trunks, media control,
  drain, or anything the `(preview)` pages describe.
- **`FC-1` narrowed one item.** `operate/deploy.md`'s `TLS 5061 | public` row now advertises a
  transport the loader *actively refuses* (`CC-V10`), which makes the row unambiguously wrong rather
  than merely unserviceable — and gives the edit a clear replacement: name it as refused, the way the
  other unshipped surface is named.

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
