---
id: DX-3
title: Write the entry pair — what sipx-clstr is, and getting started
pillar: Foundation
status: done
priority: 3
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: the two facts that bite — open registrar, in-memory bindings — are stated on the first page
---

# Write the entry pair — what sipx-clstr is, and getting started

## Goal

Give a stranger the two pages that decide whether they stay: what this is with an honest
capability matrix, and a first forwarded call in about five minutes.

## Acceptance

- [x] `intro.md` carries the capability matrix, "The honest version", and the public-vs-project
      docs split.
- [x] The open-registrar and in-memory-bindings facts appear above the fold, not in a footnote.
- [x] `getting-started.md` reaches a forwarded call using only Rust and standard-library Python.
- [x] Every command on both pages has been run.

## Progress

- **Done.** `intro.md` (`slug: /`) and `getting-started.md`.
- Rung 1 is `scripts/sip_demo.py`, not `e2e-call.sh` — it needs no second toolchain and narrates
  its own steps. The audio path is offered second, with the `sipx` CLI prerequisite stated.
- Considered for upstream: no.

## Notes

- The kernel version in the sample output is the pinned tag, not a branch.
