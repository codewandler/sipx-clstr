---
id: CX-8
title: Track M4 as the operational capability baseline
pillar: Platform
status: in-progress
priority: 1
design: docs/specs/operational-capability-baseline.md
epic:
areas: [release, upstream, conformance]
note: M4 tracker — remains open until every local story, released upstream dependency, proof and artifact is complete
---

# Track M4 as the operational capability baseline

## Goal

Keep one canonical, auditable answer to whether the released endpoint-and-platform system satisfies
the operational capability bar.

## Acceptance

- [ ] `operational-capability-baseline.md` remains the canonical requirement and proof set; roadmap
      prose links to it instead of restating a weaker gate.
- [ ] Every story in §4 is `done`, and every newly discovered defect that falsifies an `OB-*` row is
      filed and linked before this tracker closes.
- [ ] Every required kernel row is `landed` in the pinned release through `CX-9`; no local patch or
      unreleased branch satisfies it.
- [ ] `CX-10` records passing `OB-1` … `OB-12` against immutable candidate artifacts.
- [ ] `CX-11` publishes those exact artifacts and the public capability/HA documents.
- [ ] Both repository gates are green at the release commits.

## Progress

- Milestone and story set filed; implementation not started.

## Notes

- This story coordinates status only. It owns no protocol or runtime implementation.
