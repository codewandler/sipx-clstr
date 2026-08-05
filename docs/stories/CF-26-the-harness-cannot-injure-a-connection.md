---
id: CF-26
title: The harness cannot injure a connection, so three owner-RPC scenarios are unwritable
pillar: Foundation
status: in-progress
priority: 2
epic: conformance-harness
areas: [harness, sim, affinity]
note: fault.rs offers five faults, none of them reconnect, restart-with-fresh-incarnation or backpressure — which owner-rpc §10 requires
---

# The harness cannot injure a connection, so three owner-RPC scenarios are unwritable

## Goal

Give the deterministic harness the connection-level faults the owner-RPC failure taxonomy is
specified against, so `AF-7` can execute §10's scenarios rather than restate them.

## Acceptance

- [x] The simulation can drop and re-establish a client connection, so a flow reference outlives the
      connection it named and `owner-rpc` §10's reconnect scenario is executable.
- [x] The simulation can restart a node with a **fresh incarnation**, so a reference minted against
      the previous incarnation resolves as specified rather than as a stale-but-plausible hit.
- [x] The simulation can model a **non-reading peer** — a connection accepted but not drained — so
      `owner-rpc` §8's `max_pending_per_flow` bound and the `FlowRejected` answer are reachable, and
      `T_write` can actually expire.
- [x] Each new fault is deterministic under the harness's own PRNG: same seed, same schedule, same
      outcome, with a pinned vector. The sim owns its randomness; a fault must not read the clock or
      the OS entropy pool.
- [x] **Failing-first:** one of the three `owner-rpc` §10 scenarios is written and shown unwritable
      (or vacuous) before the fault exists, then passing after.
- [ ] `scripts/gate.sh` is green.

## Progress

**Done (`impl/CF-26`).** Four new `Fault` variants — `Reconnect`, `RestartNode`, `StopReading`,
`ResumeReading` — with four new `Input` variants for what a node learns from them (`Connected`,
`Restarted { incarnation }`, `WriteStalled`, `WriteFlushed`), and the scheduler machinery behind
them in `sim.rs`: a per-directed-pair connection counter stamped on every message, so a write to a
connection that has been replaced is lost rather than delivered to its successor; a per-node restart
counter that is the incarnation; and a per-node held-write buffer for a peer that is accepted and
not draining. Two accessors for scenarios: `Sim::incarnation` and `Sim::held_writes`. No new trace
vocabulary — the faults render through the existing `** FAULT` line, a restart additionally as
`started`, a lost connection as `Broken` — so `viz.rs`, its wire vocabulary and the cluster-viz
frame table are untouched.

`crates/sipx-clstr-sim/tests/connection_faults.rs` executes four of `owner-rpc` §10's rows against a
small stand-in for the owner's connection table, plus a byte-for-byte replay test and a pinned
rendered trace covering all four variants. The §10 rows themselves stay `AF-7`'s to name: the tests
here are deliberately named differently, so a report cannot read them as `AF-7` having landed.

**Two things for the integrator.**

1. **`scripts/gate.sh` is red at the merge base (`43594b1`), not from this diff.**
   `cargo fmt --all --check` and `cargo clippy` both fail on
   `crates/sipx-clstr-node/tests/devspace_dialable.rs` — unformatted at `:34` and `:118`, missing
   doc backticks at `:79`, `manual_pattern_char_comparison` at `:94` — the file the `KO-18`/`DX-13`
   WIP rescue (`8c61cf4`) brought in. Proved by stashing this branch's work and running both
   commands at the base. Every other gate step is green with this diff applied: workspace tests,
   `check-features`, `check-msrv`, `check-provenance`, `check-vectors`, `check-crd-drift`,
   `check-docs`, `check-proof-domains`, `check-site`. Left unfixed deliberately — it is another
   story's file, and another story may be inside it right now.
2. **`owner-rpc` is an `EXCLUDED` entry in `CF-25`'s spec registry and this story does not change
   that.** It should stay excluded until `AF-7` lands: §10 registers no vector prefix on purpose,
   and what changed here is that its scenarios are now *writable*, not that they are executed
   against an implementation. The registration belongs in the commit that adds `AF-7`'s rows and
   the tests that prove them, which is this repository's own rule.

## Notes

- **Found by the independent review of `AF-3`'s diff, 2026-07-31.**
  `crates/sipx-clstr-sim/src/fault.rs:33-92` offers exactly `KillNode`, `Partition`, `Heal`,
  `SetLinkPolicy` and `TimerSkew`. `docs/specs/owner-rpc.md:246`, `:247`, `:249` and `:250` require
  client reconnect, restart with a fresh incarnation, and non-reading-peer backpressure. None exists.
- **What this does and does not undercut.** `AF-3`'s transport argument — that the harness executes a
  SIP-over-TLS peer hop as it stands, because a simulated node already exchanges `sipx_sip::Message`
  (`crates/sipx-clstr-sim/src/node.rs:12,36-98`) — **holds**. What is over-claimed is only "the
  deterministic harness executes it as it stands" for those three specific scenarios.
- **Why this is its own story rather than `AF-7`'s.** These are harness capabilities, not connection
  ownership: `AF-7` implements the owner RPC and should not also be extending the fault injector,
  and any future story about connection lifetime wants the same three faults. `AF-7` is the first
  consumer and is blocked on this for three of its thirteen named scenarios — not for the rest.
- Relates to `CF-4` (fault injection in the simulation), which established the fault set this extends.
- Considered for upstream: **no.** The fault injector is this repository's harness. If a fault needs a
  kernel-side seam to be injectable at all, that seam is a sipx story — record it in
  [upstream.md](../upstream.md) rather than shadow-implementing it here.
