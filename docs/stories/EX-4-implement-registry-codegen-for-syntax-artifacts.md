---
id: EX-4
title: Implement registry codegen for syntax artifacts
pillar: Platform
status: backlog
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [extensions]
note: UPSTREAM decision per artifact
---

# Implement registry codegen for syntax artifacts

## Goal
Generate syntax artifacts (constants, header names, option tags, where practical parsers) from the registry instead of hand-writing them.

## Acceptance
- [ ] The M3 syntax set (Path, Outbound, GRUU, push, timers, 100rel) generates from registry data.
- [ ] For each artifact that belongs in the kernel, the upstream decision is recorded and the generated change lands as a sipx contribution.
- [ ] A syntax-only RFC demonstrably lands as a registry entry with no hand-written parser code.

## Progress
- (not started)

## Notes
- Design: [extension-framework](../designs/extension-framework.md).
