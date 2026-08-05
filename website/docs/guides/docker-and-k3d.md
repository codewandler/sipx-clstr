---
title: "Docker and k3d"
description: "Build the container, run two nodes over one location service on a local Kubernetes cluster with devspace, and understand what that does and does not prove."
---

# Docker and k3d

The container runs the same node as everything else on this site, and the k3d profile runs **two** of
them over one PostgreSQL location service. It is still a development environment rather than a
deployment story: there is no operator, nothing autoscales, and one address in front of the two nodes
does not work.

## Build the image

```bash
docker build -t sipx-clstr .
```

Two stages: a Rust builder and a slim Debian runtime. The build happens **inside** the image
rather than copying a host binary in, because a binary linked against your host's glibc will not
reliably run in the runtime image.

It builds only the node binary, and deliberately not with `--all-features`, which would pull in
test-only surface. It **does** build the `postgres` feature (`CARGO_FEATURES=postgres`), because a
cluster of more than one node needs a shared location service and without the feature the node refuses
to start on a document asking for one — an image that could only ever run alone.

At runtime it drops to an unprivileged user, runs `tini` as PID 1, and exposes `5060` on both
UDP and TCP.

```bash
docker run --rm -p 5060:5060/udp -p 5060:5060/tcp \
  -v "$PWD/cluster.yaml:/etc/sipx-clstr/cluster.yaml:ro" sipx-clstr \
  run --config /etc/sipx-clstr/cluster.yaml --node 1 --zone a --roles edge,registrar
```

Note that you must supply the command, and the document. The image's default is `--help`, not `run`,
on purpose: `run` requires `--config`, the image ships no document — which one it is belongs to the
deployment — so a default `run` would exit `2` on
`error: the following required arguments were not provided: --config <PATH>`. A default that always
fails teaches nothing, so the image prints its usage instead.

Set the listener's `advertise` to the address **outside** the container — the host address and
published port. See [Addressing](addressing.md).

:::note Do not drive it from the host over a published UDP port
`REGISTER` will succeed and the `INVITE` will not be forwarded. Docker's userland proxy rewrites the
source address, so the contact the registrar stores is the address the *container* saw, which is
inside its own network namespace and unreachable from the host. Run the demo from another container
on the same network — which is what `devspace run demo` does — or run the node from the binary when
you want to poke at it from your machine.
:::

## On k3d with devspace

```bash
devspace deploy      # or: devspace dev, which also streams logs
devspace run demo    # runs scripts/sip_demo.py against it from inside the cluster
```

The manifests live in `deploy/devspace/` and put the nodes in a `sipx-clstr-dev` namespace as **two**
plain `Deployment`s of one replica each — a node's id, zone and roles come from outside the document,
and a Deployment cannot give its replicas distinct ids — plus a `ClusterIP` service for each and the
location store. The containers run with a read-only root filesystem and all capabilities dropped, and
are not on the host network.

The interesting part is that **both nodes read one document**, mounted from a `ConfigMap`, and each
resolves its own address out of it:

```yaml
args: [run, --config, /etc/sipx-clstr/cluster.yaml]
env:
  - name: SIPX_CLSTR_NODE
    value: "1"
  - name: SIPX_CLSTR_ZONE
    value: a
  - name: SIPX_CLSTR_ROLES
    value: edge,registrar
  - name: POD_IP
    valueFrom:
      fieldRef:
        fieldPath: status.podIP
```

The pod's own IP is not knowable before the pod exists, so it comes from the downward API and the
document's `advertise: ${POD_IP}:5060` resolves against it. Identity comes from the environment for the
same reason it is not in the document: the document is shared, the identity is not.

## What this proves, and what it does not

It stands up **two nodes and one PostgreSQL location service**, on **UDP only**, with no
authentication. That is enough to be a cluster in the smallest honest sense: a user who registers
through one node can be called through the other, and
[Getting started](../getting-started.md) walks through hearing it answer.

It does not prove zone spread, media relaying, high availability of the store, TCP or TLS, or anything
an operator would do — because there is no operator here. The Helm chart in `deploy/helm/` renders a
custom resource that nothing currently serves; the controller that would reconcile it is designed but
not built. It also does not prove mid-dialog routing behind a single Service: each node record-routes
its own pod IP. The affinity token and simulated two-edge route path exist, but one address in front
of both still needs the loaded key set applied at runtime and a connection-owner delivery hop.

For what that is supposed to become, see [Deploy](../operate/deploy.md) and
[Scaling](../operate/scaling.md).
