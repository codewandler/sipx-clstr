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

### Open: raising the floor un-gates MSRV-gated clippy lints

`cargo clippy --workspace --all-targets --all-features -- -D warnings` is **red** on this branch, and
it is this change that made it so. clippy reads `rust-version` as the MSRV and suppresses lints whose
suggested API is newer than it. `duration_suboptimal_units` suggests `Duration::from_mins`, which is
not available on 1.88 and is on 1.94 — so the lint was silent before and fires now, 18 times, in code
this branch never touched. Verified by flipping the manifest back to 1.88 and re-running: 0 errors on
1.88, 18 on 1.94. No other gate step regressed; `fmt`, `test`, `features`, `provenance`, `vectors`,
`docs` and the new `msrv` step are all green.

The remedy is mechanical — `Duration::from_secs(60)` → `Duration::from_mins(1)` and similar, which
clippy will autofix — but every site is outside this story's write set, and one is inside a crate
another in-flight story is editing, so this branch deliberately leaves them alone:

| file | sites |
|---|---|
| `crates/sipx-clstr-probe/src/schedule.rs` | 187, 214, 232, 244, 253, 274, 277, 304, 321 |
| `crates/sipx-clstr-proxy/src/config.rs` | 125, 160, 221, 222, 223 |
| `crates/sipx-clstr-probe/src/echo.rs` | 378 |
| `crates/sipx-clstr-proxy/src/context.rs` | 745 |
| `crates/sipx-clstr-proxy/src/from_registrar.rs` | 53 |
| `crates/sipx-clstr-registrar/tests/vectors_register_auth.rs` | 241 |

Whoever picks this up should prefer fixing the call sites over adding
`duration_suboptimal_units = "allow"` to `[workspace.lints.clippy]`: suppressing the lint would hide
the same class of finding at every future floor raise, which is the opposite of what this story is
for.

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
