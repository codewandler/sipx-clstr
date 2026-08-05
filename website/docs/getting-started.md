---
title: "Getting started"
description: "Run a node, place a call, then stand up a two-node cluster on Kubernetes and dial in to hear it answer — each step with the real output."
---

# Getting started

Three steps, each one you can run. A single node and a forwarded call in about five minutes; then a
two-node cluster on a local Kubernetes cluster; then dialling in and **hearing** it answer.

You need a [Rust toolchain](https://rustup.rs) and Python 3. For the cluster you also need Docker,
[k3d](https://k3d.io) and `kubectl`.

## 1 · Build it

sipx-clstr is not on crates.io. Clone the repository and build the binary:

```bash
git clone https://github.com/codewandler/sipx-clstr
cd sipx-clstr
cargo build --bin sipx-clstr --features postgres
./target/debug/sipx-clstr --version
```

```text
sipx-clstr 0.13.0 (sipx kernel 1.0.0-beta.4)
```

The `postgres` feature is what lets a node share its registrations with another one. Leave it out and
you get a node that can only ever be alone — it will refuse to start on a configuration asking for a
shared store rather than quietly using its own memory.

The second version is the [sipx](https://github.com/codewandler/sipx) protocol kernel this node is
built against, pinned to a tag so the same source always produces the same protocol behaviour.

## 2 · Run a node

A node is configured by a **document**, not by flags. Write one:

```yaml title="cluster.yaml"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: local
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 127.0.0.1:5060
      advertise: 127.0.0.1:5060
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [127.0.0.1]
```

YAML, JSON and TOML are all accepted — the same document in any of the three produces exactly the
same configuration, and the encoding is detected from the content rather than the file name.

Then run it. A node is told **what it is** from outside the document, because the document is the
same on every node in the cluster:

```bash
./target/debug/sipx-clstr run --config cluster.yaml \
  --node 1 --zone a --roles edge,registrar
```

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

Those two lines go to **stdout**, and they are printed only once everything that could refuse to
start has declined to — so a script can wait for `listening on` instead of sleeping. Logs go to
stderr; set `RUST_LOG` to change the level.

Two addresses appear because they are allowed to differ. `bind` is the socket; `advertise` is what the
node writes into `Via` and `Record-Route`, which is what peers use to reach it back. On loopback they
are the same. Anywhere else they usually are not, and getting it wrong is the most common way to make
a node that registers phones but never rings them — see [Addressing](guides/addressing.md).

`domains` is enforced: a `REGISTER` whose address-of-record is in a domain this tenant does not serve
is answered **`403`**. The demo below registers `alice@127.0.0.1`, which is why the tenant declares
that domain. Declare the domain your phones actually use, or leave `domains` out to serve any.

:::caution This node is open
This tenant declares no `auth`, so anyone who can reach the port can register any address-of-record in
a served domain. Declaring `tenant[].auth` does not close that: digest is implemented and applied, but
there are no user credentials yet, so a node whose nonce `secretRef` resolves **refuses to start** and
one whose reference does not resolve challenges every `REGISTER` into a `401` nobody can answer. There
is no configuration today that gives you an authenticated registrar —
[Configuration](reference/configuration.md) has the exact states. With `backend: memory`, restarting
the node also forgets every registration. Keep this on loopback or a trusted network.
:::

### Place a call through it

Leave the node running. In a second terminal, from the same checkout:

```bash
python3 scripts/sip_demo.py 127.0.0.1:5060
```

```text
[PASS] REGISTER alice -> 200 (contact echoed: <sip:alice@127.0.0.1:44870>;expires=3600)
[PASS] REGISTER bob   -> 200 (contact echoed: <sip:bob@127.0.0.1:43859>;expires=3600)
[PASS] INVITE reached bob from 127.0.0.1:5060 (the node)
       Via headers stacked: 2 (proxy added its own)
       Record-Route: <sip:127.0.0.1:5060;lr>
[PASS] 200 OK returned to alice -> 200

RESULT: PASS — registrar stored both bindings and the proxy forwarded between them
```

Raw SIP over UDP with nothing but the Python standard library — no dependencies, no softphone to
build. It proves the registrar stored both bindings and the proxy forwarded between them. It moves no
audio; that comes next.

## 3 · A two-node cluster on Kubernetes

One node is a proxy. Two nodes sharing a location service are a cluster — a user who registers
through one can be called through the other. The devspace profile stands that up:

```bash
k3d cluster create sipx
docker build -t sipx-clstr:dev .
k3d image import sipx-clstr:dev -c sipx
```

### The phone image

The profile also runs a **greeting** — a phone that answers with a tone — and that greeting is a
[sipx](https://github.com/codewandler/sipx) CLI phone rather than a platform feature. It runs as a
pod, so the CLI has to be inside an image the cluster can pull, and **nothing publishes one**. You
build it, from the same kernel tag this workspace pins — the second version `--version` printed in
step 1 — so the phone and the node speak the same protocol build:

```bash
mkdir -p ~/sipx-phone && cd ~/sipx-phone
git clone --depth 1 --branch v1.0.0-beta.4 https://github.com/codewandler/sipx kernel
cargo build --release --bin sipx --manifest-path kernel/Cargo.toml
cp kernel/target/release/sipx .
```

It needs something to play. A three-second 440 Hz tone, from the standard library:

```bash
python3 - tone.wav <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "w") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
    w.writeframes(b"".join(struct.pack("<h", int(12000 * math.sin(2 * math.pi * 440 * i / 8000)))
                           for i in range(8000 * 3)))
PY
```

Then the image itself, and into the cluster. `libopus0` is the one shared library the CLI's media
side needs, and the `--help` in the build is a smoke test: a phone image that cannot start is much
cheaper to find here than as a `CrashLoopBackOff` later.

```bash
cat > phone.Dockerfile <<'EOF'
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libopus0 \
    && rm -rf /var/lib/apt/lists/*
COPY sipx /usr/local/bin/sipx
RUN chmod +x /usr/local/bin/sipx && /usr/local/bin/sipx --help >/dev/null
COPY tone.wav /tone.wav
ENTRYPOINT ["/usr/local/bin/sipx"]
EOF
docker build -f phone.Dockerfile -t sipx-phone:dev .
k3d image import sipx-phone:dev -c sipx
```

Skipping this does not fail loudly. The greeting `Deployment` names `sipx-phone:dev` with
`imagePullPolicy: IfNotPresent`, so a cluster without it asks the public registry, is told the
repository does not exist, and settles into `ImagePullBackOff` — while both nodes come up `1/1` and
look entirely healthy.

### Deploy the profile

Back in the sipx-clstr checkout:

```bash
kubectl create namespace sipx-clstr-dev
kubectl -n sipx-clstr-dev apply -f deploy/devspace/manifests/node.yaml
```

With [devspace](https://devspace.sh) installed, `cd deploy/devspace && devspace deploy` does the same
thing, and `devspace run demo` runs the whole proof described below.

That gives you **two nodes and one PostgreSQL location service**, all reading a single `ConfigMap`
document:

```bash
kubectl -n sipx-clstr-dev get deploy
```

```text
NAME                  READY   UP-TO-DATE   AVAILABLE
sipx-clstr-greeting   1/1     1            1
sipx-clstr-node-a     1/1     1            1
sipx-clstr-node-b     1/1     1            1
sipx-clstr-postgres   1/1     1            1
```

The nodes may restart once or twice on the way there. They are started in parallel with the location
service and refuse to run without it — `the configured location store could not be reached` — so the
first attempts exit until PostgreSQL is accepting connections. That is the intended behaviour, not a
flap: a node that quietly fell back to its own memory would be the bug.

Check which store each node opened. It must say `postgres` on both — a node that fell back to its own
memory would look healthy and answer every `REGISTER`, while serving bindings no peer can see:

```bash
kubectl -n sipx-clstr-dev logs deploy/sipx-clstr-node-a | grep 'node listening'
```

```text
node listening listen=0.0.0.0:5060 advertised=10.42.0.10:5060 tenant=default store="postgres" auth="open"
```

Each pod advertises **its own pod IP**, resolved per node from the one document through `${POD_IP}`.
That is why there is no per-node file. `auth="open"` is the same disclosure as the caution box above,
in the node's own words: this tenant declares no `auth`, so the registrar accepts anyone.

### Two Deployments, not two replicas

A node's identity — its id, zone and role set — comes from outside the document, and a Deployment
cannot give its replicas distinct ids. Assigning them is the operator's job, and the operator is
[designed but not built](operate/deploy.md). So the profile runs two Deployments of one replica each,
with their ids set explicitly. No pretence.

## 4 · Call in, and hear something

The profile also runs a **greeting**: a `sipx` CLI phone that registers as `hello` and answers with a
tone. It registers *through node-b* and is reached *through node-a*, so hearing it is itself the proof
that the two nodes share one registrar.

The caller is the same [sipx](https://github.com/codewandler/sipx) CLI, out of the `sipx-phone:dev`
image you built in step 3 — a separate project, deliberately not vendored here, because the point of
an end-to-end proof is that the other end is an independent implementation.

Dial it from inside the cluster. The greeting's address-of-record is `hello@10.43.0.60` —
node-a's Service address, which the profile pins with a static `clusterIP` and also declares as the
tenant's domain, because `domains` is enforced and a `REGISTER` outside it is answered `403`. One
string has to wear three hats here: `sipx dial` sends to the literal address it is given and
resolves no names, the tenant's list can only hold what is known before any pod exists, and a
static `clusterIP` is the one address that is both — declared in the manifest, dialable on the
wire. It sits in k3s's default Service CIDR (`10.43.0.0/16`), so the cluster from step 3 serves it;
a cluster created with a different CIDR refuses the Service at apply time rather than failing when
you dial:

```bash
kubectl -n sipx-clstr-dev run caller --rm -i --restart=Never \
  --image=sipx-phone:dev --image-pull-policy=IfNotPresent --command -- \
  /bin/sh -c "ME=\$(hostname -i | awk '{print \$1}'); \
    sipx dial sip:hello@10.43.0.60 --local \$ME:15080 \
      --duration 6 --record /tmp/heard.wav --stats --json"
```

```text
{"status":"answered","peer":"sip:hello@10.43.0.60","duration_ms":3566,
 "samples_recorded":24000,"heard_audio":true,"loss":0.0000,"jitter_ms":0,"mos":4.40}
```

**`heard_audio: true`** is the line that matters: 24000 samples of 8 kHz audio arrived, so the call
was answered *and* media flowed. It flowed **directly between the two phones** — the platform has no
media relay, and never puts RTP in the process that parses SIP.

:::note Two details that will bite you
The phone runs as a pod, not on your machine. Pod addresses are often unroutable from the host — a VPN
or an overlay route can swallow them silently — and pod-to-pod has no such ambiguity. It is also the
position a real client is in.

And the phone binds an explicit address rather than `0.0.0.0`. A caller that binds the wildcard puts it
in its own `Contact`, and then nothing can route the answer back to it. Same rule as the node's
`advertise`, seen from the client side.
:::

The greeting is a `sipx` CLI phone, not a platform feature. This platform is **proxy-first**: it
forwards and never terminates a dialog, so nothing in the node itself answers a call. The `echo` role
that would is [specified and not implemented](clustering/how-it-works.md).

### Prove it end to end

Both proofs are scripted, and each prints what it does *not* prove:

```bash
scripts/two-node-call.sh --sipx /path/to/sipx      # two local processes
scripts/k8s-two-node-call.sh                       # two pods in the cluster
```

```text
[PASS] node-a at 10.42.0.207, node-b at 10.42.0.208 — two pods, two addresses
[PASS] both nodes report store="postgres" — one location service, not two
[PASS] alice registered through node-a
[PASS] bob registered through node-b
[PASS] 2 bindings in one database, written by two different pods
[PASS] the call completed
[PASS] the callee heard the tone the caller played
```

What neither proves: a **single Service in front of both nodes**. Each node record-routes its own
address, so the route set names a node. Put one address in front of the two and in-dialog requests
will land on whichever the load balancer picks — the case
[affinity tokens](clustering/affinity-and-flows.md) exist for, and they are not implemented.

## Next

- **[Does this fit?](guides/does-this-fit.md)** — what this is not, before you build on it.
- **[Configuration](reference/configuration.md)** — the document schema, and what is not in it yet.
- **[CLI reference](reference/cli.md)** — every flag, exit code and output line.
- **[Addressing](guides/addressing.md)** — bind versus advertise, the thing most likely to bite.
