---
id: FC-6
title: Apply or refuse the declared node roles, so a node cannot answer a method its role excludes
pillar: Cluster
status: ready
priority: 1
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, node]
note: release blocker — roles pick listeners and the store, then are dropped; an inbound-proxy accepts and stores a REGISTER
---

# Apply or refuse the declared node roles, so a node cannot answer a method its role excludes

## Goal
Carry the projected role set into runtime dispatch, so a node performs only the methods its declared
roles cover — and refuses to start when it is asked for a role it cannot serve.

## Acceptance
- [ ] `NodeConfig` carries the node's roles (or a capability set derived from them) from
      `ProjectedConfig.identity` through `startup::node_config`.
- [ ] Dispatch consults them: a node without the `registrar` role does not process `REGISTER`; a node
      without a proxy role does not proxy. The refusal is a SIP answer with a status the spec names,
      not a silent drop.
- [ ] `e2e-tester` and `echo` roles are selected by dispatch rather than merely linked.
- [ ] **Failing-first test:** a node configured as `inbound-proxy` only is sent a `REGISTER` and does
      not create a binding. This fails on `86e6b10`, where it answers `200 OK` and stores it.
- [ ] A role the build cannot serve stops the node at startup rather than being ignored — the
      `FC-1`/`FC-3` "apply or refuse" shape.
- [ ] The vector rows for cluster-config `R3` are proved rather than deferred, or the deferral names
      this story.

## Progress
- (not started)

## Notes
- Found by the independent adversarial review of `86e6b10` (`v0.12.0`), finding **V-01**, and
  reproduced against the real binary.
- Evidence: roles are carried at `crates/sipx-clstr-node/src/config/mod.rs:323-337` and used to pick
  listeners and the location store, but `startup::node_config`
  (`crates/sipx-clstr-node/src/startup.rs:156-235`) does not transfer them; `NodeConfig`
  (`crates/sipx-clstr-node/src/driver.rs:34-84`) has no role field; and `serve`
  (`driver.rs:795-808`) dispatches on method alone — `Register` to the registrar, `Ack` to the
  stateless path, everything else to the proxy.
- Conflicts with [cluster-config](../specs/cluster-config.md) §4 `R3`.
- This is why the `0.12.0` README and site were narrowed to stop calling role separation current.
- Considered for upstream: **no.** Which methods a deployment's role serves is cluster orchestration;
  the kernel has no notion of our roles.
