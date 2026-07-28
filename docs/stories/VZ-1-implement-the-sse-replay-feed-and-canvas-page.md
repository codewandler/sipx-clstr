---
id: VZ-1
title: Implement the SSE replay feed and canvas page
pillar: Platform
status: done
priority:
design: docs/designs/cluster-viz.md
epic: cluster-viz
areas: [harness]
note: the constellation's first feed — a paced sim replay over SSE, one page, zero new runtime deps
---

# Implement the SSE replay feed and canvas page

## Goal
Deliver the design's feed 1: a dev adapter that paces a seeded scenario against the wall clock
and streams its trace live over SSE, plus the canvas page that renders the stage, the particles,
the fault visuals and the invariant HUD. The stream is the trace and nothing else.

## Acceptance
- [x] The SSE adapter (a `sipx-clstr-sim` cargo example) streams a seeded scenario live:
  `register_then_call`'s topology — the real registrar and forwarding core in the edge, a call
  that forks to two of bob's devices — paced against the wall clock at a selectable ratio, with
  bounded fast-forward through silence.
- [x] The canvas renders the fixed stage (roles from the `meta` frame, never name-sniffed),
  particles colored by method/status, fault visuals (drop, duplication, break, malformed, timers)
  and the DP-3 invariant HUD — uninstrumented counters shown as *uninstrumented*, never as zero.
- [x] `curl -N http://127.0.0.1:8975/events` shows the documented frame stream: `meta`, `tick`,
  `invariant`, and one frame per trace entry with `id:` set to the entry's `seq`.
- [x] The totality test covers every trace variant: `viz::Frame::from_entry` is an exhaustive
  match in the harness library, so a new `Event` variant fails the build rather than rendering
  as nothing.
- [x] Proved frame-for-frame equal to the trace: the frames are *derived from* the retained
  trace entries (one `Frame::from_entry` per `Entry`, tested in `src/viz.rs`), and the pacing
  loop's cursor walks `Trace::entries()` — there is no second event model to diverge. Replayed
  at `--speed 1` and `--speed 8`, same seed, same stream.

## Progress
- `crates/sipx-clstr-sim/src/viz.rs` — the wire vocabulary in the library, where the gate can
  test it: `Frame`/`FrameKind` (one kind per `Event` variant, exhaustive match), `Role`/
  `NodeMeta`/`LinkMeta` for the `meta` frame, and `invariants()` — the sim feed's DP-3 snapshot
  as trace queries, `None` for every counter with no instrumented source. Six unit tests,
  including the totality test and the honesty-rule test (uninstrumented is absent, not zero).
- `crates/sipx-clstr-sim/examples/viz/` — the adapter, std-only HTTP+SSE (no new runtime deps;
  `serde_json` is a dev-dependency for frame encoding, the library stays encoding-free):
  - `main.rs` — flags (`--seed`, `--speed`, `--links clean|jittery|storm`, `--port`), the pacing
    loop (25 ms virtual slices, 8× through silence), the hub (full backlog retained; a slow
    client is dropped into a reconnect-and-catch-up rather than fed late frames), and the three
    routes (`/`, `/events`, `/healthz`).
  - `scenario.rs` — the `register_then_call` port: edge (real registrar + real forwarding core),
    alice calling bob's two registered devices, so the fork and the losing branch's cancel are on
    screen. Returns the `Sim` plus the `meta` stage description.
  - `page.html` — the canvas: fixed layout from `meta` roles (edges centre, endpoints at the
    wings), particles per the design's visual grammar, timer rings, a note log, and the invariant
    HUD. Reconnect re-renders from the replayed `meta`, so resync is a reset, not a patch.
- Verified: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features --
  -D warnings`, `cargo test --workspace --all-features` (the example is an `--all-targets`
  target, so clippy covers it; `viz.rs` tests run in the harness suite).

## Notes
- Design: [cluster-viz](../designs/cluster-viz.md). Two of its open questions answered here:
  role hints are **explicit from the scenario builder** (name inference rejected as brittle), and
  the design's "torture scenario" proof is the same `register-call` topology under `--links
  storm` weather — the scenario code is not what a torture run varies.
- The smoke-test stub nodes were not ported; the showcase is `RG-6`'s real stack, because the
  point of the page is that everything on it is real behavior.
- `VZ-2` (interactive faults), `VZ-3` (the real-cluster feed, blocked on DP-3/ET-5/DP-7) and
  `VZ-4` (load-mode aggregation) are scoped in the design and filed when scheduled.
