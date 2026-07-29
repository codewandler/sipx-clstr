---
id: CF-11
title: Gate that every published doc is reachable from the site
pillar: Foundation
status: ready
priority: 2
epic: conformance-harness
areas: [ci, build, docs]
note: two specs are unreachable on the published site and nothing notices
---

# Gate that every published doc is reachable from the site

## Goal
Make "reachable from the site" a checked property. `AGENTS.md` says anything under `docs/` that
should be readable on the site is reachable from `website/sidebars.js`, and the site build fails on
a broken link rather than shipping one — but **nothing checks that a page exists in the sidebar at
all**, so a spec can be published to `docs/`, pass every gate, and simply not appear.

## Acceptance
- [ ] A gate step fails when a file under `docs/specs/` or `docs/designs/` is absent from
      `website/sidebars.js`, or records the omission as deliberate in one obvious place.
- [ ] `docs/specs/registrar-auth.md` is either listed or its omission is recorded. It is currently
      unreachable on the site, and it is not a minor page: it is the digest-authentication contract,
      including §7's statement of what digest does and does not protect.
- [ ] The check reads the sidebar and the exclude list from the site config rather than keeping its
      own copy, the way `check-docs.py` already reads `docusaurus.config.js`'s `exclude` globs.
- [ ] Adding a spec without listing it fails the gate, proved by trying it.

## Progress
- (not started)

## Notes
- Found during wave 4. `DP-1` added `docs/specs/cluster-config.md` and correctly reported that it
  had not touched the sidebar, since that file was outside its write set — and noted the precedent
  that `specs/registrar-auth` is already unlisted. The coordinator listed `cluster-config` at
  integration; `registrar-auth` remains unlisted and is the evidence that this is not caught.
- This is the same failure mode `check-docs.py` exists for, one step earlier. That script was
  written because a release deploy failed on a link that resolved on disk and 404'd on the site.
  A page absent from the sidebar does not 404 — it is simply never linked, which is quieter and
  therefore worse.
- Related but distinct from [CF-10](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/CF-10-check-docs-must-only-see-the-repository.md),
  which is about the *set of files* the docs check reads. This one is about a property it does not
  check at all. Doing them together is reasonable; doing neither because each looks small is how
  both persist.
