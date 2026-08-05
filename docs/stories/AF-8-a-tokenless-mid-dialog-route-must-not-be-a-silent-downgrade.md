---
id: AF-8
title: A tokenless mid-dialog platform Route must not be a silent downgrade
pillar: Cluster
status: backlog
design: docs/designs/cluster-affinity.md
epic: cluster-affinity
areas: [affinity, proxy, security]
note: blocked on DP-17 applying keys[] to the runtime key set — harmless until a token claim becomes a routing input
---

# A tokenless mid-dialog platform Route must not be a silent downgrade

## Goal

Close `affinity-token` §8's missing-token rejection once a key set can be required, so omitting the
`aft` parameter stops being a way to skip verification.

## Acceptance

- [ ] `affinity-token` §8's rule — a **tokenless** platform `Route` on a mid-dialog request is a
      `403` — is enforced, on the premise §5 states: there is no tokenless platform `Route` on a
      mid-dialog request once every edge mints.
- [ ] The premise is made true before the rule is enforced: a node that Record-Routes has a key set,
      or it refuses to start. This is the dependency, and it is `DP-17`'s runtime application of
      the `keys[]` document loaded by `DP-16` — `cluster-membership` §4. Enforcing the rule before
      that answers `403` to every in-call message on a keyless node.
- [ ] **Failing-first:** a mid-dialog request presenting a platform `Route` with no `aft` is
      forwarded today (`crates/sipx-clstr-proxy/src/types.rs:70-90` reports it as "nothing to
      verify") and is `403` after, with the keyless-node case proved to still work rather than
      collateral.
- [ ] The deviation recorded in `AF-5`'s Progress and in the `types.rs` comment is removed when the
      rule lands, so the comment and the code stop disagreeing with the spec.
- [ ] `scripts/gate.sh` is green.

## Progress
- (not started)

## Notes

- **Filed from the independent review of `AF-5`'s diff, 2026-07-31.** The review agreed with `AF-5`'s
  decision to defer this and with its reasoning, and asked that it not be left carried by a source
  comment alone.
- **Why it is not urgent, stated precisely so it is not over-escalated.** Bypassing verification
  grants nothing today: the engine discards the verdict's claims
  (`TokenVerdict::Valid { .. } => self.route_it()`, `crates/sipx-clstr-proxy/src/context.rs:184`),
  and an in-dialog request's target is the Request-URI — which a `Route`-less request already gets.
  So the attacker's reward for stripping `aft` is the routing they would have had anyway.
- **Why it must not be forgotten.** It becomes a real downgrade the moment **any** token claim
  becomes a routing input — home shard, media node, tenant. At that point stripping one parameter
  buys an attacker the default instead of the bound decision, and the code path that allows it will
  by then be years old and look deliberate.
- **Do not confuse this with `AF-5`'s blocking finding.** That one is a *token that is present,
  popped and ignored* on the second platform `Route` — a live defect against `proxy-behavior` §5 P2,
  fixed in `AF-5` itself. This story is the *absent* token case, which is a deliberate, spec-noted
  deviation.
- Blocked by `DP-17`, which applies the already-loaded `keys[]` section to the runtime key set.
- Considered for upstream: **no.** The token, its parameter and the platform `Route` convention are
  this platform's affinity mechanism; the kernel has no notion of them.
