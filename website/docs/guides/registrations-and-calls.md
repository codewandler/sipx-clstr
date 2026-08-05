---
title: "Registrations and calls"
description: "What actually happens when a phone registers and calls another through the node — the location lookup, the forward, and why media never touches the platform."
---

# Registrations and calls

This is the one thing sipx-clstr does end to end today. Two phones register, one calls the
other, the node forwards the call, and the audio never goes anywhere near the node.

## Prove it

```bash
scripts/e2e-call.sh --sipx /path/to/sipx
```

Exit `0` means a call completed with audio. `1` means a step failed; `2` means the environment
was not ready — almost always a missing `sipx` CLI.

The client side is the [sipx](https://github.com/codewandler/sipx) CLI phone, built from its own
checkout with `cargo build --bin sipx`. It is deliberately not vendored here: the point of an
end-to-end proof is that the thing on the other end is a **separate process on a real socket**, not
this one talking to itself in memory.

Be precise about what that buys, because the two are easy to conflate. The phone is built from the
same kernel this repository pins, so this is a **same-kernel, separate-process integration test**: it
proves the listener binds, that a registration made by one process is found by another, and that
media flows. It does not prove interoperation with a stack written by somebody else — a parser
disagreement shared by both ends would pass this test unnoticed. That needs an independent target,
and it does not exist yet.

The script builds the node, starts it, waits for the `listening on` line, generates a three-second
440 Hz tone, registers `alice` and `bob` on separate ports, has bob answer and record, has alice
dial and play, and then checks what actually happened.

For a version with no dependencies at all — no `sipx` build, standard-library Python only — use
`scripts/sip_demo.py` as shown in [Getting started](../getting-started.md). It proves the
signalling path but carries SDP opaquely and moves no audio.

## What registration does

A `REGISTER` binds an address-of-record to a contact. The registrar canonicalises the AoR,
applies the binding to a compare-and-swap location store, and answers `200` echoing the binding
set it now holds.

Compare-and-swap matters more than it sounds. A binding update is not "write the new value" but
"replace exactly the version I read", so two registrations racing for the same AoR cannot
interleave into a set that neither client asked for. That contract is what makes the store
shardable later without changing the registrar.

Which store you get is named in the document. `backend: memory` is in the process — restart the node
and every binding is gone. `backend: postgres` is the shared location service: it survives a restart
and two nodes reading it are one registrar. It needs the non-default `postgres` cargo feature, and a
node asking for it without the feature refuses to start rather than quietly using its own memory.

Before the store is touched, the tenant refuses a `REGISTER` whose Request-URI or address-of-record
names a domain it does not serve with **`404`**. Once the current binding set is read, the per-AoR
quota is enforced and the granted expiry is clamped to the tenant's bounds.

What is *not* enforced is who you are. The digest code is implemented, proved against the RFCs' own
vectors, and applied from `tenant[].auth` — but there is no user-credential store, so a document
asking for authentication either stops the node or challenges every `REGISTER` into a `401` nobody can
answer. A node that runs accepts any AoR in a served domain from anyone.

## What a call does

An `INVITE` addressed to a registered AoR is **forwarded**, not answered. The node:

1. validates the request and decrements `Max-Forwards`;
2. looks the AoR up in the location service to get the registered contacts;
3. makes each contact a branch target, rewrites the Request-URI, adds its own `Via`, and records
   itself in `Record-Route` so in-dialog requests come back through it;
4. forwards, aggregates the responses, and passes the best one back.

The platform never becomes a party to the call. It is a proxy: it does not answer, does not hold
the dialog, and does not terminate anything. `BYE`, `re-INVITE` and the rest are routed by the
`Route` set the phones learned from `Record-Route`.

The normative rules are in the
[proxy behaviour spec](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md),
which carries the `PB-*` vector table each rule is tested against.

## Media goes around the node

Nothing in the SDP is rewritten. The two phones exchange addresses and send RTP directly to each
other. The node sees the offer and the answer as opaque bodies.

The end-to-end script asserts this rather than assuming it: at the end of the call it checks that
the node holds **exactly one UDP socket** — the signalling one. If media were being relayed there
would be more. Since no relay exists, more sockets would mean RTP had reached a process that is
never allowed to carry it.

When media *does* need to be relayed — for NAT traversal or a carrier that will not talk to your
clients directly — it will be relayed by a separate process this platform controls over a network
protocol. No RTP in the signalling process, ever. See
[Media](../clustering/media.md).

## Why it takes about half a minute

The script does not finish the moment the call ends. It waits for the node's transaction store to
drain to zero, and that takes roughly 32 seconds.

That is RFC 3261's absorption window: 64·T1, the interval a completed transaction is kept alive
to absorb retransmissions that are still in flight. Asserting the store empties *immediately*
would be asserting a bug — it would mean a late retransmission had nothing to match and would be
processed as a new request.
