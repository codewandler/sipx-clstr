---
id: CX-2
title: Create the Cargo workspace
pillar: Platform
status: backlog
priority: 
design: 
epic: 
areas: [build]
note: first act of M1; updates the AGENTS.md gate
---

# Create the Cargo workspace

## Goal
Create the Rust workspace: crate skeleton, workspace lints (`unsafe_code = "forbid"`, no-panic warnings as errors in lib code), CI with the full gate, the provenance check adapted with the integration carve-out, and a pinned sipx dependency.

## Acceptance
- [ ] The gate runs green on the empty workspace: fmt, clippy `-D warnings`, tests, provenance, feature matrix.
- [ ] The provenance script enforces the AGENTS.md carve-out (integration targets allowed, SIP-stack prior art rejected).
- [ ] AGENTS.md's gate section is updated by this story to the command form.

## Progress
- (not started)

## Notes
- Blocked by the M0 specs — the first crates implement them.
