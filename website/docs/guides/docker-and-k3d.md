---
title: "Docker and k3d"
description: "Build the container, run one node on a local Kubernetes cluster with devspace, and understand what that does and does not prove."
---

# Docker and k3d

The container runs the same single node as everything else on this site. It is a development
environment, not a deployment story — there is no operator, no clustering, and nothing that
scales.

## Build the image

```bash
docker build -t sipx-clstr .
```

Two stages: a Rust builder and a slim Debian runtime. The build happens **inside** the image
rather than copying a host binary in, because a binary linked against your host's glibc will not
reliably run in the runtime image.

It builds only the node binary, and deliberately not with `--all-features` — the feature set
pulls in a database driver the binary cannot use anyway.

At runtime it drops to an unprivileged user, runs `tini` as PID 1, and exposes `5060` on both
UDP and TCP.

```bash
docker run --rm -p 5060:5060/udp -p 5060:5060/tcp \
  -v "$PWD/cluster.yaml:/etc/sipx-clstr/cluster.yaml:ro" sipx-clstr \
  run --config /etc/sipx-clstr/cluster.yaml --node 1 --zone a --roles edge,registrar
```

Note that you must supply the command. The image's default is `--help`, not `run`, on purpose: a
node refuses to advertise an unspecified address, so every plausible default `run` invocation
would exit `2`. A default that always fails teaches nothing, so the image prints its usage
instead.

Set the listener's `advertise` to the address **outside** the container — the host address and
published port. See [Addressing](addressing.md). The image builds with the `postgres` feature, so a
container can join a cluster rather than only ever running alone.

## On k3d with devspace

```bash
devspace deploy      # or: devspace dev, which also streams logs
devspace run demo    # runs scripts/sip_demo.py against it from inside the cluster
```

The manifests live in `deploy/devspace/` and put the node in a `sipx-clstr-dev` namespace as a
plain `Deployment` with one replica and a `ClusterIP` service. The container runs with a
read-only root filesystem and all capabilities dropped, and is not on the host network.

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
its own pod IP, so one address in front of both needs affinity tokens.

For what that is supposed to become, see [Deploy](../operate/deploy.md) and
[Scaling](../operate/scaling.md).
