---
id: FC-6
title: Refuse cluster.security policy until a specified consumer applies every declared control
pillar: Cluster
status: in-progress
priority: 1
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, security, node]
note: V-06 — four ingress controls load as applied, validate no values, and change no runtime decision
---

# Refuse cluster.security policy until a specified consumer applies every declared control

## Goal

Restore the fail-closed configuration invariant for `cluster.security`. Until source admission,
sanity checking, User-Agent denial and internal-zone classification have specified consumers, a
document asking for any of them must stop the node rather than start with the opposite posture.

## Acceptance

- [ ] A non-empty `cluster.security` block containing any of `unknownSource`, `sanityCheck`,
      `userAgentDenyList` or `internalZone` is a load error naming every declared path and saying this
      build cannot apply it. An absent or empty block remains valid and carries only the fixed
      Max-Forwards behavior; `maxForwards` remains forbidden as a knob under `CC-V6`.
- [ ] `read_security` no longer accepts values it never reads. It either has no allow-list for these
      unimplemented keys or descends far enough to produce the explicit unsupported-policy errors;
      it must not return `SecuritySpec::default()` after silently consuming them.
- [ ] The error is fatal, not an `unapplied` warning. These fields select who may reach a public SIP
      decision path, so starting without them is not a useful degraded mode.
- [ ] **Failing-first real-binary test:** start with `unknownSource: drop`,
      `userAgentDenyList: [evil-phone]`, and an `internalZone` that excludes loopback, then send the
      matching User-Agent from loopback. The test fails on `86e6b10`, where startup succeeds without
      an unapplied warning and the REGISTER receives `200 OK`; the green result is startup refusal
      before any socket is bound.
- [ ] A parser test supplies wrong-shaped values for all four keys and proves none can receive a
      successful `Config`; refusing the unsupported path before type validation is acceptable, but
      accepting arbitrary values is not.
- [ ] [cluster-config](../specs/cluster-config.md)'s section registry and vectors describe the same
      current-build refusal. The public configuration table must not call `security` applied until a
      later story supplies the specified consumers.
- [ ] `scripts/gate.sh` is green.

## Progress

- **2026-07-31 — started, killed mid-story by an org monthly spend limit, work rescued.** The
  implementor's uncommitted tree was committed by the coordinator as **`impl/FC-6-v06` at `8a78dda`**,
  worktree preserved at `/home/timo/projects/sipx-clstr-FC-6`. It carries partial work in
  `crates/sipx-clstr-node/src/config/mod.rs`, a new
  `crates/sipx-clstr-node/tests/security_refused.rs`, a touched
  `crates/sipx-clstr-node/tests/support/mod.rs`, and edits to `docs/specs/cluster-config.md`. **Not
  gated, not reviewed, and the loader half was still being written** — a starting point, not a merge
  candidate. Its last report was "now the loader".
- **The branch is deliberately not named `impl/FC-6`.** A branch by that name already exists and is
  merged into `main`, carrying `82b613a` "Apply the declared roles at dispatch, or refuse to start" —
  **different work**, which the review-backlog renumbering moved to `DP-13`. Do not be misled by
  `git log --grep=FC-6`; the story file is the source of truth and this story is the `V-06`
  `cluster.security` refusal.
- **Unverified on resume:** the `SecuritySpec` (`config/mod.rs:232-245`) and `read_security`
  (`:1444-1482`) line references in the Notes below were not re-checked against the current tree, and
  the file has moved under other stories. Confirm them before quoting them.
- **Two constraints from the dispatch worth keeping.** Build the refusal so it can be narrowed **one
  control at a time** — the story's own note anticipates a later feature story removing only its own
  control — rather than as an all-or-nothing gate a successor must tear out. And `FC-8` ("a refused
  value must not echo a secret") is `ready` and unlanded, so refusal messages must not create fresh
  instances of the defect it was filed for.
- **Blast radius unchecked.** A new refusal invalidates any in-tree document declaring one of the four
  controls; `deploy/`, `scripts/*.sh`, `website/docs/reference/configuration.md` and the devspace
  manifests were not swept. A refusal that turns the project's own proof scripts red is not done.

## Notes

- Source: validated synthesis **V-06**, reproduced against the real binary. `SecuritySpec` contains
  only fixed Max-Forwards (`crates/sipx-clstr-node/src/config/mod.rs:232-245`), while
  `read_security` recognizes four names and returns the default (`:1444-1482`); none reaches
  `NodeConfig`.
- Dependencies: none. This is the safe present-tense posture; a later feature story may specify and
  implement an individual control, then remove only that control from the refusal.
- Considered for upstream: **partly, later.** The decision to refuse unapplied cluster ingress
  policy is local configuration orchestration. Reusable SIP-message sanity primitives are
  protocol-generic and must be proposed to sipx before a future consumer is implemented here.
