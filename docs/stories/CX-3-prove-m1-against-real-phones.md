---
id: CX-3
title: Prove M1 against real phones
pillar: Platform
status: done
priority:
design: 
epic: 
areas: [build, proxy, registrar]
note: M1 #14 · two sipx CLI phones, one node, a real call — media direct, proved by audio
---

# Prove M1 against real phones

## Goal
Close M1 with the demonstration its definition names: two `sipx` CLI phones register through one
sipx-clstr node, call each other through it, and hang up — over real sockets, with media flowing
directly between the phones.

## Acceptance
- [x] A scripted run brings up one node and two `sipx` CLI phones, registers both, places a call
      from one to the other, answers it, and ends it with BYE. Every step asserts on the CLI's
      `--json` output and its exit code, not on log scraping.
- [x] **Media flows directly between the phones**, and the run proves it rather than assuming it.
- [x] The node's own record of the call agrees with what happened — the call is visible, and the
      transaction store is back to empty afterwards. A proxy that leaks one transaction per call
      is a slow outage, and this is the cheapest place to notice.
- [x] The run is one command, and it fails loudly. Anything that needs a human to interpret it is
      not an exit criterion.
- [x] Failing-first test: `scripts/e2e-call.sh` — see the note below on why it is a script rather
      than a `#[test]`.

## Progress
**It works.** Two real `sipx` CLI phones — an independent implementation of the client side, from the
kernel's own repository — register through one `sipx-clstr` node over UDP, place a call through it,
and hang up:

```
  bob:   {"status":"registered","aor":"sip:bob@127.0.0.1","expires":3600,"refresh_in":3240}
  alice: {"status":"registered","aor":"sip:alice@127.0.0.1","expires":3600,"refresh_in":3240}
  alice: {"status":"answered","peer":"sip:bob@127.0.0.1","mos":4.40, …}
  bob:   {"status":"answered","samples_recorded":24000,"heard_audio":true, …}
```

**Media is proved, not assumed.** Alice plays a three-second 440 Hz tone; bob records what arrives.
Bob receives **24 000 samples** — exactly three seconds at 8 kHz — into a 48 044-byte WAV, the same
size as the file alice played. The node runs no relay of any kind, and the script additionally
asserts it holds exactly **one** UDP socket, so RTP that reached bob came from alice's socket and
nowhere else. That is a stronger statement than reading the SDP: had the SDP named the node, the RTP
would have gone to a node with no media port and bob would have heard silence.

**The driver.** `sipx-clstr-node::driver` is `PX-2`'s design with sockets under it: built on
`sipx_transport::Handle`, one task per arrival, a `ResponseContext` per proxied request, branches as
`Handle::send` calls. It is the only place in the workspace outside the harness that reads a clock,
which is the sans-IO rule showing up as a property of the file listing rather than of a review.
`sipx-clstr run --listen <addr>` starts it; the argument surface is deliberately tiny and provisional,
and `DP-1` replaces it.

### Four defects the real network found

**The node announced its address before it was bound.** `main` printed "listening on …" and *then*
called `run`, so a failed bind produced a node that claimed to be listening and immediately died —
and a script waiting for that line proceeded happily. Found by testing the **failure** path rather
than the success path. The announcement now comes from the driver, after `bind` returns.

**The exit code was being swallowed.** The first failure check reported `exit=0` because the command
was piped into `tail`, whose status is the pipeline's. Checked without the pipe, all three are right:
`0` the call completed, `1` a step failed, `2` the environment is not ready.

**The transaction gauge could not observe what it was measuring.** Logging the count *per completed
request* means the last reading is taken while requests are still in flight — after the final one
there is nothing left to log, so the store draining is invisible by construction. It now samples on a
timer and logs **on change**, which is quieter than a periodic line and is `DP-3`'s gauge in embryo.

**Colour codes defeated the check.** `tracing`'s ANSI output put escape sequences between a field
name and its value, so `grep -o 'outstanding=[0-9]*'` matched nothing and the script reported "no
reading" as though it were a leak. The node's log is read by scripts as often as by people; it no
longer colours itself.

### Two things that look like bugs and are not

**The store takes about half a minute to drain, and that is correct.** RFC 3261 keeps a concluded
transaction alive for its absorption timer — 64·T1, thirty-two seconds — so a retransmission arriving
after the final response is answered from the transaction rather than delivered to the application a
second time. Asserting "empty immediately" would have been asserting a bug. What the check catches is
a count that never returns to zero at all.

**A contact naming a host, rather than an address, is not routable yet.** M1 forwards to registered
contacts, which are addresses; a name needs RFC 3263 resolution, which is `RT-1`'s. `destination_of`
returns `None` and says so, so it is a visible gap rather than a call that fails further along.

### The constraint that shaped the fixture

The CLI refuses a port inside an address-of-record, and §3 N7 makes `sip:bob@host` and
`sip:bob@host:5060` **deliberately distinct** keys — an absent port and an explicit one resolve
differently under RFC 3263, so they are different resources. Rather than bend the lookup to make a
test pass, the node runs on **5060**, where `sip:bob@127.0.0.1` is both what bob registers and what
alice's Request-URI canonicalizes to. The spec was right; the fixture was wrong.

## Notes
- `scripts/e2e-call.sh --sipx <path>`, or `SIPX=<path>`, or `sipx` on `PATH`. The CLI is deliberately
  **not vendored**: the value of this test is that the client side is somebody else's implementation
  of RFC 3261, and vendoring it would quietly make it ours.
- **A script rather than a `#[test]`**: it starts a process, binds a well-known port, drives two
  external binaries and waits out a 32-second protocol timer. A `cargo test` that does those things
  fails for reasons that have nothing to do with the code, and would be the first test people learn
  to ignore. The assertions and the exit code are the same; what changes is that a developer can run
  it without a harness in the way. It is not in CI for the same reasons — the harness scenarios are
  what CI has instead.
- `CF-3` generalizes this into the full interop harness (SIPp, third-party endpoints, rtpengine).
  This story is only the milestone's own proof, and stays small enough to keep running.
