---
id: PX-6
title: Implement CANCEL and Timer C
pillar: Signalling
status: done
priority:
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: M1 #6 · CANCEL and Timer C, proved under adversarial harness schedules
---

# Implement CANCEL and Timer C

## Goal
Implement CANCEL propagation and Timer C: upstream CANCEL fans out to open branches, a better final response cancels the rest, and silent INVITE branches are reaped.

## Acceptance
- [x] Upstream CANCEL propagates to all open branches; branches cancelled before a final response produce `487` per the spec vectors (after a final response, CANCEL is a no-op per §9.2).
- [x] A first 2xx cancels remaining branches; Timer C expiry cancels a silent branch.
- [x] All CANCEL/Timer C vectors pass under adversarial schedules in the harness.

## Progress
§9's C1–C6 in `sipx-clstr-proxy`, with `PB-C-1`, `-2`, `-3`, `-5` and `-6` as unit vectors and five
scenarios in `sipx-clstr-sim` — 37 proxy tests and 10 harness tests in total.

**C1 is its own effect.** `Effect::AnswerCancel` is separate from `Effect::Respond` because it
answers a *different transaction*: a CANCEL is its own transaction (RFC 3261 §9), and §16.10 makes
its `200` immediate and unconditional. It says "I received your CANCEL", not "the call is
cancelled" — conflating the two would make the acknowledgement wait on the INVITE's outcome. The
vector asserts it comes **first** in the effect list.

**C2's queueing is the subtle half.** A branch that has produced no provisional must not be sent a
CANCEL, because the CANCEL can overtake the INVITE at the next hop — cancelling a transaction that
does not exist yet, which the far end answers `481` while the INVITE it was meant to stop proceeds.
So the cancel is *queued* on the branch and released by its first provisional. Both halves are
asserted: the ringing branch is cancelled now, the silent one is not, and when it finally rings the
queued CANCEL goes out.

**C5 and C6 differ for a reason the code states.** Timer C with provisionals seen means the far end
is alive but stuck: cancel it, and let C3 produce a real `487`. Timer C with total silence means
there is no transaction worth cancelling, so the branch concludes as `408` (R9). Fabricating a
timeout in the first case, or a cancellation in the second, would each put a status on the wire that
nothing reported.

**C4 is deliberately not in this crate.** A CANCEL matching no transaction is forwarded statelessly
— and by definition no response context exists for it, so it never reaches the engine. It is the
driver's, and the driver design says so.

### Two tests that were passing for the wrong reason

Both were caught by asking *why* they passed, which is the only way this class of defect surfaces.

1. **The harness driver silently dropped every CANCEL.** Writing
   `self.perform(self.context.on_input(…))` borrows `self` twice, and I worked around it with a stub
   helper that returned no effects — so `Input::UpstreamCancelled` never reached the engine.
   Splitting the expression across two statements was the actual fix.
2. **…and the adversarial sweep passed anyway.** `run_until_idle` drains the whole event queue,
   *including Timer C at 180 s*. Every seed reached `487` — via C5 reaping a silent branch, not via
   the CANCEL. A test whose subject is CANCEL must not be able to succeed through the timer, so the
   scenarios now advance a bounded 10 s of virtual time. `BEFORE_TIMER_C` carries that reasoning at
   its definition.

**The sweep, once it meant something:** 24 seeds with 1–40 ms jitter and 25 % duplication, so
provisionals race the CANCEL, messages reorder, and requests arrive twice. Every seed reaches `487`
within the bound, and eight seeds are additionally asserted to replay byte for byte.

**Timer C is tested in virtual time**, which is the whole argument for a virtual clock: the same
assertion against a real clock would take three minutes per run.

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
- The harness's `ProxyNode` is the first place the real engine runs behind a driver. It mints its
  CANCELs per §9.1 — same Request-URI, `Call-ID`, `To`, `From`, `CSeq` number, and the same top `Via`
  branch as the INVITE — and matches responses to branches by that branch, which works precisely
  because the engine makes the branch id and the `Via` branch the same string. `PX-7` builds the full
  vector report on this driver.
