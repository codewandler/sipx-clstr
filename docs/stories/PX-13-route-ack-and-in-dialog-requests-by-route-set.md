---
id: PX-13
title: Route ACK and in-dialog requests by the Route set, not by an address-of-record lookup
pillar: Signalling
status: ready
priority: 1
design: docs/designs/proxy-transaction-driver.md
epic: proxy-engine
areas: [proxy, node]
note: release blocker — every ACK is resolved as an AoR and silently dropped when no binding exists
---

# Route ACK and in-dialog requests by the Route set, not by an address-of-record lookup

## Goal
Deliver `ACK` and in-dialog requests to the next hop the dialog names, rather than treating their
Request-URI as an address of record and asking the location service about it.

## Acceptance
- [ ] `ACK` for a 2xx is forwarded using the `Route` set and the dialog's remote target, honouring the
      core's route preprocessing rather than bypassing it.
- [ ] In-dialog requests use the core's selected next hop directly instead of a location lookup.
- [ ] An unroutable in-dialog request settles as an explicit state-machine input — never a silent
      drop.
- [ ] The probe retains the dialog's remote target and route set instead of reconstructing
      `ACK`/`BYE` to a configured AoR.
- [ ] **Failing-first test:** a call whose callee's Contact is not a registered AoR — the ordinary
      case — completes and is torn down by a `BYE` that reaches the far end. This fails on `86e6b10`,
      where the ACK is dropped.
- [ ] The two-node proofs continue to pass, and one of them uses a remote target that is not an AoR.

## Progress
- (not started)

## Notes
- Found by the independent adversarial review of `86e6b10` (`v0.12.0`), finding **V-03**; the protocol
  and assurance reviews found it independently.
- Evidence: `crates/sipx-clstr-node/src/driver.rs:800-807` routes every `Ack` through
  `forward_statelessly`; `:1189-1196` and `:1237-1257` resolve the Request-URI as an AoR, take the
  first registration, and drop the request when there is none. Correct preprocessing and next-hop
  selection already exist at `crates/sipx-clstr-proxy/src/context.rs:111-169`. The probe builds
  `ACK`/`BYE` to a configured AoR at `crates/sipx-clstr-probe/src/engine.rs:446-455`, `:635-658`.
- **Why the harness did not catch it:** the simulation's `ACK`/`BYE` are AoR-shaped, so they resolve
  by accident. A real remote Contact is not the registered AoR.
- Considered for upstream: **partly.** Generic URI resolution and `ACK` transaction semantics belong
  to the kernel and must be checked there first; choosing location lookup versus direct route/flow
  delivery is cluster orchestration and stays here.
