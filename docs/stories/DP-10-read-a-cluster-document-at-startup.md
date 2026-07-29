---
id: DP-10
title: Read a cluster document at startup and replace the provisional flags
pillar: Cluster
status: done
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, build]
note: the missing link — DP-8 reads a document, RG-12 can use one, and nothing connects them yet
---

# Read a cluster document at startup and replace the provisional flags

## Goal

Connect the two halves that already exist: `DP-8`'s loader parses a cluster document, and `RG-12`'s
`StoreChoice` can act on one, but `main.rs` still builds its config from three flags and never reads
a file. Until it does, neither is reachable from a running node, and a multi-node deployment cannot
be configured at all.

## Acceptance

- [x] The binary takes a path to a cluster document and its identity from the environment, and
      builds `NodeConfig` from `load` + `project` rather than from flags.
- [x] `NodeIdentity` comes from **outside** the document (cluster-config §5 P1): the node id, zone
      and role set. In Kubernetes these come from the downward API; on a plain host, from the command
      line. The document must not be able to name which node is reading it.
- [x] `dsnRef` is **resolved here**, not in the loader — V9 puts resolution in the driver because it
      is IO. A reference that does not resolve is a start-up failure with a message naming the
      reference, not a fallback to the in-memory store.
- [x] Every error the loader returns is printed, ordered by path, before the process exits `2`.
      Printing only the first would waste the property `DP-8` exists to provide.
- [x] The provisional flags are **replaced, not extended** — `main.rs` says the schema replaces them,
      and `cli.md` on the published site says so too. A transition period where both work is a second
      configuration surface, which is the thing being removed.
- [x] `--version` and `--help` still work without a document; a node that cannot say what it is is
      worse than one that cannot start.
- [x] **Failing-first**: a test starts the binary against a document naming a `postgres` store whose
      `dsnRef` does not resolve, and asserts exit `2` with the reference named — proving the document
      reached the store choice rather than being ignored.
- [ ] The published pages that describe configuration are updated in the same change: **not done** —
      `website/docs/reference/cli.md` and `website/docs/reference/configuration.md` both currently
      say there is no configuration file.

## Progress

- **Done.** `startup.rs` is the seam: read the file, read the environment, resolve the references the
  document deliberately does not contain, and hand the driver a `NodeConfig`. The loader stays pure.
- **Verified by running it**, not by reading it: a node starts from YAML, from TOML, and with its
  identity from environment variables instead of flags; `sip_demo.py` places a real call against each.
- **Three refusal paths exercised**: no identity (names the flag *and* the variable), a document with
  several faults (all of them, ordered by path), and a `dsnRef` that does not resolve (names the
  reference and the variable it looked for). All exit `2`.
- **`clap` replaced the hand-rolled parser.** The old one carried a comment justifying itself as
  "deliberately tiny and provisional"; that justification expired the moment the surface grew node
  identity semantics. The environment fallback stays in `or_env` rather than a `clap(env = …)`
  attribute, so one tested function decides how a flag and a variable combine.
- **TOML is now a third encoding**, and `cluster-config` §2 D3 was updated in the same change rather
  than left saying two. The converter turns TOML into the same value tree the other two produce, so
  validation has exactly one path; a test asserts the whole `Config` is equal across encodings, not a
  spot check. Encoding is detected from the bytes, never the file name.
- **Two defects found by running it, both fixed here:**
  1. The node printed `listening on` — the documented readiness signal — and *then* exited when the
     store was unreachable, because the store opened after the announcement. A script waiting on that
     line proceeded against a dying node. Everything that can refuse to start now refuses first.
  2. Logging the store choice with `?config.store` would have printed the resolved DSN, password and
     all, into the log — undoing V9 in the one artefact most likely to be pasted into an issue. It
     logs a backend name now.
- **`RG-4`'s store could not be driven from the node at all**, which nothing had noticed because its
  tests are synchronous. The blocking `postgres` client builds a runtime and `block_on`s, so the first
  real call panicked with *"Cannot start a runtime from within a runtime"*. Worse, wrapping `connect`
  in `block_in_place` did not fail loudly — it produced `error communicating with the server`, a
  connection nobody was driving. It is opened on a plain thread with no tokio context, and its queries
  go through a `BlockingStore` adapter. The honest fix is an async store on `tokio-postgres`; that
  changes the trait for every backend and is a story, not a patch.
- The Dockerfile now builds the `postgres` feature, because a cluster of more than one node needs a
  shared store and the image could otherwise only ever run a single node.
- Considered for upstream: no. Configuration, identity and startup are this platform's own.

### Not done here

The published pages still say there is no configuration file, and the README quick start still shows
the old flags. Eight files describe the surface this story changed. They are deliberately left for one
pass together with a release, because the site deploys from a tag: editing them now would make the
site describe a binary nobody can download yet.

### The blast radius, measured rather than guessed

"Replace, not extend" reaches well past `main.rs`. Every one of these invokes the provisional flags
today and has to move in the same change, or the repository ships two configuration surfaces:

| File | What it does with them |
|---|---|
| `crates/sipx-clstr-node/src/main.rs` | parses `--listen`/`--tenant`/`--advertise` |
| `scripts/e2e-call.sh` | starts the node under test — the M1 proof |
| `deploy/devspace/manifests/node.yaml` | `args: [run, --listen, …, --advertise, $(POD_IP):5060]` |
| `Dockerfile` | the `--help` default and its explanation |
| `README.md` | the quick start, with pasted output |
| `website/docs/getting-started.md` | the five-minute path, with pasted output |
| `website/docs/guides/run-a-node.md` | the flag table and exit codes |
| `website/docs/guides/addressing.md` | the whole page is about `--advertise` |
| `website/docs/guides/docker-and-k3d.md` | container and pod invocation |
| `website/docs/reference/cli.md` | the full documented surface |
| `website/docs/reference/configuration.md` | says outright there is no configuration file |

Two consequences worth deciding before starting. The published site is deployed from a **release**,
so the site and the binary disagree between the merge and the next tag unless the two are cut
together. And `addressing.md` does not survive a mechanical edit — it is an argument about bind
versus advertise, and the document form has to make that argument in its own shape rather than
rename the flags in place.

## Notes

- `DP-8` implemented ten sections of the schema and reports the rest in `Config::deferred`. This
  story does not need the deferred ones, but it must not silently drop them either — if a document
  names a section this node would need and cannot yet validate, say so at startup.
- `AuthConfig` has the same shape of gap as the store did: digest is implemented and `main.rs` never
  sets it, so the shipped binary is an open registrar. Wiring `tenant[].auth` is the obvious
  companion to this story and is deliberately **not** folded into it — it is a security-behaviour
  change and deserves its own failing-first test and its own line in the changelog.
- Once this lands, `DP-9` (two nodes, one store, a cross-node call proved with `sipx` CLI phones) is
  unblocked.
