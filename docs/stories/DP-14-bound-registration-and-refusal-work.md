---
id: DP-14
title: Bound registration and refusal work outside the proxy transaction admission ceiling
pillar: Cluster
status: ready
priority: 2
design: docs/designs/deployment.md
epic: deployment
areas: [driver, registrar, security, observability]
note: V-11 — REGISTER and every refusal still spawn without a process-wide work bound
---

# Bound registration and refusal work outside the proxy transaction admission ceiling

## Goal

Put a measurable ceiling around every request task the node creates, including REGISTER and overload
responses, without letting proxy saturation consume the registrar's entire capacity or turning
overload logging into the attacker-controlled work multiplier.

## Acceptance

- [ ] Keep `DP-11`'s proxy transaction permits and add separate bounded registrar, stateless-ACK,
      and refusal-response lanes. No arrival path calls `tokio::spawn` unless it owns capacity in the
      lane whose work it creates; lane capacity is fixed before sockets open and has no unbounded
      queue behind it.
- [ ] The registrar lane has reserved capacity independent of proxy permits. When it is full, a new
      REGISTER receives `503 Service Unavailable` with `Retry-After` if response capacity is
      available; otherwise it is counted and dropped without creating another task. A refresh storm
      can therefore degrade registrations but cannot make resident work grow with offered load.
- [ ] The refusal lane is a fixed worker set/queue rather than one task per rejected request. A full
      refusal lane drops and increments a counter. ACK still receives no SIP response; when no total
      execution capacity exists it is dropped and counted in the existing ACK-specific shed signal.
- [ ] Authentication audit records remain complete for REGISTERs that reach authentication. Requests
      rejected before authentication are overload events, not fabricated authentication outcomes;
      their counts are sampled/aggregated, never logged once per message.
- [ ] Cluster-config V11 is updated before code to distinguish proxy-transaction capacity from the
      new registrar and response capacities, including defaults, non-zero/maximum validation, and
      exact overload behavior. No hidden constant becomes the effective public limit.
- [ ] **Failing-first real-socket tests:** (a) a stalled PostgreSQL store plus a REGISTER flood,
      (b) a saturated proxy plus concurrent registration refreshes, and (c) a stalled response
      transport plus an INVITE flood. Each records peak tasks/permits/queue depth and fails on
      `86e6b10`, where exempt and refused arrivals spawn indefinitely as the accept loop drains.
- [ ] The tests assert bounded log records as well as bounded tasks and show capacity is released on
      success, parse rejection, store failure, send failure, timeout, and task cancellation.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Source: validated synthesis **V-11**. `AdmissionBound::gates` exempts REGISTER/ACK
  (`crates/sipx-clstr-node/src/driver.rs:306-319`); the accept loop spawns both refusal tasks and
  exempt request tasks (`:524-588`); REGISTER emits an authentication record per processed request.
- Dependencies: none. `DP-11` supplies the existing proxy-transaction bound and counters. `RG-21`
  improves the PostgreSQL path but is deliberately not a prerequisite: this story must bound even a
  stalled driver.
- Considered for upstream: **mostly no.** Capacity partitioning, registrar admission, and audit-log
  policy are local deployment orchestration. Kernel queue shedding/per-message logging is already a
  separate upstream-ledger concern and is not shadow-fixed here.
