---
id: DX-7
title: Write the migration pages for people arriving from an existing deployment
pillar: Foundation
status: done
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

- [x] Both pages open with the blunt one-line answer before any table.
- [x] Both carry a "Maps today / not yet" table — `In your deployment` → `Goes to` → `Status` —
      using the site's closed status vocabulary (`today` · `today, partly` ·
      `specified, not shipped` · `designed` · `not planned`).
- [x] `from-kamailio.md` is honest that the floor today is one node with no clustering and no
      trunks, and makes the actual architectural argument: the cluster carries no shared call
      state; what routes the next request rides in the message.
- [x] `from-asterisk.md` leads with the mismatch — this is proxy-first and **never terminates
      dialogs**; queues, IVR and conference are not in the core and are not scheduled.
- [x] Both end with "What does not carry over".
- [x] `scripts/check-provenance.sh` passes on the finished pages.

## Progress

- Both pages authored. `from-kamailio.md` opens on "the role is the same, the floor is one node",
  states the floor as five bullets (one node, no trunks, open registrar, in-memory bindings,
  UDP/TCP only) before the map, carries a 15-row status table, and makes the architectural
  argument from RFC 3261 §16.6 step 4, §12.1.1/§12.1.2 and §12.2.1.2 plus RFC 5626 §5.2, citing
  `docs/specs/affinity-token.md` and `docs/vision.md` by GitHub URL.
- `from-asterisk.md` leads with the mismatch, explains what "never terminates a dialog" rules out
  (two dialogs, two offer/answer machines, `CSeq` translation, leg correlation) and that the
  B2BUA design record is a deferred placeholder with no stories, then an 11-row status table.
- **No feature-parity claim on either page** — both route the reader to the conformance report,
  which is allowed to answer "no".
- Provenance: neither migration target is on the denylist, so no `provenance-allow.txt` entry was
  needed. Both names appear only as the system being migrated *from*; no prose cites another
  system's behaviour as rationale. `scripts/check-provenance.sh` → `clean (7 terms checked)`.
- Gate run in the worktree: `check-docs.py` → `docs: clean (166 markdown files checked)`;
  `check-provenance.sh` → clean; `npm run build` → `[SUCCESS] Generated static files in "build"`.
  `scripts/gate.sh` deliberately not run — no Rust changed.
- The acceptance contract was checked mechanically by a scratch script (17 failures before, clean
  after) rather than a committed test: this story's write set is the two pages, and adding a
  checker to `scripts/` would have been outside it. `DX-9` is the story that gates site content.
- **Considered for upstream: no.** These are product documentation pages for this platform's
  published site; there is nothing protocol-generic here for the sipx kernel to own.

## Notes

- **The provenance rule still binds the prose.** AGENTS.md non-negotiable #1 bans prior art as
  *rationale* — "system X does it this way, so we do too" is never an argument here. Naming a
  system you are migrating *from* is the permitted case. Map concepts and state facts; cite RFCs
  or `docs/specs/` for any reason-why.
- Neither name is on the denylist, so no `scripts/provenance-allow.txt` entry is needed. Run the
  check anyway before calling this done.
- Do not claim feature parity anywhere. The comparison that survives contact is architectural.
