---
id: FC-8
title: A refused configuration value must not echo a secret into the log
pillar: Cluster
status: ready
priority: 2
design: docs/designs/fail-closed-config.md
epic: fail-closed-config
areas: [config, node]
note: ConfigError.found carries the offending value, and the offending value for an inline key secret is the secret
---

# A refused configuration value must not echo a secret into the log

## Goal
Make the loader's refusal messages safe to print, so rejecting a secret does not publish it.

## Acceptance
- [ ] `ConfigError`'s `found` is redacted for every field the schema marks secret-bearing, and the
      marking lives in the schema rather than in a list the next field has to be remembered onto.
- [ ] `cluster-membership` §4 `KY3` — the reserved-and-always-refused `secret` key — is covered: a
      document with an inline `keys[].secret` is refused, and the refusal names the path without
      echoing the value.
- [ ] The same holds for every `…Ref` field's neighbours: `dsnRef`, `secretRef`, `keyRef` name
      references rather than values, so a document that inlines a value where a reference is expected
      is exactly the case that leaks.
- [ ] **Failing-first:** a document with an inline key secret is loaded, the node refuses it, and the
      test asserts the secret's bytes appear **nowhere** in the error or the log line. It fails today.
- [ ] A vector row in `cluster-config` §12, registered with its test name in the same commit and
      deferred to a **live** story if it cannot be proved here.

## Progress
- (not started)

## Notes
- Found by `AF-6`'s review. `KY3` creates a refusal path whose natural `ConfigError.found`
  (`cluster-config` §8's `Option<String>`) *is* the inline secret, printed at startup where the node
  reports every configuration error at once, ordered by path.
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
