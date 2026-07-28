---
id: ET-4
title: Implement the probe control API
pillar: Platform
status: backlog
priority: 
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe]
note: blocked by ET-2; private management interface only
---

# Implement the probe control API

## Goal
Expose the trigger API specified in ET-1 so a person, a pipeline or the Kubernetes operator can demand a test call on the spot and act on the verdict.

## Acceptance
- [ ] `GET /probes`, `POST /probes/{name}/runs` (returning a run id, or the verdict when asked to block) and `GET /probes/{name}/runs/{id}` behave per the ET-1 schema.
- [ ] A triggered run and a scheduled run produce byte-identical run records apart from their trigger source — proved by a test, not by inspection.
- [ ] The listener binds only to the private management interface and requires authentication (mTLS or bearer); a test asserts it is not served on any public/SIP listener.
- [ ] Concurrency and rate limits are enforced: a caller cannot flood the platform with triggered probes.
- [ ] Non-zero exit / error status is machine-readable enough to gate a rollout on it.

## Progress
- (not started)

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Consumed as a rollout gate by [k8s-deployment-operator](../designs/k8s-deployment-operator.md).
