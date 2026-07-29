---
id: KO-13
title: Run a node in a container and a devspace loop
pillar: Cluster
status: done
priority: 2
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: the first time any of this runs outside a test — no operator, no CRD
---

# Run a node in a container and a devspace loop

## Goal
Make the platform *runnable* by somebody who has not read it: a container image, a development loop
against a local cluster, and a scripted run that proves a node registers two users and proxies a
call between them.

## Acceptance
- [x] A `Dockerfile` builds the `sipx-clstr` binary and produces an image that runs it unprivileged.
- [x] A devspace profile builds that image and deploys a node to a local cluster.
- [x] A scripted acceptance run registers two users and places a call between them, through the node.
- [x] The acceptance run passes against the node **scheduled in Kubernetes**, not only in containers.
      Proved by `DP-9`: `scripts/k8s-two-node-call.sh` runs the full register-and-call against two
      pods in the k3d cluster, ending in a completed call with audio (`heard_audio: true`, 24000
      samples).
- [x] What the setup does *not* prove is stated where a reader will hit it, not buried.

## Progress
- **Filed and largely delivered 2026-07-29.** Deliberately **not** the operator path: `KO-1` has not
  pinned the custom resource and `KO-3` has not implemented the reconcile loop, so `deploy/helm/`
  renders a `SipxCluster` that nothing serves and `values.yaml` says so in its own header. This
  story runs the part that exists — the `sipx-clstr` binary — directly from a Deployment. When the
  operator lands, `deploy/devspace/` should be **deleted**, not extended.
- `Dockerfile` builds the binary inside the builder stage rather than copying a host build: a
  development machine's glibc is routinely newer than the runtime image's, so the copy produces an
  image that runs only on the machine that built it.
- `scripts/sip_demo.py` is the acceptance run — raw SIP over UDP, standard library only, so it runs
  from a stock `python:3.12-slim` pod with nothing installed.
- **Passing, in containers, against the image** (node and client each in their own container):
  ```
  [PASS] REGISTER alice -> 200 (contact echoed: <sip:alice@172.28.0.2:40836>;expires=3600)
  [PASS] REGISTER bob   -> 200 (contact echoed: <sip:bob@172.28.0.2:54448>;expires=3600)
  [PASS] INVITE reached bob from 172.28.0.11:5060 (the node)
         Via headers stacked: 2 (proxy added its own)
         Record-Route: <sip:172.28.0.11:5060;lr>
  [PASS] 200 OK returned to alice -> 200
  RESULT: PASS — registrar stored both bindings and the proxy forwarded between them
  ```
  The `Record-Route` carries the **advertised** address rather than the bound one, which is `DP-5`
  demonstrating itself outside its own tests: before `DP-5` the kernel's `sent_by` came from the
  bind address, so a container binding `0.0.0.0` advertised `0.0.0.0` and nothing could answer it.
- **The Kubernetes half is blocked on the host, not on the code.** The manifests apply and the
  Deployment is created, but the k3s node carries
  `node.kubernetes.io/disk-pressure:NoSchedule` — the host filesystem was at 99% — so the pod stays
  `Pending`. Nothing about the image or the manifests is implicated; the same image passes the same
  script in containers. Acceptance item 4 stays unticked until it is run for real.
- **The image cannot use the workspace's declared `rust-version`.** Pinned to 1.97 with the reason
  in the `Dockerfile`; see the *Notes* below.
- **A toleration for the disk-pressure taint was tried and reverted — do not retry it.** It looks
  correct: the pod is stateless, mounts no volume and has `readOnlyRootFilesystem`, so it neither
  causes disk pressure nor is threatened by it. But the taint and the **eviction manager** are two
  separate mechanisms keyed off the same condition. Tolerating the taint let the pod schedule and
  kubelet then evicted it at once; the ReplicaSet replaced it, and the loop produced **1158
  `Evicted` pod objects within minutes** before it was stopped. Eviction under disk pressure is
  node-scoped, not usage-scoped, so "this pod is not the problem" does not exempt it. The manifest
  now carries that as a comment where the toleration would go, because the next person to hit a
  `Pending` pod will reach for exactly this. There is no manifest-level answer; the answer is free
  space on the host.

## Notes
- **What this does not prove**, stated plainly because a green run invites the opposite reading: one
  process, one replica, UDP only, in-memory location store, no media, no operator, no CRD, no
  PostgreSQL, no clustering, no zone spread, no HA, no TCP and no TLS. It proves that the registrar
  stores a binding and that the forwarding core sends a request to it — nothing above that.
- The `Dockerfile`'s `CMD` is `--help`, not `run`. Since `DP-5` a node refuses to start when it
  would advertise an unspecified address, so `run --listen 0.0.0.0:5060` — the only default an image
  could pick — exits 2. A default that always fails teaches nothing.
- Considered for upstream: **no**. An image and a development loop for this platform are
  orchestration by definition; the kernel has no deployment of its own to run.
