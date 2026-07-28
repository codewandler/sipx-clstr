---
id: ET-1
title: Specify the e2e-tester role and the probe contract
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe]
note: gates ET-2 … ET-6
---

# Specify the e2e-tester role and the probe contract

## Goal
Write `docs/specs/e2e-probe.md`: what a probe run is, what it asserts, what a verdict means, and the shape of the trigger API — so the engine, the echo endpoint and the API are derived from one contract rather than three opinions.

## Acceptance
- [ ] The spec defines the probe plan as a state machine (`register → invite → assert → bye`) with per-step timeouts, the correlation marker carried in the request, and the inputs (fired timers, injected randomness) that keep it sans-IO.
- [ ] Verdict taxonomy is normative: `pass`, `fail(step, cause)`, `inconclusive(probe-side fault)` — with the cause list and the rule that a probe-side fault is never reported as a platform failure.
- [ ] The target matrix is specified (per edge address, per transport, per zone; public path only — DNS/VIP, never an internal shortcut) along with schedule, interval jitter and rate bounds.
- [ ] The control API schema is specified (`GET /probes`, `POST /probes/{name}/runs`, `GET /probes/{name}/runs/{id}`), private-interface-only, authenticated, with the run record identical for scheduled and triggered runs.
- [ ] Blast-radius rules are normative: test tenant with no trunk access, identifying marker excluding probe traffic from business metrics/CDR, behavior when overload control sheds a probe.
- [ ] Test vectors: at least one passing run and one failure per step, expressed so ET-2's harness scenarios derive from them.
- [ ] Decided and recorded: whether the echo endpoint is the same binary in `echo` mode or a separate service, and how the role slots into DP-1's role set.

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Role set and config schema: [DP-1](DP-1-design-roles-and-the-config-schema.md).
- Media assertions are explicitly out of scope here — deferred to a relay-mediated echo, never RTP in-process.
