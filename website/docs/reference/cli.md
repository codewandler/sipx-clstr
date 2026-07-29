---
title: "CLI reference"
description: "Every command, flag, exit code and output line the sipx-clstr binary offers today."
---

# CLI reference

This is the complete surface. One command with three flags, two top-level switches, one
environment variable, three exit codes. Anything not on this page does not exist — there is no
hidden flag, no config file, and no subcommand you have not been told about.

:::caution The parser is provisional
The argument surface is deliberately tiny and explicitly temporary. The real configuration schema
is a [normative spec](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/cluster-config.md),
and it **replaces** these flags rather than extending them: no flag on this page is promised a
successor. When it lands, a node is configured by a document, not by a command line. Do not build
tooling on the shape of what follows —
script against the [exit codes](#exit-codes) and the [stdout contract](#what-it-prints), which are
the parts meant to be depended on.
:::

## Synopsis

```text
sipx-clstr run [--listen <addr>] [--advertise <host[:port]>] [--tenant <name>]
sipx-clstr --version | -V
sipx-clstr --help | -h
sipx-clstr
```

`run` is positional and must come first. A flag on its own is not a command:

```console
$ sipx-clstr --tenant acme
sipx-clstr: unknown argument `--tenant`
There is no configuration surface yet — try --help.
```

That exits `2`.

## `run`

Starts a node: the registrar and the forwarding core on one listener, over **both UDP and TCP**.
There is no flag to bind only one of them.

| Flag | Default | What it does |
|---|---|---|
| `--listen <addr>` | `0.0.0.0:5060` | The socket to bind. Must be an `address:port` — a bare port is refused. |
| `--advertise <host[:port]>` | *(none — the bound address)* | What the node writes into `Via`, `Contact` and `Record-Route`. An omitted port means the port bound, never `5060`. |
| `--tenant <name>` | `default` | The tenant every registration on this node belongs to. |

Those three are the whole of `run`. `--version` after `run` is not one of them, and every option
inside `run` is a flag-and-value pair, so a flag standing alone is reported as a missing value
rather than as a bad flag:

```console
$ sipx-clstr run --version
sipx-clstr: --version needs a value
```

### `--listen`

Binds one address for the cleartext pair. `0.0.0.0:5060` binds every interface, which is the
default and is also why the default alone will not start a node — see `--advertise`.

### `--advertise`

The flag that will actually bite you. Bind and advertise are declared independently, and neither
is derived from the other: on a NAT'd host or a container the address a socket binds and the
address a peer can reach are different facts.

Without `--advertise` the node advertises what it binds, and an unspecified address cannot be
advertised at all:

```console
$ sipx-clstr run
sipx-clstr: `0.0.0.0` cannot be advertised: an unspecified address is where to listen, not where to be reached
Pass --advertise <host[:port]> with the address peers reach this node on.
```

"Everywhere" answers where to listen and not where to be reached, and a node that put `0.0.0.0`
into a `Record-Route` would accept calls that could never be transferred or hung up. Port `0` is
refused for the same reason: it names no port.

A host with no port is fine, and takes the port that was bound:

```console
$ sipx-clstr run --listen 127.0.0.1:5064 --advertise edge-1.example
listening on 127.0.0.1:5064
advertising edge-1.example:5064
```

[Addressing](../guides/addressing.md) covers the decision properly.

### `--tenant`

Sets a name and nothing else. It does not enable authentication and it isolates nothing from
anything: this node has one tenant, and that tenant has no credentials. The tenant never comes
from the message — a registrar that read its tenant from a URI would let a caller choose whose
bindings to write.

## `--version`

```console
$ sipx-clstr --version
sipx-clstr 0.10.0 (sipx kernel 0.7.0)
```

`-V` is the same thing. The second version is the [sipx](https://github.com/codewandler/sipx)
protocol kernel this binary was built against, pinned to a tag — which is the question you
actually want answered during an incident, and it is not one you can read off a lockfile you do
not have.

## `--help`

`--help`, `-h`, and no arguments at all print the same text and exit `0`:

```console
$ sipx-clstr --help
sipx-clstr 0.10.0 — a clustered SIP proxy and registrar

One node registers users and proxies calls between them. The cluster — affinity
tokens, trunks, media control — is specified but not implemented; see the docs.

  run --listen <addr>   run a node: registrar and proxy on one listener
      --tenant <name>            the tenant this node serves (default:
                                 `default`); registrations are not
                                 authenticated — this node is an open
                                 registrar, and bindings are lost on restart
      --advertise <host[:port]>  what peers reach it on, if that is not the
                                 address it binds — a node behind a NAT or on a
                                 private address must say so, or its Via and
                                 Record-Route name somewhere unreachable
  --version             print the version and the kernel it is built against
```

## What it prints

**stdout carries exactly two lines, and only on a successful bind:**

```console
$ sipx-clstr run --listen 127.0.0.1:5062 --advertise 203.0.113.10:5060 --tenant acme
listening on 127.0.0.1:5062
advertising 203.0.113.10:5060
```

Both lines are printed **after** the socket is bound. That is the whole contract: a script can
block on `listening on` instead of sleeping and hoping, and a node that failed to bind never
prints it. Printing before the bind would make a failed start look like a successful one — which
it did, until a test of the failure path caught the node saying "listening" and then dying.

Both addresses appear because they are allowed to differ, and an operator debugging "the phone
registers but nothing rings" needs to see which one went into the messages.

```bash
sipx-clstr run --listen 127.0.0.1:5060 --advertise 127.0.0.1:5060 |
  while read -r line; do
    case "$line" in "listening on"*) break ;; esac
  done
```

## Logs

**Everything else goes to stderr**, so those two lines stay parseable:

```text
2026-07-29T19:17:16.514870Z  INFO sipx_clstr_node::driver: node listening listen=127.0.0.1:5062 advertised=203.0.113.10:5060 tenant=acme
2026-07-29T19:17:17.021082Z  INFO sipx_clstr_node::driver: transactions in flight outstanding=0
```

`transactions in flight` is emitted when the number changes, not on a schedule. A proxy that leaks
one transaction per call is a slow, quiet outage; a count that only appears when it moves is a
record of what the node did.

**There is no ANSI colour, ever.** This log is read by scripts as often as by people, and escape
codes between a field name and its value defeat an honest `grep`.

Level comes from `RUST_LOG`, defaulting to `info`. It is the only environment variable the binary
reads. Silencing the logs leaves stdout untouched:

```bash
RUST_LOG=warn sipx-clstr run --listen 127.0.0.1:5063 --advertise 127.0.0.1:5063
```

```text
listening on 127.0.0.1:5063
advertising 127.0.0.1:5063
```

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. `--help` and `--version` printed, or the node stopped cleanly. |
| `1` | Runtime failure. The bind failed, or the async runtime could not start. |
| `2` | Usage or configuration error. Nothing was started. |

A node that cannot do what it was asked must not look like a node that did it, so every
misconfiguration below is `2` and never `0`.

**Exit `1` — it was configured correctly and could not run:**

```console
$ sipx-clstr run --listen 127.0.0.1:5062 --advertise 127.0.0.1:5062
sipx-clstr: io: Address already in use (os error 98)
```

**Exit `2` — the whole set, each with the message it prints on stderr:**

| Cause | Message |
|---|---|
| An unknown command or leading flag | ``sipx-clstr: unknown argument `serve` `` |
| An unknown option after `run` | ``sipx-clstr: unknown option `--port` `` |
| A flag with no value | `sipx-clstr: --listen needs a value` |
| A listen address that is not `address:port` | ``sipx-clstr: `5060` is not an address:port `` |
| An unspecified address with no `--advertise` | ``sipx-clstr: `0.0.0.0` cannot be advertised: an unspecified address is where to listen, not where to be reached`` |
| An advertised port of `0` | `sipx-clstr: port 0 cannot be advertised: it names no port` |

Each of the last two also prints the same second line:

```text
Pass --advertise <host[:port]> with the address peers reach this node on.
```

## What is not here

No configuration file. No reload. No management port, no metrics endpoint, no health endpoint.
No flag that enables authentication, points the registrar at a database, declares a trunk, or
joins a cluster. Those are specified in detail and none of them is reachable from this binary —
[Configuration](configuration.md) is the honest inventory of what exists versus what is designed.

The TLS listener is the sharpest edge of that: the node can *decide* everything about a TLS
listener, but it cannot be given a certificate, so it cannot be declared here. Certificate
material is the configuration schema's.
