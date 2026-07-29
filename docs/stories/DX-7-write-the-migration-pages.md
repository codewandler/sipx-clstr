---
id: DX-7
title: Write the migration pages for people arriving from an existing deployment
pillar: Foundation
status: ready
priority: 3
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: naming a migration target is permitted; citing its behaviour as rationale is not
---

# Write the migration pages for people arriving from an existing deployment

## Goal

Give `website/docs/migrate/from-kamailio.md` and `website/docs/migrate/from-asterisk.md` their
content: honest concept maps for the two places readers actually arrive from.

## Acceptance

- [ ] Both pages open with the blunt one-line answer before any table.
- [ ] Both carry a "Maps today / not yet" table — `In your deployment` → `Goes to` → `Status` —
      using the site's closed status vocabulary (`today` · `today, partly` ·
      `specified, not shipped` · `designed` · `not planned`).
- [ ] `from-kamailio.md` is honest that the floor today is one node with no clustering and no
      trunks, and makes the actual architectural argument: the cluster carries no shared call
      state; what routes the next request rides in the message.
- [ ] `from-asterisk.md` leads with the mismatch — this is proxy-first and **never terminates
      dialogs**; queues, IVR and conference are not in the core and are not scheduled.
- [ ] Both end with "What does not carry over".
- [ ] `scripts/check-provenance.sh` passes on the finished pages.

## Progress

- (running log)

## Notes

- **The provenance rule still binds the prose.** AGENTS.md non-negotiable #1 bans prior art as
  *rationale* — "system X does it this way, so we do too" is never an argument here. Naming a
  system you are migrating *from* is the permitted case. Map concepts and state facts; cite RFCs
  or `docs/specs/` for any reason-why.
- Neither name is on the denylist, so no `scripts/provenance-allow.txt` entry is needed. Run the
  check anyway before calling this done.
- Do not claim feature parity anywhere. The comparison that survives contact is architectural.
