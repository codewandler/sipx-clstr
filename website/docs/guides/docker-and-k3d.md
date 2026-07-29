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
docker run --rm -p 5060:5060/udp -p 5060:5060/tcp sipx-clstr \
  run --listen 0.0.0.0:5060 --advertise 203.0.113.10:5060
```

Note that you must supply the command. The image's default is `--help`, not `run`, on purpose: a
node refuses to advertise an unspecified address, so every plausible default `run` invocation
would exit `2`. A default that always fails teaches nothing, so the image prints its usage
instead.

Set `--advertise` to the address **outside** the container — the host address and published
port. See [Addressing](addressing.md).

## On k3d with devspace

```bash
devspace deploy      # or: devspace dev, which also streams logs
devspace run demo    # runs scripts/sip_demo.py against it from inside the cluster
```

The manifests live in `deploy/devspace/` and put the node in a `sipx-clstr-dev` namespace as a
plain `Deployment` with one replica and a `ClusterIP` service. The container runs with a
read-only root filesystem and all capabilities dropped, and is not on the host network.

The interesting line is the advertise argument:

```yaml
args: [run, --listen, 0.0.0.0:5060, --advertise, $(POD_IP):5060, --tenant, default]
```

The pod's own IP is not knowable before the pod exists, so it comes from the downward API and is
expanded into the argument at start. This is the real-world version of the lesson in
[Addressing](addressing.md): bind is local, advertise is what peers use.

## What this proves, and what it does not

It stands up **one process**, on **UDP only** in practice, with the **in-memory** location store
and no authentication.

It does not prove clustering, zone spread, media, high availability of the store, TCP or TLS, or
anything an operator would do — because there is no operator here. The Helm chart in
`deploy/helm/` renders a custom resource that nothing currently serves; the controller that would
reconcile it is designed but not built.

For what that is supposed to become, see [Deploy](../operate/deploy.md) and
[Scaling](../operate/scaling.md).
