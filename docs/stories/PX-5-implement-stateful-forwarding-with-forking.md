---
id: PX-5
title: Implement stateful forwarding with forking
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: blocked by PX-2
---

# Implement stateful forwarding with forking

## Goal
Implement transaction-stateful forwarding (§16.2): one server transaction, N branches, response context, and Record-Route insertion carrying the affinity token.

## Acceptance
- [ ] Parallel and serial forking vectors pass, including best-response selection (§16.7) and provisional forwarding.
- [ ] Record-Route insertion round-trips the affinity token (with AF-4's library).
- [ ] Branch failure (transport error, timeout) advances or concludes the context per the spec.

## Progress
- (not started)

## Notes
- Design: [proxy-engine](../designs/proxy-engine.md).
