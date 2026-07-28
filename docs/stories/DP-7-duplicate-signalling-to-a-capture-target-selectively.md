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
- One deployment captures plaintext with a node-level agent and duplicates only TLS from the proxy, precisely to avoid double entries in the trace store.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-10**). The evidence sits in that deployment's own reference material.
