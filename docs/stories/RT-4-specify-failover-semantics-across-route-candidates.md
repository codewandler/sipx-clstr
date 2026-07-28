---
id: RT-4
title: Specify failover semantics across route candidates
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, dns]
note: 
---

# Specify failover semantics across route candidates

## Goal
Specify exactly which failures advance to the next RFC 3263 candidate, which terminate the plan, and how the element becomes stateful after first failover (RFC 3263 §4.4).

## Acceptance
- [ ] The advance/terminate taxonomy is normative with vectors, including mid-transaction DNS failover.
- [ ] Post-failover statefulness is specified so retransmissions follow the selected destination.

## Progress
- (not started)

## Notes
- Design: [routing-trunks](../designs/routing-trunks.md).
