---
id: DP-10
title: Read a cluster document at startup and replace the provisional flags
pillar: Cluster
status: ready
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

- [ ] The binary takes a path to a cluster document and its identity from the environment, and
      builds `NodeConfig` from `load` + `project` rather than from flags.
- [ ] `NodeIdentity` comes from **outside** the document (cluster-config §5 P1): the node id, zone
      and role set. In Kubernetes these come from the downward API; on a plain host, from the command
      line. The document must not be able to name which node is reading it.
- [ ] `dsnRef` is **resolved here**, not in the loader — V9 puts resolution in the driver because it
      is IO. A reference that does not resolve is a start-up failure with a message naming the
      reference, not a fallback to the in-memory store.
- [ ] Every error the loader returns is printed, ordered by path, before the process exits `2`.
      Printing only the first would waste the property `DP-8` exists to provide.
- [ ] The provisional flags are **replaced, not extended** — `main.rs` says the schema replaces them,
      and `cli.md` on the published site says so too. A transition period where both work is a second
      configuration surface, which is the thing being removed.
- [ ] `--version` and `--help` still work without a document; a node that cannot say what it is is
      worse than one that cannot start.
- [ ] **Failing-first**: a test starts the binary against a document naming a `postgres` store whose
      `dsnRef` does not resolve, and asserts exit `2` with the reference named — proving the document
      reached the store choice rather than being ignored.
- [ ] The published pages that describe configuration are updated in the same change:
      `website/docs/reference/cli.md` and `website/docs/reference/configuration.md` both currently
      say there is no configuration file.

## Progress

- (running log)

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
