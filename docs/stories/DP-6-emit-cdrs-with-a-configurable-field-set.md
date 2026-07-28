---
id: DP-6
title: Emit CDRs with a configurable field set
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, observability]
note: the field list is an external billing contract
---

# Emit CDRs with a configurable field set

## Goal
Emit call detail records whose field set is configuration, because the fields are a contract with whatever consumes them.

## Acceptance
- [ ] The CDR field list is declared in config; adding or removing a field is a config change.
- [ ] Fields can carry values set during routing (direction, selected egress, tenant identifier, media statistics).
- [ ] The emission target is pluggable.
- [ ] A test asserts the emitted field set matches the declared one exactly.

## Progress
- (not started)

## Notes
- babelforce's current CDR fields are a contract with billing and must survive the migration unchanged: call id, reply code, from/to numbers, carrier, direction, source user-agent, customer identifier and RTP statistics.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-9** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
