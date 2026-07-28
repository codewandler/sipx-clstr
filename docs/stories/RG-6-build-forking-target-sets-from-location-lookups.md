---
id: RG-6
title: Build forking target sets from location lookups
pillar: Signalling
status: backlog
priority: 
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, proxy]
note: blocked by RG-3, PX-5
---

# Build forking target sets from location lookups

## Goal
Turn a location lookup into what the proxy forks on: ordered branches carrying the Path route set and, when present, the flow_ref.

## Acceptance
- [ ] Lookups return q-value-ordered targets with expired bindings excluded and per-tenant quotas applied.
- [ ] The proxy's forking path consumes the target set end-to-end in the M1 register-and-call harness scenario.

## Progress
- (not started)

## Notes
- Design: [registrar-location](../designs/registrar-location.md).
