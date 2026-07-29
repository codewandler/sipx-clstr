---
id: DX-6
title: Write the conformance and what's-new pages
pillar: Foundation
status: ready
priority: 2
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: publish the methodology and the legend, not a second copy of a generated table
---

# Write the conformance and what's-new pages

## Goal

Give `website/docs/reference/conformance.md` and `website/docs/whats-new.md` their content: how
correctness is measured here, and where the project stands release by release.

## Acceptance

- [ ] `reference/conformance.md` explains the method: every normative rule carries a numbered
      vector; a vector is proved by a test whose name encodes the row, or deferred in
      `docs/reference/vector-scope.toml` **with a reason and a story ID**; `check-vectors.py`
      regenerates the report and fails the gate if the committed copy is stale.
- [ ] It states that a deferred row that *is* covered also fails — the report is a measurement,
      not a claim.
- [ ] It links the generated table by absolute GitHub URL and does **not** duplicate it, so the
      numbers cannot drift.
- [ ] Any count quoted in prose is read from `docs/reference/conformance.md` at the time of
      writing, not remembered.
- [ ] `whats-new.md` gives the current state and the recent releases, each bullet leading with
      what changed *for a user*, deferring detail to `CHANGELOG.md` on GitHub.
- [ ] `whats-new.md` names what is still missing, not only what landed.

## Progress

- (running log)

## Notes

- Source of truth for the numbers: `docs/reference/conformance.md` (generated) and `CHANGELOG.md`.
- The current release is 0.9.0; check the CHANGELOG rather than assuming.
- Prefer no number to a stale number — a count in prose is the thing most likely to rot.
