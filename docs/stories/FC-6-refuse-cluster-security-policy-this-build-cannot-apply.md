---
id: FC-6
title: Refuse cluster.security policy until a specified consumer applies every declared control
pillar: Cluster
status: ready
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

- (not started)

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
