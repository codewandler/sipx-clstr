---
id: EX-3
title: Implement the hook runtime
pillar: Platform
status: backlog
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: blocked by EX-1
---

# Implement the hook runtime

## Goal
Implement the runtime that executes a declared module graph over the typed hook phases.

## Acceptance
- [ ] A declared graph executes in specified phase order with typed contexts; an invalid graph (missing dependency, conflict) fails at startup with a precise error.
- [ ] Capability advertisement (`Supported`/`Allow`) is derived from the graph, not hand-listed.

## Progress
- (not started)

## Notes
- Design: [extension-framework](../designs/extension-framework.md).
