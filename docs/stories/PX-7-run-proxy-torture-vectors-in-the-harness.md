---
id: PX-7
title: Run proxy torture vectors in the harness
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, harness]
note: blocked by PX-5, CF-5
---

# Run proxy torture vectors in the harness

## Goal
Run the complete PX-1 vector suite plus adversarial schedules — retransmission storms, reordering, duplicate UDP — as seeded harness scenarios in CI.

## Acceptance
- [ ] Every PX-1 vector is executed by a named harness scenario; failures reproduce from the printed seed.
- [ ] Adversarial schedules (retransmission, reorder, duplication) run green across a seed corpus in CI.

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
