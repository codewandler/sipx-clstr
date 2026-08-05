---
id: CF-20
title: Make proof claims require executed evidence
pillar: Foundation
status: blocked
priority: 2
design: docs/designs/conformance-harness.md
epic: conformance-harness
areas: [harness, gate]
note: implementation and local live proof complete; closure awaits an observed GitHub real-socket run
---

# Make proof claims require executed evidence

## Goal

Close both false-positive paths in the assurance layer: a conformance row is proved only by a test
Cargo can actually execute, and the real-socket media check succeeds only when it observes exactly the
one signalling socket it claims to have observed.

## Acceptance

- [x] `scripts/check-vectors.py` credits a named function or `// covers:` declaration only when the
      corresponding test is discoverable in the workspace's all-feature test inventory. A plain
      helper, an ignored test, and a test excluded by the active `cfg` do not prove a row.
- [x] The checker has failing-first fixtures for all four cases: ordinary helper, `#[ignore]`,
      inactive `cfg`, and an executable test. The first three fail to cover the fabricated vector;
      the executable test alone covers it.
- [x] A `// covers:` comment with no following executable test is rejected rather than attributed to
      an arbitrary function. The diagnostic names the vector ID, source path, and function or comment
      that tried to claim it.
- [x] The checker does not replace execution evidence with a hand-maintained allowlist. Adding a new
      crate or test target is covered through Cargo/workspace discovery without editing a second list
      of test names.
- [x] `scripts/e2e-call.sh` fails unless it can establish that the node owns **exactly one** UDP
      socket. Missing `ss`, unreadable process ownership, zero matches, and more than one match are all
      failures; none may print the one-socket success line.
- [x] The socket-count logic has an isolated failing-first shell fixture (or equivalent executable
      test) covering tool failure and counts zero, one, and two, so CI can prove the assertion without
      relying on whichever sockets happen to exist on its runner.
- [x] The conformance methodology page states the enforced definition: “proved” means an executable,
      non-ignored test in the configuration the gate runs. The e2e guide states what the socket check
      observes and no longer claims more than that observation establishes.
- [x] `scripts/gate.sh` is green, and the checked CI workflows invoke both the isolated fixture and
      the hardened assertion through the real-socket script.
- [ ] A GitHub real-socket run actually reaches and passes that assertion. This is blocked by
      the release-preparation changes remaining uncommitted: `CX-15` removed the sibling patch and
      pinned an immutable tag, but workflow wiring is not execution evidence and no GitHub runner can
      observe this revision before the user authorizes commit and push.

## Progress

- 2026-08-05 — Began failing-first from the two validated V-16 reproductions. The executable-proof
  fixture covers a plain helper, ignored test, inactive-cfg test, ordinary executable test, and both
  valid and invalid `// covers:` bindings. The isolated socket fixture owns missing-tool, inspection
  failure, unreadable ownership, and zero/one/two-count cases before the e2e script consumes it.
- 2026-08-05 — Initial implementation. Cargo metadata supplies workspace source roots and compiler-artifact
  JSON supplies every libtest executable; active listings minus ignored listings are intersected
  with attributed source functions, so no test-name allowlist exists. The self-test proves
  all four discovery outcomes and both `// covers:` failures. The socket fixture proves five failure
  directions plus exact-one success. The local gate invokes it, and the checked CI workflows are
  wired to the fixture and to the same helper through `e2e-call.sh`; that is wiring, not a claim that
  the currently dependency-blocked GitHub jobs reached either one. Conformance remains 215/619.
- 2026-08-05 — Reopened after independent review reproduced a `// covers:` declaration at the end
  of one module binding to the first test in a later module. The existing "next function anywhere
  in the file" rule is not an item relationship. CI wording is reopened too: CX-12 records that the
  checked workflow cannot resolve the temporary sibling `../sipx` patch on an Ubuntu runner, so
  workflow wiring is not an observed CI execution.
- 2026-08-05 — Closed the review's proof-discovery findings. `// covers:` now binds only when it is
  same-indented leading trivia of one function item with no brace, module, statement or other
  structural line between them. The 68-case self-test reproduces cross-module drift and pins both
  duplicate-source and duplicate-Cargo-name ambiguity branches. The full local gate and a live
  same-kernel e2e call are green; the latter observed one node-owned UDP socket, 24000 audio samples,
  and a drained transaction store. The story remains `blocked`, not `done`, solely on the unchecked
  GitHub-runner acceptance above.
- 2026-08-05 — Tightened the declaration boundary after a second review: only a standalone line/doc
  comment is a claim, so matching string contents and trailing comments are ignored. Attribute
  scanning now parses complete attributes without counting brackets inside Rust strings; the fixture
  pins the prior `#[doc = "["]` false positive with an intervening constant.
- 2026-08-05 — `CX-15` pinned the published `v1.0.0-beta.5` tag and removed the sibling patch. Local
  dependency resolution and the checked fixture now pass without a sibling checkout. The final
  acceptance item remains open until this exact revision is committed, pushed and observed in the
  GitHub real-socket job; workflow text is not substituted for that evidence.

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
- Blocker: commit and push require explicit user instruction. Once this revision reaches GitHub, the
  real-socket job must pass the hardened assertion before this story returns to `done` and gains a
  CHANGELOG entry.
