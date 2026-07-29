---
id: PX-3
title: Header surgery API in sipx
pillar: Signalling
status: ready
priority: 2
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: S-15 landed in sipx v0.4.0 — what remains is adopting it here; unblocks PX-4
---

# Header surgery API in sipx

## Goal
Land `remove_first`/`insert_at`/`retain` (or equivalent) on sipx's `Headers` so Via pop/push and Record-Route insertion need no private collection rebuild.

## Acceptance
- [x] The sipx story is filed (CX-1) and landed; sipx-clstr pins a kernel version exposing the API. — `S-15` is in `v0.4.0` as `Headers::remove_first` and `Headers::retain` (`sipx-sip/src/message.rs:387,408`), and the workspace pins that tag.
- [ ] The proxy's Via and Record-Route mutations use the upstream API exclusively.

## Progress
- **The upstream half is done and the local half is not.** `v0.4.0` exposes the API, so what is
  left is this repo's: replace the private collection rebuilds with `remove_first`/`retain`.
- `CF-5` and `PX-5` each left a rebuild behind with a comment saying so —
  `PX-5`'s note calls its own the "third site". Those comments are the work list.

## Notes
- Upstream ledger: [upstream.md](../upstream.md). Blocks PX-4.
