---
id: CX-9
title: Pin the qualifying released kernel and clear the M4 upstream ledger
pillar: Platform
status: backlog
priority: 20
design: docs/specs/operational-capability-baseline.md
epic:
areas: [upstream, release]
note: blocked by every M4 kernel story and a tagged release carrying them
---

# Pin the qualifying released kernel and clear the M4 upstream ledger

## Goal

Consume the released kernel capability set M4 proves, with every upstream claim re-read against the
tag actually pinned.

## Acceptance

- [ ] The pin is an immutable released tag/commit containing every dependency named in the baseline
      and all earlier open ledger defects.
- [ ] Each `docs/upstream.md` row is re-read against that tag and moves to `landed` only with exact
      source/API evidence and a local consuming story.
- [ ] No `[patch]`, path dependency or kernel-main checkout enters the release proof.
- [ ] Default, no-default and all-feature builds use the same pin and both repositories' gates pass.
- [ ] A generated dependency inventory records the kernel commit in the node image and release
      evidence.

## Progress

- Not started.
