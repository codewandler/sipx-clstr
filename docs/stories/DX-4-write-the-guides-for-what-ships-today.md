---
id: DX-4
title: Write the guides for what ships today
pillar: Foundation
status: done
priority: 4
design: docs/designs/docs-site.md
epic: docs-site
areas: [docs]
note: bind-vs-advertise gets its own page — it is the failure every first-time operator hits
---

# Write the guides for what ships today

## Goal

Document the five things a reader can actually do: qualify the project, run a node, register and
call, get addressing right, and run it in a container.

## Acceptance

- [x] `guides/does-this-fit.md` states the non-goals, including that call-survival HA is not a v1
      guarantee.
- [x] `guides/run-a-node.md` gives the flags, the two stdout lines, the exit codes and `RUST_LOG`.
- [x] `guides/registrations-and-calls.md` explains why the end-to-end script takes ~32 s to drain.
- [x] `guides/addressing.md` quotes the real refusal message for an unspecified advertise address.
- [x] `guides/docker-and-k3d.md` explains why the image defaults to `--help` rather than `run`.

## Progress

- **Done.** Five guides.
- The addressing refusal message was written from memory first and then corrected against
  `crates/sipx-clstr-node/src/listen.rs` — it is "`{addr}` cannot be advertised: an unspecified
  address is where to listen, not where to be reached", not a paraphrase.
- Considered for upstream: no.

## Notes

- The ~32 s drain is RFC 3261's 64·T1 absorption window; asserting immediate drain would assert a
  bug.
