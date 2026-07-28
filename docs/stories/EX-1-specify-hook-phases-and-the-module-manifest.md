---
id: EX-1
title: Specify hook phases and the module manifest
pillar: Platform
status: ready
priority: 5
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: must land before the proxy API hardens
---

# Specify hook phases and the module manifest

## Goal
Write `docs/specs/hook-framework.md`: the typed hook phases and the module manifest that make extensions declared modules instead of core edits.

## Acceptance
- [ ] The ordered phase list is specified, each phase with its typed context and permitted effects.
- [ ] The module manifest schema covers hooks used, dependencies, conflicts, methods/headers/option tags consumed and advertised, state needs, and timers.
- [ ] Startup graph validation rules (ordering, conflict detection, capability advertisement for `Supported`/`Allow`) are specified, with at least one deliberately invalid module set as a vector.
- [ ] Phase boundaries are reviewed against PX-1 so the hook spec and the proxy spec name the same pipeline.

## Progress
- (not started)

## Notes
- Design: [extension-framework](../designs/extension-framework.md).
