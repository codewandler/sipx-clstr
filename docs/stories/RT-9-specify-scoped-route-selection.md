---
id: RT-9
title: Specify tenant-scoped route selection
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks]
note: several pools can be 'default' at once; a single global default cannot express it
---

# Specify tenant-scoped route selection

## Goal
Support route selection scoped by tenant, so that a deployment can express 'the default egress for this tenant' alongside 'the default egress for everyone else'.

## Acceptance
- [ ] Selection is scope × trigger: a scope (a tenant identifier, or shared) crossed with a trigger (default within that scope, dialled prefix, or destination domain).
- [ ] Several routes may be `default` simultaneously provided their scopes differ; overlapping scopes are a validation error, not a race.
- [ ] A route reachable only by an external routing decision (no trigger) is expressible.
- [ ] Precedence between triggers within a scope is specified and tested.

## Progress
- (not started)

## Notes
- Production EU scopes 8 of its 12 carrier pools to a customer UUID, which is why four pools there are simultaneously flagged default.
- Two further pools have no trigger at all and are selected only by an external lookup.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-14** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
