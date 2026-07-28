---
id: PX-8
title: Decide whether topology hiding is in scope, and how it survives a node change
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy, topology]
note: babelforce's current implementation is the defect the cluster design exists to fix
---

# Decide whether topology hiding is in scope, and how it survives a node change

## Goal
Decide whether the platform hides internal topology from peers, and if so specify a mechanism whose state does not pin a dialog to one node.

## Acceptance
- [ ] A decision, recorded: in scope or not, with the reasoning.
- [ ] If in scope: the mechanism is specified, and mid-dialog requests are servable by any healthy node — state rides the message or is genuinely shared.
- [ ] If not in scope: the consequence for deployments that rely on it today is written down.
- [ ] Interaction with affinity tokens and Record-Route is specified either way.

## Progress
- (not started)

## Notes
- babelforce hides topology today by storing every dialog in a database keyed by the pod's own name, which makes replicas non-interchangeable and forces a 15-minute drain on every rollout. One production region runs a single replica as a result.
- This is the concrete instance of the problem the affinity-token design exists to solve, so the answer here shapes AF.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-3** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
