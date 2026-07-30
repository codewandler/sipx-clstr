---
id: DP-11
title: Give the node an admission bound, not only an inherited queue bound
pillar: Cluster
status: in-progress
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

- [x] Concurrent in-flight transactions are bounded — a semaphore, a `JoinSet` with a cap, or an
      explicit admission decision — and the bound is configurable through
      [cluster-config](../specs/cluster-config.md) rather than a constant. `driver.rs`'s
      `while let Some(arrival) = incoming.recv().await { … tokio::spawn(…) }` has no limit today.
      → an explicit admission decision, `AdmissionBound::admit` in `crates/sipx-clstr-node/src/driver.rs`,
      taken on the accept loop before a task exists; the knob is `cluster.admission.maxInFlightTransactions`
      (`config/mod.rs`'s `read_admission`, plumbed in `startup.rs`).
- [x] Refusing over the bound is a SIP answer, not a drop. The kernel already answers `503` with
      `Retry-After` when *its* queue is full; a node-level refusal should be the same shape so a
      client sees one behaviour regardless of which layer shed it.
      → `driver.rs`'s `overloaded()`, `503 Service Unavailable` + `Retry-After: 5`, the same value the
      kernel's `Endpoint::refuse` sends; asserted on the wire in `tests/admission_bound.rs`.
- [x] The kernel's shed counters are read and exported. `Handle::shed()` separates shed requests,
      ACKs and unmatched messages, and nothing in this repo calls it — `outstanding()` is the only
      kernel instrument used, and it goes to a log line rather than a metric.
      `website/docs/operate/scaling.md` names overload shed rate as the one number that says the
      platform is past its limit, and that number exists in-process today and is discarded.
      → `report_load` in `driver.rs` samples `shed()` and emits `shed_requests` / `shed_acks` /
      `shed_unmatched` beside `outstanding`, `in_flight`, `admitted` and `refused`;
      `dp11_the_shed_counters_reach_a_human` drives the binary and reads its stderr. Still a log line
      rather than a metric — a metrics endpoint is `DP-3`'s, and this is its input.
- [x] **Failing-first**: a harness scenario floods the node and asserts a ceiling on concurrent
      in-flight transactions. It fails today — the count tracks offered load until something else
      runs out.
      → `dp11_a_flood_cannot_exceed_the_admission_bound`. At the merge base:
      `40 of 40 offered transactions were admitted at once; the bound is 8`.
- [x] Log amplification under overload is considered. The kernel emits a `warn` or `error` line per
      shed message to stderr synchronously, which is a per-message cost on exactly the input that
      triggers it. Rate-limit, sample, or count instead — decided and recorded.
      → **counted, never logged per message**: the node's own refusals increment
      `AdmissionBound::refused` and `report_load` reports the count on a 500 ms sample. The kernel's own
      per-message line is not ours to fix; see the upstream bullet below.
- [x] Losing the transport driver stops being reported as success. When `incoming.recv()` returns
      `None` the driver returns `Ok(())` and the process exits `0`, so a node whose socket layer died
      looks like an intentional shutdown. Contrast the care taken over startup refusals.
      → `NodeError::TransportGone`; the accept loop's only exit. `main.rs` already maps any `Err` to
      `ExitCode::FAILURE`, so the process now exits non-zero.

## Progress

- **The shape chosen, and why it cannot starve registration.** The bound is taken at the point of
  admitting a **proxied** transaction, on the accept loop, before anything is cloned and before a task
  exists. `REGISTER` is exempt: it never takes a permit and never waits for one, because a registration
  storm *is* the overload and a refused refresh is a phone that becomes unreachable — a node that shed
  REGISTERs would turn a spike into a permanent outage. `ACK` is exempt because SIP has no response to
  an ACK (RFC 3261 §17.1.1.3), so "refusing" one can only mean dropping it, which is the call-leaking
  failure the kernel counts apart as `ShedCounts::acks`. Everything else on the proxy path is gated,
  BYE and CANCEL included: exempting the messages that *end* work was considered and rejected, because
  an unbounded method is an unbounded node.
- **Where the knob lives.** `cluster.admission.maxInFlightTransactions`, a new cluster section, default
  1024 (the kernel's own queue capacity), refused at `0`, ceiling 65536. Not `security`, which is the
  `edge`'s section and whose members all answer "who may talk to us"; not `rateLimit[]`, which is
  `RT-3`'s and whose subject is arrival rate per source rather than resident concurrency; and
  emphatically not `security.maxForwards`, which §8 V6 refuses to make a knob at all.
  **`docs/specs/cluster-config.md` §7 still needs a row for this section** — that file was outside this
  story's write set, so the loader currently recognises a key the spec does not yet declare. It is the
  one loose end here.
- **Considered for upstream.** The concurrency bound is ours: the kernel deliberately leaves the TU to
  decide how much work it takes on, and reading `shed()` is ours too. **One half does belong upstream**
  and is not fixed here: `sipx-transport`'s `Endpoint` emits a `tracing::warn!` per shed request and a
  `tracing::error!` per shed ACK, synchronously on its event loop
  (`crates/sipx-transport/src/endpoint.rs` around the `incoming.try_send` failure arm). That is a
  per-message cost on precisely the input that triggers it, and under a flood the logging is on the
  same loop that must keep timers running — so the kernel's own overload handling gets slower the more
  overload there is. The remedy is the kernel's: sample or rate-limit the line and leave the counters,
  which are already lock-free and already correct. `docs/upstream.md` wants a row for it; that file was
  outside this story's write set, so the finding is recorded here for whoever holds the ledger.
- Not done, and deliberately: no metric endpoint (`DP-3`), and no way to *induce* a dead transport from
  outside `driver::run` — the handle that could shut the endpoint down never leaves it, so
  `TransportGone` is pinned by its message and by being the loop's only exit rather than by a scenario.

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

## Integration

- Reviewed as evidence rather than on the report. **The failing-first test was re-run at the merge
  base independently**: 40 of 40 offered transactions admitted against a bound of 8, panicking exactly
  as claimed. No fenced file was touched.
- **The spec gap the implementor escalated was closed before merging, not after.** It recognised
  `cluster.admission` while `cluster-config.md` §7 declared no such section — spec-before-code, and it
  correctly refused to edit a fenced spec. The registry row, the §8 V8 ceiling (`1..=65536`) and a new
  **V11** rule landed first. V11 is where the exemption reasoning now lives, because an operator
  reading the schema is who needs it: the bound is on *concurrency*, not the kernel's queue;
  `REGISTER` and `ACK` are outside it; every other gated method is subject to it.
- **The REGISTER exemption was checked, not taken on trust.** A registration storm *is* the overload,
  and a shed refresh makes a phone unreachable — turning a spike into an outage plus a retry spike. The
  code takes no permit anywhere on the REGISTER path. `ACK` is exempt for the harder reason: RFC 3261
  §17.1.1.3 gives an ACK for a 2xx no response at all, so there is no `503` to answer it with.
- **`BYE`/`CANCEL` stay gated**, and the trade-off is argued rather than assumed: shedding the requests
  that *end* work makes overload self-sustaining, but an unbounded method is an unbounded node, and a
  `503` with `Retry-After` to a `BYE` is a retry rather than a loss.
- Merge conflict in `config/tests.rs` only, against `FC-4` which landed after this branched. Purely
  additive on both sides — both test sets kept, verified by running them.
- Carried forward rather than dropped: the kernel logs per shed message synchronously on the loop that
  must keep timers running, which makes overload handling slower the more overload there is. That is
  the kernel's to fix and belongs on `docs/upstream.md`; recorded here so it is not lost.
