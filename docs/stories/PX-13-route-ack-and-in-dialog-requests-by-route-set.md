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
- [ ] ACK handling is split by semantics: the kernel-generated downstream ACK for a non-2xx remains
      transaction-scoped; an upstream non-2xx ACK is absorbed by its server transaction; a 2xx ACK is
      a separately routed request and is never answered.
- [ ] BYE, re-INVITE and every other in-dialog request use the core's selected next hop directly
      instead of a location lookup. The driver does not edit the message after the pure engine has
      applied Route preprocessing.
- [ ] An unroutable in-dialog request settles as an explicit state-machine input — never a silent
      drop.
- [ ] **Failing-first test:** a call whose callee's Contact is not a registered AoR — the ordinary
      case — receives its 2xx ACK and is torn down by a Route-set BYE that reaches the far end. The
      test asserts the ACK is sent exactly once, neither request performs an AoR lookup, and the BYE's
      2xx returns on the existing Via path. This fails on `86e6b10`, where the ACK is dropped.
- [ ] `ET-7` owns correcting the synthetic probe after this real-node path lands; PX-13's socket test
      uses a protocol-correct test UA and cannot pass with AoR-shaped ACK/BYE shortcuts.
- [ ] The two-node proofs continue to pass, and one of them uses a remote target that is not an AoR.
- [ ] `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Validated synthesis finding [**V-03**](../reviews/00-validated-synthesis.md#v-03--ack-and-in-dialog-requests-are-routed-as-registrar-lookups); the protocol and assurance reviews found it independently.
- Evidence: `crates/sipx-clstr-node/src/driver.rs:800-807` routes every `Ack` through
  `forward_statelessly`; `:1189-1196` and `:1237-1257` resolve the Request-URI as an AoR, take the
  first registration, and drop the request when there is none. Correct preprocessing and next-hop
  selection already exist at `crates/sipx-clstr-proxy/src/context.rs:111-169`. The probe builds
  `ACK`/`BYE` to a configured AoR at `crates/sipx-clstr-probe/src/engine.rs:446-455`, `:635-658`.
- **Why the harness did not catch it:** the simulation's `ACK`/`BYE` are AoR-shaped, so they resolve
  by accident. A real remote Contact is not the registered AoR.
- **Upstream boundary:** generic URI resolution and ACK transaction semantics are sipx capabilities;
  choosing location lookup versus direct route/flow delivery and feeding the result to the local
  response context are cluster orchestration.
