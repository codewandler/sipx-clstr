---
title: "Run a node"
description: "What the command line carries, what the document carries, the two lines it prints, the exit codes, and how to read its logs."
---

# Run a node

One process, one or more listeners, one tenant. What it does comes from a
[configuration document](../reference/configuration.md); the command line says only which document and
which node this is.

```bash
sipx-clstr run --config cluster.yaml --node 1 --zone a --roles edge,registrar
```

## What the command line carries

| Flag | Environment | Meaning |
|---|---|---|
| `--config <PATH>` | — | The document: YAML, JSON or TOML. **Required.** |
| `--node <1..65535>` | `SIPX_CLSTR_NODE` | This node's logical id. |
| `--zone <NAME>` | `SIPX_CLSTR_ZONE` | Its failure domain. |
| `--roles <A,B>` | `SIPX_CLSTR_ROLES` | What it runs. |

That is the whole surface. Identity is **not** in the document, because the document is the same on
every node in the cluster — see [Configuration](../reference/configuration.md).

## What the document carries

The listener is where bind and advertise are declared, independently of each other:

```yaml
listener:
  - roles: [edge, registrar]
    transport: udp
    bind: 0.0.0.0:5060
    advertise: 203.0.113.10:5060
```

`advertise` is the one that will actually bite you. A node **refuses to start** if it would advertise
`0.0.0.0`, because "everywhere" answers where to listen and not where to be reached — a node that put
it in a `Record-Route` would accept calls that could never be transferred or hung up.
[Addressing](addressing.md) covers this properly.

A `transport` this build cannot serve — `tls`, `ws`, `wss` — is **refused**, not quietly downgraded to
cleartext.

The tenant carries what the registrar enforces: its served `domains` (a `REGISTER` whose
Request-URI or address-of-record names any other domain is answered `404`), its
`maxBindingsPerAor` quota and its `expiry` bounds. Leaving `domains` out serves any domain.

:::danger Do not put this on a public address
The listener above is the one this page is about — a public bind, advertised to a routable address —
and this node is an **open registrar**. Anyone who can reach the port can register any
address-of-record in a served domain, and take over an existing one.

`tenant[].auth` is applied, not ignored, but it cannot make this node authenticate: there is no user
credential store yet. A node whose nonce `secretRef` **resolves** refuses to start, saying so; a node
whose reference does not resolve starts, logs `auth="required"`, and challenges every `REGISTER` into a
`401` that nobody can answer. Neither is an authenticated registrar. Until credentials exist, the only
protection is the network: loopback, a private subnet, or a firewall.
:::

## What it prints

On a successful bind, two lines on **stdout**:

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

These are printed *after* the socket is bound, deliberately, so a script can block on the line
instead of sleeping:

```bash
sipx-clstr run --config cluster.yaml --node 1 --zone a --roles edge,registrar |
  while read -r line; do
    case "$line" in "listening on"*) break ;; esac
  done
```

They are printed only after **everything** that could refuse to start has declined to — the bind, the
document, the location store. It was briefly the other way round, and a script waiting on that line
proceeded against a node that was already exiting.

Both addresses appear because they are allowed to differ, and an operator debugging "the phone
registers but nothing rings" needs to see which one went into the messages.

## Logs

Logs go to **stderr**, never stdout, so the two lines above stay parseable. Level comes from
`RUST_LOG`, defaulting to `info`:

```bash
RUST_LOG=debug sipx-clstr run --config cluster.yaml --node 1 --zone a --roles edge,registrar
```

Colour is switched off on purpose — this log is read by scripts as often as by people, and
escape codes between a field name and its value defeat an honest `grep`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean exit. |
| `1` | Runtime failure — the bind failed, or the runtime could not start. |
| `2` | Anything wrong before the node ran: an unknown flag, a missing `--config` or identity, an unreadable document, a document that was refused, or a secret reference that did not resolve. |

A node that cannot do what it was asked must not look like a node that did it, so a
misconfiguration is `2` and never `0`.

## What it is not

There is no reload, no management port, and no metrics endpoint. A configuration change is a restart.
Authentication is applied but has no credential store behind it, so there is no way to configure a
registrar that authenticates. All of those are specified; none is implemented — see
[Configuration](../reference/configuration.md) for the section-by-section state.
