---
id: RT-8
title: Express source-IP admission as reviewable config
pillar: Signalling
status: backlog
priority: 
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks, security]
note: admission is the whole security boundary when no user is authenticated
---

# Express source-IP admission as reviewable config

## Goal
Let the set of source addresses permitted to send traffic be expressed as reviewable, diffable configuration at production scale, attributed to the trunk or zone it belongs to.

## Acceptance
- [ ] Permitted sources are declared per trunk and per internal zone, not as rows in an opaque table.
- [ ] The scheme is workable at ~120 entries without becoming unreadable.
- [ ] An unknown source is handled by a declared policy (drop silently, or reject) — the choice is config.
- [ ] A failing-first test proves an unlisted source is refused and a listed one is admitted.
- [ ] Entries carry an attribution so an operator can tell which peer an address belongs to.

## Progress
- (not started)

## Notes
- In one deployment's estate admission is *entirely* IP-based — no user is ever authenticated — so this table is the whole security boundary. It holds ~117 carrier entries plus a small internal zone.
- Today it is rows in a database with a free-text tag, which nothing reviews or diffs.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-13**). The evidence sits in that deployment's own reference material.
