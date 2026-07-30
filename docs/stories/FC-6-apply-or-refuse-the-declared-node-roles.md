---
id: FC-6
title: Apply or refuse the declared node roles, so a node cannot answer a method its role excludes
pillar: Cluster
status: in-progress
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
- [x] `NodeConfig` carries the node's roles (or a capability set derived from them) from
      `ProjectedConfig.identity` through `startup::node_config`.
- [x] Dispatch consults them: a node without the `registrar` role does not process `REGISTER`; a node
      without a proxy role does not proxy. The refusal is a SIP answer with a status the spec names,
      not a silent drop.
- [ ] `e2e-tester` and `echo` roles are selected by dispatch rather than merely linked.
- [x] **Failing-first test:** a node configured as `inbound-proxy` only is sent a `REGISTER` and does
      not create a binding. This fails on `86e6b10`, where it answers `200 OK` and stores it.
- [x] A role the build cannot serve stops the node at startup rather than being ignored — the
      `FC-1`/`FC-3` "apply or refuse" shape.
- [x] The vector rows for cluster-config `R3` are proved rather than deferred, or the deferral names
      this story.

## Progress
- **The roles reach dispatch.** `Capabilities { registrar, proxy }` is derived from the role set
  (`crates/sipx-clstr-node/src/config/mod.rs:163`), carried on `NodeConfig`
  (`driver.rs:49`) and assigned from `ProjectedConfig.identity` in `startup::node_config`
  (`startup.rs:234`). `serve` now routes through `Dispatch::of` (`driver.rs:819`), which is a pure
  function of the wiring and the method — §4 R3 forbids consulting the *role set* when a request is
  classified, and a decision that needs no socket is the only kind AGENTS.md #2 keeps.
- **The refusal is `405` with `Allow`** (RFC 3261 §21.4.6), `driver.rs:891`. **One exception, stated
  rather than discovered:** an ACK on a node with no forwarding path is dropped and logged, because
  RFC 3261 §17.1.1.3 makes an ACK for a 2xx a transaction nothing answers — a `405` there would put a
  response on a transaction that has none.
- **A role this build cannot serve stops the node**, by name and with the section that would have
  configured it: `StartupError::RoleNotServed` (`startup.rs:60`, raised at `startup.rs:180`).
- **`CC-R-1` is proved** rather than deferred to `DP-8`
  (`crates/sipx-clstr-node/src/config/tests.rs:928`); its deferral is removed from
  `docs/reference/vector-scope.toml` and `conformance.md` regenerated (126/549).
- **Not done: `echo`/`e2e-tester` are refused, not served.** They are no longer merely linked — a node
  given either now refuses to start instead of coming up as a full proxy *and* registrar, which is
  what e2e-probe §9 forbids absolutely — but nothing dispatches to an echo endpoint. That needs
  `EchoConfig`'s `aor`, `contact` and `registrar`, which live in the `cluster.echo` section; §7 gives
  that section's content to [e2e-probe](../specs/e2e-probe.md) §9, **which does not define its
  fields**. Inventing them here would be a schema written in a driver story, against AGENTS.md #4 and
  §7 S1. Wiring only the answering half was rejected for the epic's own rule 1: an echo that answers
  but never registers cannot be found by the probe and looks configured. The next story is a spec
  addition to e2e-probe §9 plus a loader section, then the endpoint driver.
- **Two live claims are now false and are the coordinator's to correct:** `README.md:174` and
  `website/docs/intro.md:48` both say the roles are dropped before dispatch and that an
  `inbound-proxy` will accept and store a `REGISTER`. Left untouched deliberately — they are the
  release's capability matrix, written at `2cb22dd`, and four sibling blocker stories edit the same
  two rows.
- **Deployment consequence:** the chart's default set (`deploy/helm/values.yaml:112,119`) creates an
  `e2e-tester` and an `echo` workload. Those two now refuse to start with a named reason instead of
  silently running a proxy and a registrar. `deploy/helm/check-values.sh` still passes — it fails only
  on a *document* refusal — but it will report those two identities as "loaded (exit 2 …)".

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
