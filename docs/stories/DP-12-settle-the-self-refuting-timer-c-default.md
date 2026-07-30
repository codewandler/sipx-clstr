---
id: DP-12
title: Settle the self-refuting Timer C default, and stop discarding accepted keys in silence
pillar: Platform
status: in-progress
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

- [x] **Failing-first**: a test loading a document whose `timers:` section omits `timerC` is refused
      at the merge base and accepted after. This is the whole bug in one document; it must be red
      first or the fix is unpinned.
      → `cc_v_12_a_timers_section_without_timer_c_loads_with_the_declared_default`,
      `crates/sipx-clstr-node/src/config/tests.rs:811`. Red at the merge base with the story's own
      reproduction verbatim: `cluster.timers.timerC [CC-V7]: found 180000 ms, expected greater than
      180000 ms (3 minutes), per RFC 3261 §16.6 step 11`.
- [x] §8 V7 no longer states a default that its own rule forbids. Either the default moves strictly
      above 3 minutes or the rule becomes `>=`, **and the choice cites RFC 3261 §16.6 step 11** —
      the RFC says "greater than 3 minutes", so a `>=` reading needs an argument, not a preference.
      Whichever way it settles, the spec and `crates/sipx-clstr-node/src/config/mod.rs` agree
      afterwards.
      → **The default moved; the rule stayed `>`.** §16.6 step 11 reads "the timer MUST be larger
      than 3 minutes" — a MUST over a strict inequality with no SHOULD and no rounding language, so
      `≥` would admit exactly the value the RFC forbids and has no textual support. New default
      **240 s**: the smallest whole-minute value above the floor, stated in the unit the RFC states
      the floor in. Reasoning is in the spec at `docs/specs/cluster-config.md:220` (V7), and in the
      code at `config/mod.rs:62` (`TIMER_C_FLOOR_MS`) and `:68` (`DEFAULT_TIMER_C_MS`).
- [x] Vector `CC-V-9` (`timers.timerC: 120s` → refused) still holds, and a new row covers the
      omitted-`timerC` case that is the actual defect. A rule this specific with no vector is how it
      got here.
      → `CC-V-9` was *deferred* rather than proved, which is the other half of how this survived. It
      now has a test (`cc_v_9_a_timer_c_below_three_minutes_is_refused`) and its deferral is gone
      from `docs/reference/vector-scope.toml`. New row **`CC-V-12`** at `cluster-config.md:380`,
      proved by the failing-first test. Both verified proved-and-not-deferred by `check-vectors.py`.
- [x] `maxCallDuration`, `locationStore.ha` and `listener[].tls` either reach `Config::unapplied` or
      are removed from the allow-list. Accepted-and-silently-dropped is not a third option.
      → All three now reach `unapplied`; reporting was chosen over removal because §7 *declares*
      these keys, so refusing them would put V2's closed world at odds with the schema. `tls` was
      already reported (`config/mod.rs:961`); `maxCallDuration` and `ha` were not. Pinned by
      `dp12_recognised_but_unread_keys_are_reported_as_unapplied`.
- [ ] `deploy/helm/values.yaml`'s `timerC: 600000` is revisited: if omission works after this, say
      in the file whether 600000 is still a deliberate choice or was only the workaround.
      → **Revisited; the premise does not hold and nothing was changed.** There is no `timerC:
      600000` and no `timers:` block anywhere in `deploy/helm/values.yaml`. What exists is
      `cluster.limits.timerC: 600s` (`values.yaml:221`), which predates this defect — it is from the
      M0 spec landing (`5ead6f7`), not from `KO-14`, which is still `ready` and has not touched the
      chart. `limits:` is not the schema's `timers:` at all; migrating it is `KO-14`'s own acceptance
      row, as is correcting the comment beside it. Left untouched to avoid conflicting with that
      story over a block it is going to rewrite wholesale.
- [x] `scripts/gate.sh` green.
      → Plus `python3 scripts/check-vectors.py --check`: `vectors: 136/493 rows proved, 357 deferred
      with a reason` (was 134/492 — one new row, two newly proved).

## Progress

- **Settled.** The RFC decides it: `>` is not a preference the spec chose, it is what §16.6 step 11
  says, so the default moved to 240 s rather than the rule relaxing to `≥`. Full reasoning lives in
  §8 V7 where a later reader hits it before the number.
- The floor check now runs against whatever value stands — written *or* defaulted. Previously
  `read_timers` returned early when the document carried no `timers:` section, so the default was
  never measured against its own rule; that early return is exactly what let a self-refuting default
  ship, invisible until some document happened to carry a `timers:` block. A `const _: () =
  assert!(DEFAULT_TIMER_C_MS > TIMER_C_FLOOR_MS);` makes the recurrence a build failure rather than a
  refusal in every operator's startup.
- **Found while doing this, deliberately not fixed — needs its own story.** The same defect exists a
  second time, in the crate that actually arms the timer, and `DP-12`'s fix does not reach it:
  - `docs/specs/proxy-behavior.md:162` F11 declares "Default **180 s**, configurable ≥ 180 s (§16.8
    'larger than 3 minutes')". That is the identical contradiction — an inclusive floor under an
    exclusive RFC bound, with the default sitting on it — plus a **mis-citation**: §16.8 is
    "Processing Timer C" and states no bound; the bound is §16.6 step 11, as `cluster-config.md` §1
    correctly cites. `crates/sipx-clstr-proxy/src/config.rs:128` (`Duration::from_secs(180)`) and
    `:165` (`.max(Duration::from_secs(180))`) implement F11 faithfully, so the proxy will happily arm
    a Timer C of exactly 3 minutes, which RFC 3261 forbids.
  - Worse, and the reason this is not cosmetic: **`TimersSpec` reaches no driver.** `Config::timers`
    is parsed and projected onto `ProjectedConfig` and then read by nothing —
    `driver.rs:680` `proxy_config_keyed` never assigns `proxy.timer_c`. So the *armed* Timer C on the
    call path is `ProxyConfig`'s 180 s regardless of what the cluster document says, and the whole
    `timers` section is accepted-and-discarded — the same `FC-2` class as the three keys above, one
    level up. Reporting only `maxCallDuration` as unapplied arguably understates it, but widening
    that to all of `cluster.timers` is a product decision that belongs with the wiring work, not
    here.
  - Closing it properly means moving `proxy-behavior.md` F11 **and** vector row `PB-F-1` ("Timer C
    set 180 s", `proxy-behavior.md:253`), then the three restatements in
    `docs/designs/extension-framework.md:160,315,1080` and `docs/specs/hook-framework.md:261`, then
    wiring `timers` through the driver. Four documents and two crates — outside this story's Goal,
    its write set, and its spec.

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
