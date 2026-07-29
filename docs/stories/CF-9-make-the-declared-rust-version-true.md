---
id: CF-9
title: Make the declared rust-version true, and gate on it
pillar: Foundation
status: in-progress
priority: 1
epic: conformance-harness
areas: [ci, build]
note: found building the container image — the workspace does not build on its own declared floor
---

# Make the declared rust-version true, and gate on it

## Goal
`Cargo.toml` declares `rust-version = "1.88"` with the comment *"Matches the kernel's floor"*. The
workspace does not build on 1.88. Either the floor is wrong and should be raised to what actually
works, or it is right and something below it must change — but a declared MSRV that nobody checks is
a promise made to every consumer and kept for none.

## Acceptance
- [x] The declared `rust-version` is one the workspace actually builds on, established by building
      on it rather than by reasoning about it.
- [x] The gate builds on the declared floor, so the two cannot drift apart again silently. Today
      `scripts/gate.sh` uses whatever toolchain the developer has, which on a current machine is
      several releases newer than the claim.
- [x] If the floor is raised, the reason is recorded where the current comment is — the comment
      asserts a relationship to the kernel's floor that should be re-checked rather than copied.

## Progress

**The floor is 1.94**, established by bisecting 1.88 → 1.97 with
`cargo check --workspace --all-features --locked` against the pinned kernel tag `v0.7.0`:

| rustc | result |
|---|---|
| 1.88.0 | fails — `E0277`, `sipx-transport` |
| 1.92 | fails — same |
| 1.93.1 | fails — same |
| **1.94.1** | **passes** |
| 1.95.0 | passes |
| 1.97.0 | passes |

1.89–1.91 were not probed: 1.92 and 1.93 both fail and the boundary 1.93 → 1.94 is directly
bracketed, so nothing below 1.93 can change the answer. The two lowest probes ran in
`rust:1.8x-bookworm`/`rust:1.92-bookworm` containers, the rest on local rustup toolchains; both
routes produce the identical error. `--all-targets` (dev-dependencies and the test targets) also
passes on 1.94, so the floor holds for the whole gate surface rather than for `cargo build` alone.

**The drift check is `scripts/check-msrv.sh`, and it runs in both places.** It reads `rust-version`
out of Cargo.toml — the number is never duplicated — installs nothing, and runs
`cargo +<floor> check --workspace --all-targets --all-features --locked` in its own target
directory (`target/msrv`), so it does not invalidate the main one and force a full rebuild on the
next `cargo test`.

- `scripts/gate.sh` runs it as a step. This is the working agreement's own rule: green locally and
  red in CI is a bug in `gate.sh`. The declared floor is the one manifest claim a current-stable
  machine structurally cannot falsify, so leaving it to CI alone guarantees it is found after the
  push. Cost was the deciding worry and it is small: it is a `check`, not a build, and on a warm
  `target/msrv` a run is **7.98s**. Only the first run pays a cold check of the dependency graph.
- `.github/workflows/ci.yml` gains an `msrv` job that installs the floor from the manifest and runs
  the same script, in parallel with `gate`. That is what makes the check unconditional: a developer
  without the toolchain gets an actionable failure locally (`SIPX_SKIP_MSRV=1` is the documented,
  deliberate escape hatch), and CI never sets that variable.
- The script clears `RUSTFLAGS` on purpose. CI sets `-D warnings` workflow-wide, and an older rustc
  has an older lint set; failing the floor on a lint that merely differs between two releases would
  make the check brittle about something other than the promise it exists to keep.

**`Dockerfile`'s `ARG RUST_VERSION` is back on the declared floor (1.94)**, which `KO-13` said would
happen once this settled. Building the image on the floor rather than on stable is kept
deliberately — it is what caught the original defect.

**Considered for upstream: yes, and it belongs there.** The constraint is not ours. sipx-transport's
`impl<K, I> Default for TimerQueue<K, I>` (`crates/sipx-transport/src/timers.rs:63`) calls
`BinaryHeap::new()` without the bounds the older `BinaryHeap::<T>::new` required, so *the kernel's*
floor is 1.94 and ours can only follow. Bounding that impl (`K: Eq, I: Ord`) upstream would let both
projects drop back down. `docs/upstream.md` is fenced for this story, so this wants a row added by
whoever owns that ledger rather than by this branch.

