---
title: "Addressing — bind vs advertise"
description: "The one flag that will actually bite you. Why a node refuses to start on 0.0.0.0, and what to advertise behind NAT, in Docker, and in Kubernetes."
---

# Addressing — bind vs advertise

Most first-time failures with this node are this one thing. A phone registers, everything looks
healthy, and then nothing ever rings.

## Two different questions

Both are declared on a listener in the [configuration document](../reference/configuration.md), and
neither is derived from the other:

```yaml
listener:
  - roles: [edge, registrar]
    transport: udp
    bind: 127.0.0.1:5060
    advertise: 127.0.0.1:5060
```

**`bind` is where the socket goes.** It is a local concern: which interface, which port.

**`advertise` is where other people reach you.** It is what the node writes into its `Via` header and
into `Record-Route`. Those are the addresses a peer uses to send responses back and to route in-dialog
requests like `BYE`.

On loopback they are identical and the distinction is invisible, as above.

Everywhere else, they usually differ.

## Why it refuses to start on 0.0.0.0

```yaml
listener:
  - roles: [edge, registrar]
    transport: udp
    bind: 0.0.0.0:5060
    # advertise omitted
```

```text
sipx-clstr: cluster.listener: `0.0.0.0` cannot be advertised: an unspecified address is where to listen, not where to be reached
```

Exit code `2`, and the message names the path in the document that has to change. This is deliberate. `0.0.0.0` is a valid answer to "where should I listen" —
every interface — and a meaningless answer to "where can you be reached". A node that put
`0.0.0.0` into a `Record-Route` would take calls that could never be transferred or hung up,
because every in-dialog request would be addressed to nowhere.

Failing at startup is much cheaper than discovering it when someone tries to end a call.

## What to advertise where

| Situation | `bind` | `advertise` |
|---|---|---|
| Loopback development | `127.0.0.1:5060` | `127.0.0.1:5060` |
| One host, one interface | `0.0.0.0:5060` | the host's own address, e.g. `203.0.113.10:5060` |
| Behind a 1:1 NAT | `0.0.0.0:5060` | the **public** address and port |
| Docker with published ports | `0.0.0.0:5060` | the **host** address and published port |
| Kubernetes | `0.0.0.0:5060` | the pod IP, from the downward API |

In Kubernetes that last row is why the document supports substitution. Every node reads the *same*
document, and each resolves `${POD_IP}` to its own address:

```yaml
# in the ConfigMap
listener:
  - roles: [edge, registrar]
    transport: udp
    bind: 0.0.0.0:5060
    advertise: ${POD_IP}:5060
```

```yaml
# in the pod spec
env:
  - name: POD_IP
    valueFrom:
      fieldRef:
        fieldPath: status.podIP
```

An undefined variable is an error naming it — never the empty string, which would turn
`advertise: "${POD_IP}:5060"` into an unparsable address and report the wrong problem.

See [Docker and k3d](docker-and-k3d.md) for the rest of that manifest.

## It applies to clients too

A phone that binds `0.0.0.0` puts the wildcard in its own `Contact`, and then nothing can route the
answer back to it. Bind an explicit address on the client side for the same reason the node does.

## Diagnosing it

The node prints both addresses at startup precisely so you can tell them apart:

```text
listening on 0.0.0.0:5060
advertising 203.0.113.10:5060
```

If registrations succeed but calls never arrive, look at the second line first, then look at what
your phone actually received. The symptom of a wrong `advertise` is always the same shape:
requests reach the node fine, and everything the node sends *back* is addressed somewhere the
other side cannot use.

## Ports

`advertise` takes `host` or `host:port`. With no port it advertises the port from `bind`.
Give the port explicitly whenever a NAT or a container remaps it — the outside port is the one
that belongs in the message, not the one you bound.
