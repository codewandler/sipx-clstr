---
id: CF-6
title: Seed the conformance registry with the M1 profile
pillar: Platform
status: backlog
priority:
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness, extensions]
note: blocked by EX-2 — the extraction work CF-2's report needs
---

# Seed the conformance registry with the M1 profile

## Goal
Extract the normative requirements the M1 profile actually claims — RFC 3261 §16 (proxy) and §10
(registrar), with their updates — into the machine-readable registry, so CF-2's report generator
has real data and the coverage claims mean something.

## Acceptance
- [ ] Every normative requirement of §16 and §10 applicable to the M1 roles exists as a registry
entry with applicability (role, transport) and an initial status.
- [ ] Each implemented requirement links the tests that prove it; unimplemented ones carry an
honest status (not-applicable / profile-disabled / partial), never absence.
- [ ] CF-2's generated report over this data shows all four coverage kinds for the M1 profile
with no empty sections.

## Progress
- (not started)

## Notes
- Design: [conformance-harness](../designs/conformance-harness.md). Scoped deliberately to the
M1 profile — extraction for later RFCs lands with their extension modules.