**User-visible, for the CHANGELOG:** the declared minimum supported Rust version rises from 1.88 to
1.94. This is a corrected claim rather than a new restriction — 1.88 never worked — but it is the
kind of change a consumer reads the changelog for.

### Raising the floor un-gates MSRV-gated clippy lints — settled

clippy reads `rust-version` as the MSRV and suppresses lints whose suggested API is newer than it.
`duration_suboptimal_units` suggests `Duration::from_mins`/`from_hours`, which do not exist on 1.88
and do on 1.94 — so the lint was silent before this change and fires after it, in code this story
never otherwise touched. Confirmed by flipping the manifest back to 1.88 and re-running: **0 errors
on 1.88, 18 on 1.94**. The count then grew to **23**: the first run aborted compilation before
reaching `sipx-clstr-sim`, so five sim sites only became visible once the earlier crates were clean.

**Twenty of the twenty-three sites keep their seconds**, with a targeted `#[allow]` and a reason at
each. This is not lint-dodging: SIP measures time in seconds, and every one of these numbers is
either a spec-stated constant or is read against another second-valued quantity in the same
expression.

| site | why it stays in seconds |
|---|---|
| `proxy/src/config.rs` ×5, `proxy/src/context.rs` ×1 | Timer C. [proxy-behavior](../specs/proxy-behavior.md) F11 states it as "Default **180 s**, configurable ≥ 180 s", and vector row PB-F-1 as "Timer C set 180 s". `from_mins(3)` would stop matching the rows it is checked against |
| `probe/src/schedule.rs` ×9 | Every cadence is `from_secs(60)` read against seconds nearby — the 59/60 due boundary, dues spread at 20 and 40 across the interval, "60 s interval plus the 6 s jitter budget" = 66, and loops stepping one second at a time |
| `probe/src/echo.rs` ×1 | 1800 s is exactly half of `register_expires`, a SIP `Expires` value in seconds; `from_mins(30)` hides the halving the test exists to check |
| `registrar/tests/vectors_register_auth.rs` ×1 | The 300 s nonce lifetime is read against `T0 + 3_600`, a raw second count |
| `sim/tests/proxy_cancel.rs` ×1 | The 60 is a step toward Timer C's 180 s, followed by a 150 that is not a whole minute — converting only the 60 would make the pair look unrelated to the timer they straddle |
| `sim/tests/probe_run.rs` ×1, `sim/tests/probe_end_to_end.rs` ×1 | One probe interval of virtual time, sized against the scheduler's 60 s cadence |

**Three sites were converted**, where the value is a genuine whole unit with no second-valued
neighbour: `proxy/src/from_registrar.rs:53` (`from_secs(3_600)` → `from_hours(1)`, a one-hour
registration fixture) and the pair in `sim/src/sim.rs:742,745`, which convert together so the
`advance(x)` / `now == settled + x` assertion keeps reading as a pair.

The allows are per-site or per-test-function, except `probe/src/schedule.rs`, where all nine share
one reason and the lint joins that module's existing `#[allow]` list — the idiom already used for
`unwrap_used`/`expect_used`/`panic` there. `duration_suboptimal_units` is deliberately **not**
allowed workspace-wide: that would hide the same class of finding at every future floor raise, which
is the opposite of what this story is for.

Incidentally, the `msrv` gate step earned itself immediately — it is what proved `Duration::from_hours`
is actually available on 1.94 rather than only on the developer's stable.

## Notes
- The failure, on `rust:1.88-bookworm`, is in the kernel rather than in this workspace:
  ```
  error[E0277]: the trait bound `I: Ord` is not satisfied
  note: required for `timers::Entry<K, I>` to implement `Ord`
  note: required for `Reverse<timers::Entry<K, I>>` to implement `Ord`
  note: required by a bound in `BinaryHeap::<T>::new`
  error: could not compile `sipx-transport` (lib) due to 2 previous errors
  ```
  `BinaryHeap::<T>::new` carried a `T: Ord` bound that a later release relaxed, so `sipx-transport`
  compiles only on the newer form. That makes the real floor a property of the **kernel**, which is
  why the comment's claim to match it needs re-deriving rather than trusting.
- Found while building `KO-13`'s container image, which pinned the declared floor precisely because
  that is what the workspace claims to support. A development machine on current stable never sees
  this, which is the argument for the gate half of the acceptance.
- The image is pinned to 1.97 as a workaround, with this story named in the `Dockerfile`. That pin
  comes back to the declared floor once this settles.
