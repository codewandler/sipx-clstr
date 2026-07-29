---
id: DX-2
title: Lay down the site's information architecture, navigation and landing page
pillar: Foundation
status: done
priority: 2
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: URLs are stable from the start — stub pages beat stub URLs that move later
---

# Lay down the site's information architecture, navigation and landing page

## Goal

Establish the full ladder — what it is, first call, guides, clustering, operate, migrate,
reference — so every route exists and no URL has to move when unshipped sections fill in.

## Acceptance

- [x] `website/sidebars.js` carries the whole ladder, manually ordered, with unshipped sections
      labelled `(preview)`.
- [x] Navbar and footer no longer point at `/docs/vision`, `/docs/roadmap` or any spec route.
- [x] The landing page no longer claims "nothing forwards a SIP message yet".
- [x] Every route named in the sidebar resolves.

## Progress

- **Done.** Sidebar, navbar, footer, landing page.
- Status is carried in the category label (`Clustering (preview)`) so a reader knows before
  opening a page, and restated in the page's own words after.
- Considered for upstream: no.

## Notes

- Ordering lives in `sidebars.js` rather than in per-page `sidebar_position`, so the shape of the
  documentation is readable in one file.
