---
id: DX-12
title: Gate that every site page is reachable and every command shown is real
pillar: Foundation
status: backlog
priority:
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, ci]
note: supersedes the site half of CF-11 — after the split no spec is published, so the concern moved
---

# Gate that every site page is reachable and every command shown is real

## Goal

Make the two site properties that are currently held by hand into checks: every authored page is
reachable from the sidebar, and every command the site tells a reader to run still works.

## Acceptance

- [ ] A check fails when a file under `website/docs/` is not reachable from `website/sidebars.js`
      — proved by adding an unlisted page and watching it go red.
- [ ] A check fails when `sidebars.js` names a doc id that does not exist.
- [ ] The check runs in `scripts/gate.sh` and in the `docs` workflow.
- [ ] The CLI surface quoted in `reference/cli.md` is verified against the binary rather than
      trusted — at minimum, that every flag named on the page is accepted and every flag the
      binary accepts is named.

## Progress

- (running log)

## Notes

- `CF-11` ("gate that every published doc is reachable from the site", `ready`, priority 2) is
  the ancestor of this story. Its note says two **specs** are unreachable on the published site;
  after `DX-1` no spec is published at all, so that half of it is dissolved rather than fixed.
  Decide there whether `CF-11` is re-aimed at something else or closed as superseded — do not
  silently repurpose it from here.
- Docusaurus already fails the build on a broken link and on a broken markdown link; the gap is
  the *orphan* page, which builds fine and is reachable by nobody.
