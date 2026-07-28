---
id: PX-3
title: Header surgery API in sipx
pillar: Signalling
status: backlog
priority: 
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: UPSTREAM — see docs/upstream.md
---

# Header surgery API in sipx

## Goal
Land `remove_first`/`insert_at`/`retain` (or equivalent) on sipx's `Headers` so Via pop/push and Record-Route insertion need no private collection rebuild.

## Acceptance
- [ ] The sipx story is filed (CX-1) and landed; sipx-clstr pins a kernel version exposing the API.
- [ ] The proxy's Via and Record-Route mutations use the upstream API exclusively.

## Progress
- (not started)

## Notes
- Upstream ledger: [upstream.md](../upstream.md). Blocks PX-4.
