---
id: DP-13
title: Wire declared roles into runtime capabilities instead of a method-global dispatcher
pillar: Cluster
status: ready
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [config, node, deploy]
note: V-01 release blocker — roles select listeners and the store, then disappear before dispatch
---

# Wire declared roles into runtime capabilities instead of a method-global dispatcher

## Goal

Carry projection's role decision into runtime wiring so a node exposes only the services its identity
declares. Roles choose which handlers exist; request content still decides behavior inside a handler.

## Acceptance

- [ ] `ProjectedConfig.identity.roles` becomes a closed runtime capability/handler set in
      `NodeConfig`; `startup::node_config` cannot construct a running node while dropping it.
- [ ] An `edge`-only node and either proxy-only role do not process `REGISTER` or mutate any location
      store. A `registrar`-only node does not proxy `INVITE`, `OPTIONS`, BYE, CANCEL, or ACK. A
      non-ACK request with no role handler receives `503 Service Unavailable` with `Retry-After`,
      except an unmatched CANCEL, which follows RFC 3261 §9.2 and receives `481`; ACK is dropped and
      counted because SIP permits no response. No method is silently routed to another role's
      handler.
- [ ] The implemented echo engine is wired for `echo` rather than the ordinary SIP dispatcher.
      `e2e-tester` has no runtime driver today, so an identity containing it refuses startup until
      `ET-4`/`ET-6` supply that driver. A probe role must never fall through to proxy/registrar service.
- [ ] The existing R6 refusal for mixing call-path and probe roles remains, and every individually
      accepted role either produces its declared runtime or a startup error naming the missing
      capability.
- [ ] **Failing-first real-binary matrix:** an `inbound-proxy`-only node receives REGISTER and creates
      no binding; an `edge`-only node does the same; a `registrar`-only node receives INVITE and sends
      no downstream request; `echo` executes the echo behavior; `e2e-tester` refuses startup. The
      first case fails on `86e6b10` with `200 OK` and a stored binding.
- [ ] Cluster-config R3's deferred vectors are proved at the runtime boundary, not only by checking
      which listener/store projection returned.
- [ ] Public role documentation is updated only after those socket tests prove the corresponding
      capability matrix, and `scripts/gate.sh` is green.

## Progress

- (not started)

## Notes

- Source: validated synthesis **V-01**. Roles are carried in projection at
  `crates/sipx-clstr-node/src/config/mod.rs:323-337`, disappear in
  `startup::node_config` (`crates/sipx-clstr-node/src/startup.rs:156-235`), and the global dispatcher
  at `crates/sipx-clstr-node/src/driver.rs:795-808` serves REGISTER, ACK, or proxy unconditionally.
- Dependencies: none. `DP-1` and `DP-10` already provide the role schema and startup projection;
  `ET-4`/`ET-6` are not blockers because the honest current behavior for `e2e-tester` is refusal.
- Considered for upstream: **no.** sipx has SIP transactions and user-agent machinery, not this
  deployment's roles. Selecting which local services a configured process wires is cluster
  orchestration.
