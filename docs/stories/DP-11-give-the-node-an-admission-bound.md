---
id: DP-11
title: Give the node an admission bound, not only an inherited queue bound
pillar: Cluster
status: ready
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, driver, observability]
note: one tokio::spawn per new transaction with no semaphore — the only backpressure is the kernel's queue
---

# Give the node an admission bound, not only an inherited queue bound

## Goal

Bound how much work a node will take on at once. The driver spawns a task per new server transaction
with no cap, so in-flight concurrency is limited only by memory — and the overload signal the scaling
design calls decisive is computed by the kernel and never read.

## Acceptance

- [ ] Concurrent in-flight transactions are bounded — a semaphore, a `JoinSet` with a cap, or an
      explicit admission decision — and the bound is configurable through
      [cluster-config](../specs/cluster-config.md) rather than a constant. `driver.rs`'s
      `while let Some(arrival) = incoming.recv().await { … tokio::spawn(…) }` has no limit today.
- [ ] Refusing over the bound is a SIP answer, not a drop. The kernel already answers `503` with
      `Retry-After` when *its* queue is full; a node-level refusal should be the same shape so a
      client sees one behaviour regardless of which layer shed it.
- [ ] The kernel's shed counters are read and exported. `Handle::shed()` separates shed requests,
      ACKs and unmatched messages, and nothing in this repo calls it — `outstanding()` is the only
      kernel instrument used, and it goes to a log line rather than a metric.
      `website/docs/operate/scaling.md` names overload shed rate as the one number that says the
      platform is past its limit, and that number exists in-process today and is discarded.
- [ ] **Failing-first**: a harness scenario floods the node and asserts a ceiling on concurrent
      in-flight transactions. It fails today — the count tracks offered load until something else
      runs out.
- [ ] Log amplification under overload is considered. The kernel emits a `warn` or `error` line per
      shed message to stderr synchronously, which is a per-message cost on exactly the input that
      triggers it. Rate-limit, sample, or count instead — decided and recorded.
- [ ] Losing the transport driver stops being reported as success. When `incoming.recv()` returns
      `None` the driver returns `Ok(())` and the process exits `0`, so a node whose socket layer died
      looks like an intentional shutdown. Contrast the care taken over startup refusals.

## Progress

- (running log)

## Notes

- **What is inherited and what is missing.** The kernel's bounded 1024-message queue plus `503`-on-full
  is real backpressure and it is good; it bounds the *queue*, not concurrency. The node drains that
  queue as fast as it can and spawns per new transaction, and a task lives for the whole proxied
  transaction — up to Timer B, or the 180-second unanswered backstop. So offered load converts
  directly into resident tasks.
- **Not one task per datagram.** Retransmissions are absorbed by the kernel's transaction matching, so
  the multiplier is new transactions rather than packets. That makes the bound cheaper than it sounds
  and does not make it unnecessary.
- **Why this pairs with `RG-14`.** That story bounds what one REGISTER costs; this one bounds how many
  are in flight. Either alone leaves the product unbounded, and the two together are what make a
  registration storm survivable rather than merely slower.
- This is the story the scaling design's guardrails already assume exists — "overload control is
  shedding" is listed as a correctness signal that disables autoscaling, and there is nothing on the
  node that sheds or reports shedding. Doing this before `KO-5` means the autoscaler has a real input
  rather than a planned one.
- Considered for upstream: no, for the concurrency bound — the kernel deliberately leaves the TU to
  decide how much it will take on. Reading `shed()` is ours too. If the per-message log line under
  overload turns out to need fixing inside the kernel, that half goes on
  [upstream.md](../upstream.md).
