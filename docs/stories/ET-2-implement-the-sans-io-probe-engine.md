---
id: ET-2
title: Implement the sans-IO probe engine
pillar: Platform
status: done
priority:
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe, harness]
note: M1 #12 · the probe engine and scheduler; every EP-* vector proved
---

# Implement the sans-IO probe engine

## Goal
Implement the scheduler and probe state machine as pure logic over the sipx UA layers, so probe runs — including their failure modes — are seeded, reproducible tests before they ever touch a socket.

## Acceptance
- [x] The engine consumes fired timers and injected randomness; no clock, socket or sleep appears in the logic crate.
- [x] Failing-first harness scenarios from ET-1's vectors: a clean pass; no 200 within the step timeout; 200 but the correlation marker is missing; the register step fails; the echo never rings. Each yields the specified verdict, deterministically, from a fixed seed.
- [x] Scheduling honours interval, jitter and the configured rate bound; the target matrix is walked so each verdict names its edge address, transport and zone.
- [x] `inconclusive` is produced — not `fail` — when the fault is on the probe side, and a harness scenario proves it.
- [x] The run record type is shared with ET-4's API and ET-5's metrics; there is one record shape, not three.

## Progress
`sipx-clstr-probe`, 40 tests, plus 5 harness scenarios. **All 19 `EP-*` rows proved** — the
conformance report is at 53 of 61, with only the M2 proxy rows still deferred.

Four modules: `verdict` (the taxonomy), `marker` (§4), `engine` (the plan), `schedule` (§5's
cadence, jitter and rate bound). No clock, no socket, no sleep anywhere: `now` is a parameter and
jitter is an injected closure.

**One run record**, `ProbeRun`, produced by the engine and consumed by whatever reports it — the
acceptance's "one record shape, not three". `ET-4`'s API and `ET-5`'s metrics read this type rather
than each inventing a view of it.

### Two defects the adversarial harness found

**The engine matched responses to whatever step was outstanding, with no correlation at all.** Under
duplication — which UDP produces routinely — a second `200` to REGISTER arrived while the probe was
waiting on INVITE and was consumed as the INVITE's answer, producing `MarkerMismatch`: a platform
failure the probe manufactured. Caught at seed 5 of a 16-seed sweep. Responses are now matched to the
`CSeq` of the request that provoked them, and a unit vector pins it so the harness is not the only
thing standing between that bug and production.

Fixing it exposed that the *unit fixtures were unrealistic*: they built responses with **no `CSeq`**
at all, which is why they could not have caught this. They now play a `Peer` that echoes the `CSeq`
of the last request expecting an answer, exactly as a real peer does — a fixture that cannot be wrong
in a way a real network cannot be.

**The rate bound's sliding window let a second run straight through.** `now.saturating_sub(per)`
clamps to zero, so the entry recorded at time zero was immediately trimmed and the bound did the
opposite of bounding. Written as `started + per > now` now, with the reason at the site.

### Decisions worth recording

- **A cleanup failure never rewrites a verdict** (§6 V6), and the two obligations — end the dialog,
  remove the registration — are modelled as a *set* rather than two flags. S3 and S4 are the same
  rule, "undo what you did, in reverse order", and modelling them separately is how one gets
  forgotten in a new terminal path.
- **A failed BYE is not retried.** When the failing step *is* the BYE, cleanup must not send another:
  S7 makes retries the transport's job, and a probe that re-sent its own BYE would be measuring its
  own persistence. Two tests failed until that was right.
- **A challenge is a round trip, not a retry.** The engine re-sends the challenged request once —
  which S7 does not forbid, because a challenge is a protocol exchange rather than a lost message —
  and a second challenge after that is the platform's behaviour (`Fail`), not the probe's account
  (`Inconclusive`). `ProbeConfig::has_credentials` is what makes V5's two arms distinguishable at all;
  without it the engine could only ever report the first, and a re-challenging registrar would be
  invisible.
- **The scheduler spreads a matrix's first runs across one interval** rather than emitting all of them
  at zero, because a rollout starts many nodes at once and they would otherwise fire together.
- **The rate bound defers, never drops**, and counts what it deferred: a skipped target is a blind
  spot, a delayed one is only late, and a probe that quietly stopped probing looks exactly like a
  platform that quietly stopped failing.
- **`base64_url` is pinned against RFC 4648's own vectors**, not against its own output, so an
  encoding bug cannot be frozen in by recording what it happened to produce.

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Spec: [e2e-probe](../specs/e2e-probe.md).
- The harness scenario's platform is a stub that answers and reflects — deliberately *not* the real
  proxy and registrar. `CX-3` runs the probe against those; what this scenario is about is the
  engine's behaviour when the **network** chooses the ordering, and a second subject would make a
  failure ambiguous.
- `ET-3` still owes the real echo endpoint. The engine's marker rows are proved against a reflecting
  stub, which proves the *engine*; proving the *echo* is that story.
- The `Resolve` step's `Resolved` input is fed by the driver rather than crossing the simulated
  network, because DNS is not on the SIP link.
