---
id: DX-15
title: Sweep both doc trees for claims the beta-line kernel bump invalidated
pillar: Foundation
status: ready
priority: 2
design:
epic:
areas: [docs]
note: CX-12 fixed the claims the gate can see; this sweeps the ones it cannot — "at the pinned v0.10.0" citations that are still true but now name the wrong pin, and prose anchored to a kernel state twelve releases old
---

# Sweep both doc trees for claims the beta-line kernel bump invalidated

## Goal

`CX-12` moved the kernel pin across twelve releases. The gate caught every claim it has a checker
for (site version banners, documented commands); this story owns the ones it does not: prose and
citations that say **"at the pinned `v0.10.0`"** about facts that are still true at
`v1.0.0-beta.4` but now cite a pin the workspace no longer holds. A citation that names the wrong
pin is the "true number attached to a false reason" failure `CX-4` documented — the next reader
re-verifies against the wrong tag.

## Acceptance

The inventory, taken by `CX-12` (`grep -rl '0\.10\.0'` minus `node_modules`, build output,
lockfiles and closed-story records, which are historical and stay):

- [ ] `docs/specs/registrar-auth.md` §"replay window" — "`challenge.rs:194,388` at the pinned
      `v0.10.0`" re-cited at `v1.0.0-beta.4` (the file is one blob, so the numbers hold; the pin
      name must move).
- [ ] `docs/reference/vector-scope.toml` (`RA-R-8` deferral text) — same "at the pinned
      `v0.10.0`" phrase; fix there and regenerate `docs/reference/conformance.md` from it rather
      than editing the generated file (`CF-21`'s rule: hold counts and text to their generator).
- [ ] `docs/stories/RG-20-…` (ready) — note and acceptance say "consumes the pinned sipx
      `v0.10.0` `Expires`"; re-point at the pinned kernel by role ("the pinned kernel's fallible
      `Expires`, present since `v0.10.0`") so the story does not go stale at every bump, and
      regenerate the board (the note renders there).
- [ ] `docs/stories/AF-5-…` (in-progress) — the `name.rs:109` citation pinned to `v0.10.0`;
      re-read at `v1.0.0-beta.4` (`Privacy` and `History-Info` entered that table in the range,
      so the line number likely moved).
- [ ] `docs/stories/CX-5-…` (in-progress, deliberately open) — its byte-identity claim reads
      "`v0.7.0` → `v0.10.0` → kernel `main`"; extend to `v1.0.0-beta.4` per the ledger's
      re-check, so the story and the ledger tell one story.
- [ ] `website/docs/architecture.md` / guides — verify no page still describes kernel behaviour
      the beta line changed: the stricter response builder (a proxy answering malformed traffic
      now refuses instead of inventing), overload control live in the transport, and QUIC
      existing as a kernel transport this platform refuses at the listener set.
- [ ] `docs/architecture.md` — same pass for the internal chart prose.
- [ ] A final `grep -r '0\.10\.0'` over `docs/` and `website/docs/` shows only historical
      records: closed stories, review records, `whats-new.md` release entries, and the ledger's
      own row histories.

## Progress

- Filed by `CX-12`, which already fixed the gate-visible half: site banners and the
  getting-started clone tag (checked by `check-site.py`), `README.md`'s "Built on" row,
  `AGENTS.md`'s state of play, the roadmap status, the `Cargo.toml` narrative comments, and the
  ledger itself.

## Notes

- `Dockerfile`'s MSRV comment and `scripts/check-site.py`'s fixture strings were inspected and
  deliberately left: the first is historical narrative that stays true, the second is checker
  self-test data, not a claim.
- The sweep is bounded by the inventory above on purpose; if new stale claims surface while
  fixing these, extend the list here rather than fixing silently, so the next bump inherits the
  checklist shape.
