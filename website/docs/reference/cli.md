---
title: "CLI reference"
description: "Every command, flag, exit code and output line the sipx-clstr binary offers — produced by running it, not read off the source."
---

# CLI reference

Two commands. Everything a node does is decided by its
[configuration document](configuration.md); the command line only says *which* document and *which
node this is*.

```text title="sipx-clstr -h"
A clustered SIP proxy and registrar

Usage: sipx-clstr [COMMAND]

Commands:
  run   Run a node from a cluster configuration document
  help  Print this message or the help of the given subcommand(s)

Options:
  -V, --version  Print the version and the sipx kernel it is built against
  -h, --help     Print help (see more with '--help')
```

`--help` prints the same thing with the long descriptions, and `run --help` documents where identity
and secrets come from. Both exit `0`.

## `sipx-clstr run`

```bash
sipx-clstr run --config cluster.yaml --node 1 --zone a --roles edge,registrar
```

| Flag | Environment | Meaning |
|---|---|---|
| `--config <PATH>` | — | The configuration document: YAML, JSON or TOML. **Required.** |
| `--node <1..65535>` | `SIPX_CLSTR_NODE` | This node's logical id. `0` is reserved. |
| `--zone <NAME>` | `SIPX_CLSTR_ZONE` | Its failure domain. |
| `--roles <A,B>` | `SIPX_CLSTR_ROLES` | What it runs, comma-separated. |

A flag wins over its environment variable — the command line is the more specific statement. The
variables exist so a Kubernetes manifest can supply identity from the downward API without wrapping
the binary in a shell.

**Identity is never read from the document**, because the document is the same on every node. A
document that could name which node is reading it would be a per-node document, and then its version
number would stop being a fact about the cluster.

### Roles

`edge` · `registrar` · `inbound-proxy` · `outbound-proxy` · `e2e-tester` · `echo`

A role selects which decision paths are wired, never what a request decides — that is the schema's
rule. Any combination is allowed except that **`echo` and `e2e-tester` are refused beside any proxy
role**: a probe that enters through the node it is probing measures a path no caller takes.

An empty role set is refused too: a node that runs nothing should not have been started. So is a
role this build has **no runtime for** — `echo` and `e2e-tester` stop the node at startup by name,
whatever they are combined with, rather than being accepted and quietly served as something else.

What the binary does with the rest of the set: it derives a capability set from it and dispatches
through that, so a node without `registrar` answers a `REGISTER` with `405` and an `Allow` header
naming the methods its roles do wire. The refusal *shape* — `503` with `Retry-After`, and `481` for
an unmatched `CANCEL` — and the counted `ACK` drop are `DP-13`'s and are not what this build sends
today.

### What it prints

On a successful start, two lines on **stdout**:

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

They are printed only after everything that could refuse to start has declined to — the bind, the
document, the location store. A script can therefore wait for `listening on` rather than sleeping.
It was briefly the other way round, and a script that waited for that line proceeded against a node
that was already exiting.

Logs go to **stderr**, never stdout, so those two lines stay parseable. Level comes from `RUST_LOG`,
default `info`. Colour is off on purpose: this log is read by scripts as often as by people, and
escape codes between a field name and its value defeat an honest `grep`.

## Secrets

The document never contains a credential — it names one:

```yaml
locationStore:
  backend: postgres
  dsnRef: location-dsn
```

`dsnRef: location-dsn` is resolved from the environment variable `LOCATION_DSN` (uppercased, with
`-`, `.` and `/` becoming `_`). A reference that does not resolve **stops the node**:

```text
sipx-clstr: cluster.locationStore.dsnRef names `location-dsn`, which is not set in the environment as `LOCATION_DSN`
```

The log reports the backend by name and never the resolved value, so a log pasted into an issue does
not carry a password.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean exit, or `--version` / `--help`. |
| `1` | Runtime failure — the bind failed, or the async runtime could not start. |
| `2` | Anything wrong before the node ran: an unknown flag, a missing `--config`, a missing identity, an unreadable document, a document that was refused, or a reference that did not resolve. |

A node that cannot do what it was asked must not look like a node that did it, so every
misconfiguration is `2` and never `0`.

### What a refusal looks like

A document is validated as a whole, so **every** problem is reported, ordered by path — five mistakes
cost one restart rather than five:

```text
sipx-clstr: cluster.yaml was refused — 2 problem(s):
  cluster.bogusKey [CC-V2]: found bogusKey, expected one of: name, environment, zones, listener, …
  cluster.locationStore.dsnRef [CC-V9]: expected a dsnRef; a store that is not in-process needs one
```

Each line names the path in the document's own spelling and the rule that rejected it. Other
refusals:

```text
sipx-clstr: this node has no id: pass --node <1..65535> or set SIPX_CLSTR_NODE
sipx-clstr: cannot read /etc/sipx/cluster.yaml: No such file or directory (os error 2)
```

## `sipx-clstr --version`

```text
sipx-clstr 0.14.0 (sipx kernel 1.0.0-beta.5)
```

The second version is the [sipx](https://github.com/codewandler/sipx) protocol kernel this binary was
built against, pinned to a tag rather than a branch.

## Build features

`--features postgres` compiles in the shared location service. Without it, a node asking for one is
**refused** rather than quietly given its own in-process store:

```text
sipx-clstr: the configured location store could not be reached: this binary was built without the `postgres` feature
```

The container image builds with it, because a cluster of more than one node needs a shared store.

## What is not here

There is no reload, no management port, no metrics endpoint, and no subcommand that inspects a
running node. All are specified; none is implemented. See
[Observability](../operate/observability.md).
