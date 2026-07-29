---
id: EX-6
title: Design an async external routing hook
pillar: Platform
status: done
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [hooks, routing]
note: blocks a downstream deployment's parity milestone; a blocking HTTP call on the INVITE path today
---

# Design an async external routing hook

## Goal
Give the hook framework a phase in which an external service can influence routing — selecting egress and rewriting identity — without blocking the transaction.

## Acceptance
- [x] A hook phase on the outbound path may consult an external service asynchronously.
- [x] Timeout, and the behaviour on client error, server error and timeout, are declared per hook, not coded.
- [x] The transaction's timers and capacity are not coupled to the external service's latency.
- [x] Failure semantics are specified: which outcomes fail the call, which proceed with a declared default.
- [x] Harness scenarios cover slow, failing and flapping external services.

## Progress
- 2026-07-29 — Designed in [extension-framework](../designs/extension-framework.md), section
  *EX-6: the async external routing hook*. No Rust changed; no phase added.
  - **Attachment**: the consult is EX-1's existing `Query` suspension at **H7
    `BeforeTargetResolution`** — the last point before `ResolveTargets`, and the only phase that
    permits `query` *and* `rewrite-target-query`. H8 (`SelectTargets`) and H9 (`PatchHeaders` on
    `P-Asserted-Identity`/`Privacy`) read the answer as a published request-scoped fact and never
    query. One consult per request; a second oracle is a G2 conflict via the
    `external-route-decision` capability.
  - **Sans-IO**: `Query` out → engine arms an **engine-owned** `QueryDeadline` from the declared
    `timeout` → driver I/O → `QueryAnswered` or `TimerFired` back in. The deadline is a fired
    timer, not a stopwatch; a generation counter makes the outcome decided exactly once, so a
    late answer is discarded and the trace stays seed-stable. Same-phase queries run serially in
    G3 order (parallel dispatch rejected: the decision would depend on answer order).
  - **Timer decoupling**: E5 promotes the request to stateful, so the INVITE server transaction
    emits `100 (Trying)` (RFC 3261 §17.2.1) and absorbs retransmissions while suspended; the
    query concludes at H7, before F10, so Timer C is armed at `t_forward + 180 s`, never
    `t_invite + 180 s`. Bounds: per-query `timeout` (default 500 ms = T1, RFC 3261 §17.1.1.1) and
    a profile `hook_budget` (default 2 s) checked at startup; retries capped at 1 inside the same
    deadline (RFC 5390 §3 amplification).
  - **Capacity decoupling**: declared `in_flight_max` (512) and `queue_max` (**0** — shed, don't
    queue), a per-node breaker that answers `Unavailable` with no driver call while open, and a
    declared cache whose negative caching follows RFC 2308 §5 ("no route" is cacheable, "could
    not ask" is not).
  - **Fail-open vs fail-closed — decided**: fail-closed by construction. A missing outcome arm is
    a *startup* error (G7), never a runtime default; fail-open exists only as
    `Proceed(DefaultRef)` naming a startup-validated fallback (G8). Shipped defaults reject on
    `ClientError`/`ServerError`/`Malformed`/`Timeout` (`500`), `Declined` (`480`, matching proxy
    spec §7's empty-target-set rule) and `Unavailable` (`503`). `Reject` statuses are a closed
    set `{403, 404, 480, 500, 503}`; **6xx is forbidden** (RFC 3261 §21.6, §16.7 — a routing
    failure cannot claim the user is unavailable everywhere, and it would cancel an upstream
    proxy's other branches). This makes the deployment's current "tolerate 5xx" explicit rather
    than implicit: it becomes `Proceed(fallback_pool)` in a manifest, with a vector.
  - **Closed world**: an answer selects among configured trunks and RT-7-admissible identities;
    an unknown pool resolves as `Malformed`. An oracle that could name arbitrary egress is a
    toll-fraud path with an HTTP front end.
  - **Harness**: eleven named scenarios over a declarative `ResourceModel` (latency distribution
    + outcome schedule, RNG stream `"resource:route-oracle"`), covering slow, timing-out,
    client-error, server-error-with-default, flapping/breaker, capacity-under-slow-oracle,
    malformed (no panic), CANCEL-during-suspension and trace determinism. Specified at design
    grain — nothing executes them until the hook runtime exists (EX-3).
  - **Upstream** (rule 6): *considered for upstream: no, cluster-specific* — the declaration
    surface is bound to this platform's manifest and pipeline; the kernel has no hook phases. The
    generic pieces it needs are already there and are consumed, not re-made. Both read in the
    pinned kernel (`v0.7.0`) rather than assumed: `ServerTransaction` arms `Timer::Trying100` for
    INVITE per RFC 3261 §17.2.1 (`transaction/server.rs:73`), sends it only if the TU has not
    answered (`server.rs:200`), and absorbs retransmissions so the TU hears nothing
    (`server.rs:100`); and `TimerQueue<K, I = Instant>` landed in `v0.7.0` under the ledger's
    *timer queue drivable from a virtual clock* row — **not** `X-14`, which is the row that
    generalized the queue over its key and explicitly failed to close this gap. Nothing new is
    filed against the ledger.
- 2026-07-29 — **Handoff: `docs/specs/hook-framework.md` was deliberately NOT edited** (EX-1 is
  closed). The design's *What this hands to the spec* subsection names the exact deltas — §4
  outcome/disposition enums and the `QueryDeadline` timer class, §6 `QueryDecl` fields, §7 G7/G8,
  §8 `hook_budget` as a profile value, §9 vectors `HF-9` … `HF-13`. A follow-up story is needed
  to make them normative; it was not filed here because the board is generated and fenced.

## Notes
- One deployment performs a synchronous HTTP lookup on every outbound INVITE to select the carrier pool and caller-ID; a 4xx fails the call and a 5xx is tolerated.
- For some pools this lookup is the *only* selection mechanism, so it cannot simply be removed.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-2**). The evidence sits in that deployment's own reference material.
