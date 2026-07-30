---
id: DP-12
title: Settle the self-refuting Timer C default, and stop discarding accepted keys in silence
pillar: Platform
status: ready
priority: 1
epic:
areas: [config, node]
note: a document carrying a timers section with no timerC is refused by the loader's own declared default
---

# Settle the self-refuting Timer C default, and stop discarding accepted keys in silence

## Goal

`cluster-config.md` §8 V7 declares `timerC` with **default 180 s** and, in the same sentence,
`MUST be > 3 minutes`. 180 s *is* three minutes, so the declared default violates the rule it is
declared beside, and a document that carries a `timers:` section without naming `timerC` is refused
by the loader for the value the loader itself supplied.

Reproduced against `main` — a document whose `timers:` block sets only `t1`:

```
sipx-clstr: cluster.yaml was refused — 1 problem(s):
  cluster.timers.timerC [CC-V7]: found 180000 ms, expected greater than 180000 ms (3 minutes),
    per RFC 3261 §16.6 step 11
```

Nothing is wrong with the check: RFC 3261 §16.6 step 11 does say *greater than* 3 minutes, and the
loader implements that faithfully. The **default** is the defect, and it is in the spec before it is
in the code. `KO-14` hit this from the other side and had to declare `timerC: 600000` in the chart to
get a node to boot at all — a workaround for a rule that cannot be satisfied by omission.

Second, smaller defect found alongside it: `timers.maxCallDuration`, `locationStore.ha` and
`listener[].tls` are on the loader's closed-world allow-lists, read by nothing, and **not** reported
in `Config::unapplied`. They are accepted and silently discarded — the precise class `FC-2` added
`unapplied` to eliminate. An operator who sets `maxCallDuration` today is told nothing and gets
nothing.

## Acceptance

- [ ] **Failing-first**: a test loading a document whose `timers:` section omits `timerC` is refused
      at the merge base and accepted after. This is the whole bug in one document; it must be red
      first or the fix is unpinned.
- [ ] §8 V7 no longer states a default that its own rule forbids. Either the default moves strictly
      above 3 minutes or the rule becomes `>=`, **and the choice cites RFC 3261 §16.6 step 11** —
      the RFC says "greater than 3 minutes", so a `>=` reading needs an argument, not a preference.
      Whichever way it settles, the spec and `crates/sipx-clstr-node/src/config/mod.rs` agree
      afterwards.
- [ ] Vector `CC-V-9` (`timers.timerC: 120s` → refused) still holds, and a new row covers the
      omitted-`timerC` case that is the actual defect. A rule this specific with no vector is how it
      got here.
- [ ] `maxCallDuration`, `locationStore.ha` and `listener[].tls` either reach `Config::unapplied` or
      are removed from the allow-list. Accepted-and-silently-dropped is not a third option.
- [ ] `deploy/helm/values.yaml`'s `timerC: 600000` is revisited: if omission works after this, say
      in the file whether 600000 is still a deliberate choice or was only the workaround.
- [ ] `scripts/gate.sh` green.

## Progress

- (running log)

## Notes

- Found by the coordinator while integrating `KO-14`, and confirmed by running the binary rather
  than by reading the rule — the loader's own default is what it refuses.
- The interesting question is which document is wrong. The RFC fixes the floor and says nothing
  about a default; the spec chose 180 s, which is the floor exactly. A default equal to an exclusive
  bound is always a bug, so this is a spec defect that the code inherited honestly.
- Do not "fix" this by making the check `<` against a different constant in the loader alone. The
  spec is normative here (`AGENTS.md` non-negotiable #4) and the two must not drift apart again.
- `timerC` is the one timer in the set with a *lower* bound rather than an upper one, which is why
  this class of mistake did not show up for `t1`/`timerB`/`timerF`.
