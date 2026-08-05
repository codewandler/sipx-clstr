---
id: FC-8
title: Keep refused configuration secrets out of every diagnostic
pillar: Cluster
status: done
priority: 2
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, node]
note: V9 now owns redaction across five inline-secret paths; the failing-first pass also closed listener and management TLS key acceptance
---

# A refused configuration value must not echo a secret into the log

## Goal
Make the loader's refusal messages safe to print, so rejecting a secret does not publish it.

## Acceptance
- [x] The rule is stated in **`cluster-config` §8, beside `V9`** — it is a property of `ConfigError`,
      which §8 declares, not of keys. Leaving three call sites governed by one spec's key section is
      the "unowned rule" shape §7 `S1` warns about.
- [x] It **permits both forms**: a `ConfigError` on a `V9` path carries a *description* of what was
      written, or nothing — never the value. `config/mod.rs:1288` passes `found: None` for the
      *missing* `dsnRef` case, so a rule demanding a description would read as requiring text where
      absence is the honest answer.
- [x] `cluster-membership` §4 `KY3` — the reserved-and-always-refused `secret` key — is covered: a
      document with an inline `keys[].secret` is refused, and the refusal names the path without
      echoing the value.
- [x] The same holds for every `…Ref` field's neighbours: `dsnRef`, `secretRef`, `keyRef` name
      references rather than values, so a document that inlines a value where a reference is expected
      is exactly the case that leaks.
- [x] **Failing-first, and it is cheap:** the two existing call sites can be asserted on day one,
      because they already behave correctly. The new coverage is `KY3`'s inline `keys[].secret` and
      `tenant[].auth.secret`, where the test asserts the secret's bytes appear **nowhere** in the
      typed error, stable startup message, stdout, or stderr. The operator surface does not exist;
      `CC-V-26` defers its admission error response to live story `KO-3`, and CRD A2 forbids an
      admission refusal from creating an object or changing `status`.
- [x] A vector row in `cluster-config` §12, registered with its test name in the same commit and
      deferred to a **live** story if it cannot be proved here.

## Progress
- `cluster-config` §8 V9 now owns the general rule: `found` is a description or absent, and every
  consumer of the refusal must preserve that redaction.
- `CC-V-25` runs five distinct sentinels through the neighbours of `dsnRef`, both `secretRef`
  fields, and both `keyRef` blocks. The first failing run found that a TLS `key` inside the
  still-deferred block loaded without a V9 error; the targeted redaction check now covers listener
  and management TLS without taking over `FC-1`'s broader decision about applying or refusing those
  blocks.
- The real binary is exercised for all five paths, and both its stable startup messages and stderr
  name each path under `CC-V9` without containing the sentinel. No operator admission consumer
  exists, so `CC-V-26` defers its error-response surface to live story `KO-3` rather than calling a
  second `ConfigError::to_string()` an operator proof; A2 explicitly rules out a status mutation.

## Notes
- Found by `AF-6`'s review, and then **narrowed by `AF-6` itself into something much smaller**: this
  is not a new convention to invent, it is an existing one to write down. `DP-8` already redacts, in
  two places, against no written rule — `config/mod.rs:1278` emits `Some("an inline DSN")` and
  `:1431` emits `Some("an inline nonce secret")`, with the reasoning in a comment at `:1424`. `KY3`
  added a third. So the implementing story documents behaviour rather than requesting a change.
- **The hazard pre-exists `AF-6`** — `CC-V-10` already expects `E(cluster.keys[0].secret, CC-V9)` and
  nothing redacts it — but `cluster-membership` is now the spec that owns secret handling, so the rule
  belongs somewhere and nowhere currently states it.
- Note the interaction with a property this project is otherwise proud of: the loader reports **every**
  mistake in one pass rather than one per restart. That is the right behaviour and it is also what
  turns a single leaked value into a line in every operator's terminal and CI log.
- The existing discipline to build on: `DP-10` established that a secret reference which does not
  resolve **stops the node** rather than being ignored, and that the log reports the backend by name
  and never the resolved value. This is the same rule applied to the refusal path rather than the
  success path.
- Considered for upstream: **no.** This is this platform's configuration loader and its error type.
