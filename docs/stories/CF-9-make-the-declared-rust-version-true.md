---
id: CF-9
title: Make the declared rust-version true, and gate on it
pillar: Foundation
status: ready
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
- [ ] The declared `rust-version` is one the workspace actually builds on, established by building
      on it rather than by reasoning about it.
- [ ] The gate builds on the declared floor, so the two cannot drift apart again silently. Today
      `scripts/gate.sh` uses whatever toolchain the developer has, which on a current machine is
      several releases newer than the claim.
- [ ] If the floor is raised, the reason is recorded where the current comment is — the comment
      asserts a relationship to the kernel's floor that should be re-checked rather than copied.

## Progress
- (not started)

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
