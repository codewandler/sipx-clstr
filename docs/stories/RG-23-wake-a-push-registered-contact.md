---
id: RG-23
title: Wake a push-registered contact and resume delivery after refresh
pillar: Signalling
status: backlog
priority: 13
design: docs/designs/registrar-location.md
epic: registrar-location
areas: [registrar, push, proxy]
note: M3/M4 · blocked by RG-22 and deployment push-adapter configuration
---

# Wake a push-registered contact and resume delivery after refresh

## Goal

Turn stored push parameters into a bounded wake-and-refresh orchestration for an incoming call.

## Acceptance

- [ ] The location-service spec states the binding state, wake deduplication key, refresh deadline and
      final response when no refreshed reachable contact appears.
- [ ] A lookup of a sleeping push-capable binding emits a driver push effect and parks only the owning
      branch within a finite timer budget.
- [ ] A matching refresh REGISTER resumes delivery through the new flow; unrelated registrations do
      not release the branch.
- [ ] Provider credentials and network I/O live in a configured driver adapter and never enter the
      sans-IO registrar/proxy cores or logs.
- [ ] Concurrent calls deduplicate wake requests without merging their SIP transaction outcomes.
- [ ] Failing-first `OB-6` wakes a disconnected WSS client into an answered call.

## Progress

- Not started.
