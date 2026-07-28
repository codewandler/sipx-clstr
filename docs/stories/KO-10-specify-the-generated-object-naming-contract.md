---
id: KO-10
title: Specify the generated-object naming and labelling contract
pillar: Cluster
status: backlog
priority: 
design: docs/designs/k8s-deployment-operator.md
epic: k8s-deployment-operator
areas: [k8s, deploy]
note: tooling cannot address a role's workload without it
---

# Specify the generated-object naming and labelling contract

## Goal
State, as a contract, how the operator names and labels the objects it generates, so operators and tooling can address a role's workload deterministically.

## Acceptance
- [ ] Workload names are derived from the custom resource's name and the role, by a documented rule.
- [ ] Standard labels distinguish the platform, the owning custom resource, the role and the managed-by tool.
- [ ] The rule is stable across upgrades — renaming generated objects is a breaking change and is called out as such.
- [ ] A test asserts the generated names and labels for a multi-role cluster.
- [ ] The contract is documented where a chart or a dev tool author will find it.

## Progress
- (not started)

## Notes
- Without this, anything outside the operator — a dev loop, a port-forward, an alert rule, a runbook — has to guess a workload name, and the guess breaks silently when the operator changes.
- The consuming deployment addresses roles as `<cr-name>-<role>` and relies on the CR being named after the Helm release.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`); requirement **U-15** in that repo's `docs/upstream.md`.
