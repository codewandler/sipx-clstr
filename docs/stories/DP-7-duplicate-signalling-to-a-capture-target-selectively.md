---
id: DP-7
title: Duplicate signalling to a capture target, selectively by transport
pillar: Cluster
status: backlog
priority: 
design: docs/designs/deployment.md
epic: deployment
areas: [deploy, observability]
note: 
---

# Duplicate signalling to a capture target, selectively by transport

## Goal
Duplicate SIP signalling to a capture collector, with the choice of which transports to duplicate as configuration.

## Acceptance
- [ ] Capture is enabled per transport — typically only the encrypted ones, because plaintext is captured off the wire elsewhere.
- [ ] Target, protocol version and enablement are config.
- [ ] Duplication does not double-count when another capture path is already active.
- [ ] Capture failure never affects call handling.

## Progress
- (not started)

## Notes
- babelforce captures plaintext with a node-level agent and duplicates only TLS from the proxy, precisely to avoid double entries in the trace store.
- Filed from the babelforce-sip-clstr deployment (`~/babelforce/projects/babelforce-sip-clstr`), whose capability inventory records this as `upstream`. Requirement **U-10** in that repo's `docs/upstream.md`; evidence in its `docs/reference/environments.md`.
