---
id: PX-2
title: Design the proxy transaction driver
pillar: Signalling
status: done
priority:
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, transport]
note: M1 #2 · decided: build on sipx_transport::Handle, not on a socket loop of our own
---

# Design the proxy transaction driver

## Goal
Design the driver that fans one server transaction out to N client transactions directly over sipx-sip's `TransactionLayer`, reusing sipx-transport's pool and resolution machinery where crate boundaries allow.

## Acceptance
- [x] The design covers per-branch destinations and failure handling, CANCEL wiring via `TransactionKey::for_cancelled_invite`, ownership (no locks on the signalling path), and backpressure.
- [x] The crate boundary against sipx-transport is decided: what imports, what is extracted, what is reimplemented — with rationale in the design doc.
- [x] The driver contract is expressible sans-IO so PX-1 vectors drive it in the harness.

## Progress
Design: [proxy-transaction-driver](../designs/proxy-transaction-driver.md).

**The epic design's premise was wrong, and reading 0.2.1 rather than remembering it is what found
that.** It assumed sipx's transport API was "UA-shaped: one transaction, one target", and that a
forking proxy would therefore need its own socket loop over
`sipx_sip::transaction::TransactionLayer`. In fact `sipx_transport::Handle` already fans out:
`send(request, target)` creates one client transaction per call and returns its own event stream,
N calls run concurrently over one `TransactionLayer`, and — decisively — `send` inserts a `Via`
only when one is absent, so a proxy that pushes its own keeps full control of the branch. The
branch we choose *is* the transaction key.

**Decision: the driver is built on `sipx_transport::Handle`.** The alternative buys control over
two small gaps at the cost of reimplementing framing, three handshakes, NAT `Via` rewriting, the
pool and the retransmission driver — released, interop-tested kernel code. Rewriting that here is
what AGENTS.md rule 6 forbids in as many words.

**Shape:** one task per proxied request, owning its response context; branch streams collected in
a `FuturesUnordered` so responses, timers and an upstream CANCEL arrive at one `select!` and
become one `Input` at a time. No locks on the signalling path — nothing is shared — and the
resulting total order of inputs is what makes a run replayable in the harness. The effect table,
the `TuEvent`→`Input` mapping (including *stream ended with no final* → `BranchTransportError`,
so a vanished branch cannot hang a context forever) and the three backpressure points are in the
design.

**Two kernel gaps found and filed** rather than worked around here:

- **sipx `T-18`** — unmatched *responses* are dropped. §16.7 step 1 requires a stateful proxy with
  no response context to forward them statelessly; it cannot, if it never sees them.
- **sipx `T-19`** — `Incoming` is delivered with `try_send`, so requests are lost silently under
  backpressure with no counter. A dropped INVITE is a missed call the peer retransmits; **a
  dropped 2xx ACK is a call that never ends.** Filed as a kernel defect on its own terms, not as a
  downstream ask.

Neither blocks M1 — one node always finds its response context, and M1 makes no overload claim —
but both must close before M2 asserts that killing a node leaves calls working, because node loss
produces exactly those two failure modes. Recorded in the [ledger](../upstream.md).

## Notes
- Epic design: [proxy-engine](../designs/proxy-engine.md), whose "the sipx transport `Driver`/
  `Handle` API is UA-shaped" paragraph this story supersedes.
- `PX-5` implements the core against this seam; `PX-6` settles the one open question — which
  `Target` a CANCEL uses when its branch failed over to a second candidate.
