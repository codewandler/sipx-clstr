---
id: DP-13
title: Wire declared roles into runtime capabilities instead of a method-global dispatcher
pillar: Cluster
status: in-progress
priority: 1
design: docs/designs/deployment.md
epic: deployment
areas: [config, node, deploy]
note: V-01 fail-open is closed; the 503/481 shape, the counted ACK and the echo wiring remain
---

# Wire declared roles into runtime capabilities instead of a method-global dispatcher

## Goal

Carry projection's role decision into runtime wiring so a node exposes only the services its identity
declares. Roles choose which handlers exist; request content still decides behavior inside a handler.

## Acceptance

- [x] `ProjectedConfig.identity.roles` becomes a closed runtime capability/handler set in
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
- [x] The existing R6 refusal for mixing call-path and probe roles remains, and every individually
      accepted role either produces its declared runtime or a startup error naming the missing
      capability.
- [ ] **Failing-first real-binary matrix:** an `inbound-proxy`-only node receives REGISTER and creates
      no binding; an `edge`-only node does the same; a `registrar`-only node receives INVITE and sends
      no downstream request; `echo` executes the echo behavior; `e2e-tester` refuses startup. The
      first case fails on `86e6b10` with `200 OK` and a stored binding.
- [x] Cluster-config R3's deferred vectors are proved at the runtime boundary, not only by checking
      which listener/store projection returned.
- [ ] Public role documentation is updated only after those socket tests prove the corresponding
      capability matrix, and `scripts/gate.sh` is green.

## Progress

- **The fail-open is closed** (merged 2026-07-30). `Capabilities` is derived from
  `ProjectedConfig.identity.roles` in `startup::node_config` and carried on `NodeConfig`; `Dispatch::of`
  consults it, so an `inbound-proxy` no longer accepts and stores a `REGISTER`. A role this build
  cannot serve (`echo`, `e2e-tester`) raises `StartupError::RoleNotServed` instead of being ignored.
  Failing-first proof `dp13_an_inbound_proxy_does_not_register_anyone`, verified failing at the merge
  base by the implementor and re-run independently at review.
- Roles become a **capability set** rather than a `BTreeSet<Role>` on the hot path, because
  `cluster-config` §4 `R3` forbids consulting a role when classifying a request — carrying the raw set
  into the request path would invite exactly that.
- **Independent review: `PASS` on the code.** No fail-open reachable (`startup.rs:229` is the only
  non-test `NodeConfig::listening` caller; `--roles ""` exits 2 on `CC-R4`); the refusal path is
  panic-free and clippy-clean; `RoleNotServed` fires for exactly the unservable roles; `CC-R-1`'s
  deferral removal is backed by a real assertion, not a stale tick.

### Deliberately **not** closed by that merge

This story superseded the `FC-6` it was written as, and three of its items describe behaviour the
merged diff does not have. They are left unticked rather than ticked without code:

- **`503 Service Unavailable` with `Retry-After`, and `481` for an unmatched CANCEL.** The diff
  answers **`405 Method Not Allowed` with `Allow`**, citing RFC 3261 §21.4.6. In a cluster this story's
  status is the better one: `405` tells a client the method is permanently unavailable *here* and it
  should stop, where `503` + `Retry-After` invites failover to a node that does serve the role. An
  unmatched CANCEL currently gets `405` where §9.2 wants `481` (at the base it got `404`, so this is
  not a regression).
- **The ACK drop is logged but not counted.** The drop itself is correct and has in-tree precedent —
  `cluster-config` V11 makes the identical argument with the identical citation, that an ACK for a 2xx
  has no response in SIP at all (RFC 3261 §17.1.1.3) — but this story asks for a counter.
- **The echo engine is not wired.** It cannot be, yet: `e2e-probe` §9 fixes `E1`–`E5` behaviour and
  defines **no** configuration fields, `cluster-config` §7 assigns the `echo` section's content to that
  §9, and both `cluster.echo` and `cluster.probe` sit in `DEFERRED_SECTIONS`. Wiring only the
  answering half was rejected on purpose: an echo that answers but never registers cannot be found by
  the probe and *looks* configured, which is this epic's third state. **A spec addition to §9 comes
  first** — that is the blocker, and no rework of the driver reaches it.

### Known consequences, recorded rather than fixed

- `deploy/helm/values.yaml:112,119` puts `e2e-tester` and `echo` in the default set at `replicas: 1`.
  Both identities now exit 2 at startup, so a controller creating those workloads gets a crash loop.
  **No impact today** — the chart renders only the `SipxCluster` resource and there is no operator
  (`KO-3`) — but `KO-2`/`KO-3` must set them to `0` or wait for the echo wiring above.
- `deploy/helm/check-values.sh` still prints "the rendered cluster document loads for every role in the
  default set" and exits 0 while reporting those two identities as "(exit 2 after the load —
  environment, not schema)". The label is now false. The script is not in the gate, so nothing failed.
- `Allow` understates for a proxy node: it lists INVITE/ACK/BYE/CANCEL/OPTIONS, but the catch-all
  forwards *any* method, so MESSAGE and SUBSCRIBE are served and unlisted. A subset does not violate
  §21.4.6, but the header is not the set the node serves.
- `Capabilities::CALL_PATH` is the default for the in-code `NodeConfig` constructors — open-by-default
  for any future non-document construction path. Confined today to tests plus `startup.rs:229`.

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
