---
id: CF-20
title: Make proof claims require executed evidence
pillar: Foundation
status: ready
priority: 2
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness, gate]
note: V-16 — a plain Rust function counts as a proved vector, and zero sockets prints as exactly one
---

# Make proof claims require executed evidence

## Goal

Close both false-positive paths in the assurance layer: a conformance row is proved only by a test
Cargo can actually execute, and the real-socket media check succeeds only when it observes exactly the
one signalling socket it claims to have observed.

## Acceptance

- [ ] `scripts/check-vectors.py` credits a named function or `// covers:` declaration only when the
      corresponding test is discoverable in the workspace's all-feature test inventory. A plain
      helper, an ignored test, and a test excluded by the active `cfg` do not prove a row.
- [ ] The checker has failing-first fixtures for all four cases: ordinary helper, `#[ignore]`,
      inactive `cfg`, and an executable test. The first three fail to cover the fabricated vector;
      the executable test alone covers it.
- [ ] A `// covers:` comment with no following executable test is rejected rather than attributed to
      an arbitrary function. The diagnostic names the vector ID, source path, and function or comment
      that tried to claim it.
- [ ] The checker does not replace execution evidence with a hand-maintained allowlist. Adding a new
      crate or test target is covered through Cargo/workspace discovery without editing a second list
      of test names.
- [ ] `scripts/e2e-call.sh` fails unless it can establish that the node owns **exactly one** UDP
      socket. Missing `ss`, unreadable process ownership, zero matches, and more than one match are all
      failures; none may print the one-socket success line.
- [ ] The socket-count logic has an isolated failing-first shell fixture (or equivalent executable
      test) covering tool failure and counts zero, one, and two, so CI can prove the assertion without
      relying on whichever sockets happen to exist on its runner.
- [ ] The conformance methodology page states the enforced definition: “proved” means an executable,
      non-ignored test in the configuration the gate runs. The e2e guide states what the socket check
      observes and no longer claims more than that observation establishes.
- [ ] `scripts/gate.sh` is green, and the CI real-socket job exercises the hardened socket assertion.

## Progress

- (not started)

## Notes

- Filed from the validated adversarial review of `86e6b10` (`v0.12.0`), synthesis finding **V-16**.
- Reproduction: a temporary source containing only
  `fn pb_v_999_plain_helper_is_not_a_test() { assert_eq!(1, 1); }` was credited as `PB-V-999` by
  `covered()`, although Cargo could never run it. The current credited functions were separately
  audited as tests; this is a gate weakness, not a claim that today's numerator is fabricated.
- The second false positive is at `scripts/e2e-call.sh:264-272`: only a count greater than one fails,
  so zero passes and prints “the node holds one socket.”
- Considered for upstream: **no for this story.** This work joins this repository's vector-ID naming
  convention, generated report, Cargo workspace, and shell proof. If implementation reveals a
  generally reusable test-discovery API missing from sipx testkit, file that primitive upstream
  before adding a duplicate; the repository-specific classification and socket assertion stay here.
