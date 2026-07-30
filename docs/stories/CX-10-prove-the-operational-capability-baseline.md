---
id: CX-10
title: Prove every M4 operational capability scenario
pillar: Platform
status: backlog
priority: 21
design: docs/specs/operational-capability-baseline.md
epic: conformance-harness
areas: [conformance, interop, deploy, load]
note: blocked by the complete M4 story set and CX-9
---

# Prove every M4 operational capability scenario

## Goal

Execute the milestone against the immutable release candidates and archive evidence another operator
can reproduce.

## Acceptance

- [ ] One bounded runner executes `OB-1` … `OB-12` and fails on a missing, skipped or ambiguous
      scenario.
- [ ] The runner records configuration, seeds, revisions and artifact digests and uses the real
      network/three-zone path where the row claims it.
- [ ] Independent peers are distinguished from same-kernel integration tests in the report.
- [ ] Node-kill and overload cases arrange process-group cleanup and wait for it before verdict.
- [ ] The generated report is linked from the public capability page and cannot be edited by hand.

## Progress

- Not started.
