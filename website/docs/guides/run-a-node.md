---
title: "Run a node"
description: "The three flags, the two lines it prints, the exit codes, and how to read its logs — the whole operational surface of a sipx-clstr node today."
---

# Run a node

One process, one listener, one tenant. The whole surface is three flags.

```bash
sipx-clstr run --listen 0.0.0.0:5060 --advertise 203.0.113.10:5060
```

That binds **both UDP and TCP** on the given address. There is no flag to bind only one.

## The flags

| Flag | Default | What it does |
|---|---|---|
| `--listen <addr>` | `0.0.0.0:5060` | The socket to bind. UDP and TCP both. |
| `--advertise <host[:port]>` | *(none)* | What goes into `Via` and `Record-Route`. **Required** when `--listen` is unspecified (`0.0.0.0` or `::`). |
| `--tenant <name>` | `default` | The tenant this node serves. One tenant per node, for now. |

`--advertise` is the one that will actually bite you. The node **refuses to start** on
`0.0.0.0` without it, because "everywhere" answers where to listen and not where to be reached —
a node that put `0.0.0.0` in a `Record-Route` would accept calls that could never be transferred
or hung up. [Addressing](addressing.md) covers this properly.

`--tenant` sets a name and nothing else. It does not enable authentication, and it does not
isolate anything from anything: this node has one tenant and no credentials.

## What it prints

On a successful bind, two lines on **stdout**:

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

These are printed *after* the socket is bound, deliberately, so a script can block on the line
instead of sleeping:

```bash
sipx-clstr run --listen 127.0.0.1:5060 --advertise 127.0.0.1:5060 |
  while read -r line; do
    case "$line" in "listening on"*) break ;; esac
  done
```

Both addresses appear because they are allowed to differ, and an operator debugging "the phone
registers but nothing rings" needs to see which one went into the messages.

## Logs

Logs go to **stderr**, never stdout, so the two lines above stay parseable. Level comes from
`RUST_LOG`, defaulting to `info`:

```bash
RUST_LOG=debug sipx-clstr run --listen 127.0.0.1:5060 --advertise 127.0.0.1:5060
```

Colour is switched off on purpose — this log is read by scripts as often as by people, and
escape codes between a field name and its value defeat an honest `grep`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean exit. |
| `1` | Runtime failure — the bind failed, or the runtime could not start. |
| `2` | Usage or configuration error: an unknown flag, a flag with no value, an address that does not parse, or `0.0.0.0` with no `--advertise`. |

A node that cannot do what it was asked must not look like a node that did it, so a
misconfiguration is `2` and never `0`.

## What it is not

There is no config file, no reload, no management port, no metrics endpoint, and no way to
enable authentication or point the registrar at a database. All of those are specified; none is
implemented. See [Configuration](../reference/configuration.md) for what exists versus what is
designed.

The argument parser itself is explicitly provisional. The real configuration schema is a
[normative spec](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md)
that will **replace** these flags rather than extend them, so do not build tooling that depends
on their shape.
