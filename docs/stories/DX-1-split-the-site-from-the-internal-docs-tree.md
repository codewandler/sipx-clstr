---
id: DX-1
title: Split the published site from the internal docs tree
pillar: Foundation
status: done
priority: 1
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs, ci]
note: the old link gate would have gone green by no longer looking — inverted rather than dropped
---

# Split the published site from the internal docs tree

## Goal

Stop publishing `docs/` and give the site its own authored tree at `website/docs/`, so the public
documentation has one audience and internal contributor material stops dating the product page.

## Acceptance

- [x] `website/docusaurus.config.js` reads `path: 'docs'`, has no `exclude` key, and points
      `editUrl` at `tree/main/website/`.
- [x] The search theme indexes `docs` rather than `../docs`, so the search box returns results.
- [x] `onBrokenMarkdownLinks` is `throw`, not `warn`.
- [x] `scripts/check-docs.py` check 3 is inverted: a page under `website/docs/` that
      relative-links into `docs/` is a **failure**, naming the GitHub URL to use instead.
- [x] `docs/README.md` and `AGENTS.md` state the split from both sides.
- [x] Nothing under `docs/` is routable on the built site.

## Progress

- **Done.** Config, gate, and both working-agreement documents.
- The inversion is the substance. The old check read the `exclude:` globs out of the site config;
  with that key gone `site_excludes()` returns `[]` and `check_site_links()` returns `[]` with it
  — green because it stopped looking. `fnmatch` and `is_excluded()` were removed with it.
- Also fixed in passing: `crates/sipx-clstr-node/src/main.rs` `--help` claimed "No roles are
  implemented yet", stale since M1, and never documented `--tenant`. Nothing asserted the string.
- Considered for upstream: no. The docs arrangement is this repository's own.

## Notes

- `check-docs.py` module docstring rewritten — it opened by asserting `docs/` is published.
- The rule it replaces exists because the `v0.4.0` site deploy died on a link into an excluded
  page while the gate stayed green.
