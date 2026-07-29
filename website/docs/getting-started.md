---
title: "Getting started"
description: "Build the node, start it, and watch two users register and call each other through it — in about five minutes, with nothing but Rust and Python."
---

# Getting started

About five minutes to a working proxy and registrar: one node, two users registering, and a call
forwarded between them. You need a [Rust toolchain](https://rustup.rs) and Python 3. **No PBX, no
account, no configuration file** — there is no configuration file yet.

## Build it

sipx-clstr is not on crates.io. Clone the repository and build the binary:

```bash
git clone https://github.com/codewandler/sipx-clstr
cd sipx-clstr
cargo build --bin sipx-clstr
```

Check it built:

```bash
./target/debug/sipx-clstr --version
```

```text
sipx-clstr 0.10.0 (sipx kernel 0.7.0)
```

The second version is the [sipx](https://github.com/codewandler/sipx) protocol kernel this node
is built against. It is pinned to a tag, not a branch, so the same source always produces the
same protocol behaviour.

## Start a node

```bash
./target/debug/sipx-clstr run --listen 127.0.0.1:5060 --advertise 127.0.0.1:5060
```

```text
listening on 127.0.0.1:5060
advertising 127.0.0.1:5060
```

Those two lines go to **stdout**, and they are printed *after* the socket is bound — so a script
can wait for `listening on` rather than sleeping and hoping. Logs go to stderr; set `RUST_LOG`
to change the level.

Two addresses appear because they are allowed to differ. `--listen` is the socket; `--advertise`
is what the node writes into `Via` and `Record-Route`, which is what peers use to reach it back.
On loopback they are the same. Anywhere else they usually are not, and getting this wrong is the
single most common way to make a node that registers phones but never rings them — see
[Addressing](guides/addressing.md).

:::caution This node is open
There is no authentication. Anyone who can reach the port can register any address-of-record.
Bindings are also held in memory, so restarting the node forgets every registration. Keep this on
loopback or a trusted network.
:::

## Make two users register and call each other

Leave the node running. In a second terminal, from the same checkout:

```bash
python3 scripts/sip_demo.py 127.0.0.1:5060
```

This speaks raw SIP over UDP using nothing but the Python standard library — no dependencies to
install, and no SIP client to build.

```text
RESULT: PASS — registrar stored both bindings and the proxy forwarded between them
```

It narrates each step as it goes. What it proves, in order:

1. the registrar accepts a `REGISTER` and answers `200`, echoing back the binding it stored;
2. a second user registers independently;
3. an `INVITE` addressed to the second user's address-of-record is **forwarded by the proxy** to
   the contact that user registered — which is the whole point, because it means the location
   lookup and the forwarding core are wired to each other;
4. the callee's `200` reaches the caller, and the caller's `ACK` reaches the callee.

It deliberately does not prove media: the SDP is carried opaquely and no RTP flows. For a call
with real audio, see below.

## A real call, with audio

The demo above proves signalling. To place an actual call between two softphones with audio,
use the end-to-end script, which drives two independent
[sipx](https://github.com/codewandler/sipx) CLI phones through the node:

```bash
scripts/e2e-call.sh --sipx /path/to/sipx
```

The `sipx` CLI is a separate project and is not vendored here — that is deliberate, since the
point is that the client side is an independent implementation. Build it from a sipx checkout
with `cargo build --bin sipx`, then pass `--sipx`, set `$SIPX`, or put it on your `PATH`.

Exit codes: `0` the call completed with audio, `1` a step failed, `2` the environment was not
ready.

The interesting result is what it asserts at the end: the node holds **exactly one UDP socket**
for the whole call. There is no media relay, so the audio went directly between the two phones.
See [Registrations and calls](guides/registrations-and-calls.md) for what else that script
proves and why it takes about half a minute to finish.

## Next

- **[Does this fit?](guides/does-this-fit.md)** — what this is not, before you build on it.
- **[Run a node](guides/run-a-node.md)** — flags, output, exit codes, logging.
- **[Addressing](guides/addressing.md)** — bind versus advertise, NAT, and containers.
- **[Docker and k3d](guides/docker-and-k3d.md)** — the same thing in a container.
