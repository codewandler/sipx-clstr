---
id: PX-3
title: Adopt the upstream header surgery API
pillar: Signalling
status: done
priority:
design: docs/designs/proxy-engine.md
epic: proxy-engine
areas: [proxy]
note: S-15 landed in sipx v0.4.0 — what remains is adopting it here; unblocks PX-4
---

# Adopt the upstream header surgery API

## Goal
Use sipx's `Headers::remove_first`/`retain` for Via pop/push and Record-Route insertion, so no site in this repo rebuilds the header collection privately to edit one header.

The story was filed as *"Header surgery API in sipx"* when the API did not exist and getting it upstream was the whole job. `S-15` shipped in `v0.4.0`, so the title now names work that is done; what is left is adoption here.

## Acceptance
- [x] The sipx story is filed (CX-1) and landed; sipx-clstr pins a kernel version exposing the API. — `S-15` is in `v0.4.0` as `Headers::remove_first` and `Headers::retain` (`sipx-sip/src/message.rs:387,408`), and the workspace pins that tag.
- [x] The proxy's Via and Record-Route mutations use the upstream API exclusively. — `pop_via` (`context.rs:693`) is one `Headers::remove_first` call; `tests/header_surgery.rs` proves both the behaviour and the exclusivity.

## Progress
**Done 2026-07-29.** Two sites, both the same shape, both gone.

- `sipx-clstr-proxy::context::pop_via` and the `smoke.rs` stub's `pop_top_via` each built a fresh
  `Headers` and copied every header except the first `Via` into it. Both are now
  `headers.remove_first(&HeaderName::Via)`. The rebuild was O(n) clones per hop and correct; the
  call is O(n) memmove and states the intent.
- The `remove_all` uses in `forward.rs` (strict-route rewrite, `set_header`) are **not** S-15 sites
  and were left alone: they mean "every occurrence", which is what `remove_all` is for. Adoption
  ends where the semantics differ.
- `tests/header_surgery.rs` is new. The existing `PB-R` rows only assert that our own `Via` is
  gone, which is equally true of a response whose remaining stack came back shuffled — so the
  tests use a **three-Via** stack and assert the survivors keep their arrival order, plus that
  headers either side of the stack are untouched.
- The third test is the failing-first one: it reads the crate's source for `Headers::new()`, the
  shape of the pre-S-15 rebuild, and fails on the pre-change tree (one occurrence in `context.rs`).
  Behaviour alone cannot prove "exclusively" — a reintroduced rebuild would pass every other
  assertion in the file.

## Notes
- Upstream ledger: [upstream.md](../upstream.md). Blocks PX-4.
