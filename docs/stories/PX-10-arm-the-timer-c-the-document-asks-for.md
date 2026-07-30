---
id: PX-10
title: Arm the Timer C the document asks for, and settle F11's copy of the self-refuting default
pillar: Platform
status: done
priority: 1
epic:
areas: [proxy, node, config]
note: timerC now reaches the engine from the document and F11 defaults to 240 s; the rest of the timers section is reported unapplied rather than dropped
---

# Arm the Timer C the document asks for, and settle F11's copy of the self-refuting default

## Goal

`DP-12` settled the Timer C default in the configuration schema. The same defect exists a second
time in the crate that actually **arms** the timer, and `DP-12`'s fix does not reach it.

Two problems, and the second is the one that matters:

**1 — `proxy-behavior.md` §F11 repeats the contradiction, and mis-cites the RFC.** It reads
"Default **180 s**, configurable ≥ 180 s (§16.8 'larger than 3 minutes')". That is an *inclusive*
floor under an *exclusive* RFC bound with the default sitting exactly on it — the same shape
`DP-12` removed from `cluster-config.md` §8 V7. The citation is also wrong: RFC 3261 §16.8 is
"Processing Timer C" and states no bound at all; the bound is **§16.6 step 11**, which
`cluster-config.md` §1 cites correctly. `crates/sipx-clstr-proxy/src/config.rs:128` and `:165`
(`.max(Duration::from_secs(180))`) implement F11 faithfully, so the proxy will arm a Timer C of
exactly three minutes — the one value RFC 3261 forbids.

**2 — `TimersSpec` reaches no driver, so none of it is armed anyway.** `Config::timers` is parsed,
validated, and projected onto `ProjectedConfig`, and then read by nothing:
`crates/sipx-clstr-node/src/driver.rs:680` `proxy_config_keyed` never assigns `proxy.timer_c`.
`grep -n timer_c crates/sipx-clstr-node/src/driver.rs` returns **no matches**. An operator who sets
`timers.timerC` in the cluster document gets the proxy crate's own 180 s, silently.

This is the `FC-2` class — accepted and discarded without a word — one level above the three keys
`DP-12` just fixed. `DP-12` reports `maxCallDuration` as unapplied, which understates it: on the
evidence, *the entire `timers` section* is unapplied.

## Acceptance

- [x] **Failing-first**: a test that loads a document setting `timers.timerC` to a value distinct
      from every default, drives an INVITE that forks, and asserts the *armed* Timer C is the
      document's value. It must fail at the merge base, where the answer is always 180 s.
      → `crates/sipx-clstr-node/tests/timer_c_armed.rs`; at the merge base both cases fail with
      `left: 180s`.
- [x] `timers` is wired through the driver, or every key in it that is not wired is reported in
      `Config::unapplied`. Those are the only two honest outcomes; the current third — accepted,
      projected, and dropped — is the one being removed.
      → both halves: `timerC` wired (`startup.rs` → `NodeConfig::timer_c` → `driver.rs:proxy_config_keyed`),
      and `t1`/`timerB`/`timerF`/`maxCallDuration` reported (`config/mod.rs:read_timers`).
- [x] `proxy-behavior.md` F11 states a default and a floor that can both hold at once, cites
      **§16.6 step 11** rather than §16.8, and agrees with `cluster-config.md` §8 V7 as `DP-12` left
      it (floor `> 180 s`, default 240 s). Two specs disagreeing about one timer is how this
      survived twice. → `docs/specs/proxy-behavior.md:162`.
- [x] Vector row `PB-F-1` ("Timer C set 180 s", `proxy-behavior.md:253`) moves with F11 and is
      **proved**, not deferred. `DP-12` found `CC-V-9` was deferred rather than proved, and that is
      precisely why the sibling defect went unnoticed. → the row reads 240 s, and
      `pb_f_1_a_dialog_forming_invite_is_record_routed_with_a_branch_and_timer_c` now compares the
      armed duration; it previously asserted only that *a* timer was set, which is why a row saying
      180 s and code arming 180 s never met.
- [x] `crates/sipx-clstr-proxy/src/config.rs:128` and `:165` agree with the corrected F11.
      → `DEFAULT_TIMER_C` (240 s) and `TIMER_C_FLOOR` (180 s, exclusive); `effective_timer_c` falls
      back to the default rather than clamping onto a strict bound.
- [x] `deploy/helm/values.yaml:403`'s `timerC: 600000` is revisited. It was declared as a workaround
      for the self-refuting default (`KO-14` said so); omission works now, so the file should either
      drop it or say that 10 minutes is a deliberate choice. → dropped, with the reason recorded;
      `deploy/helm/check-values.sh` still renders and loads for every role.
- [x] `scripts/gate.sh` green.

## Progress

- 2026-07-30 (`PX-10`): done, with one caveat stated below that a reader should not miss.
- **Considered for upstream (AGENTS.md #6): no — the arming stays here.** RFC 3261 gives the kernel
  the *transaction* timers (§17: A, B, D, E, F, G, H, I, J, K) and gives Timer C to the **proxy TU**
  (§16.6 step 11 arms it, §16.7 restarts it on each 101–199, §16.8 processes it). It is a property of
  a response context fanning one server transaction out to N client transactions, and that driver is
  already recorded as local in [upstream.md](../upstream.md) ("Decided (not upstream)"). No new
  ledger row: this is that row.
- **The caveat: the node driver still performs no timer at all.** `driver.rs`'s effect loop drops
  `SetTimer`/`ClearTimer` on the floor, as it always has. What `PX-10` fixed is the *value* the
  engine puts in `SetTimer` — the document's, not a private 180 s. A fired Timer C on a real socket
  is `PX-6`'s, together with `CancelBranch`, which is dropped in the same arm: a fired Timer C's
  first act is to cancel the branch (§9 C5), so arming without the cancel would reap branches the
  node could not tell to stop. Commented at the arm rather than left for the next reader to discover.
- The armed value is asserted where it is because it cannot be asserted anywhere later: every Timer C
  §8 V7 admits is `> 180 s`, so a real-clock test would take over three minutes, and the harness that
  *does* fire timers in virtual time has no dependency on the node crate and cannot see `driver.rs`.
- `crates/sipx-clstr-sim/tests/proxy_cancel.rs` advanced `60 s` then `150 s` — a pair chosen to
  straddle 180 s — and stopped straddling anything when the default moved. Rewritten against
  `DEFAULT_TIMER_C` so the next change to the default cannot silently un-fire the timer.

## Notes

- Found by `DP-12` while fixing the schema half, reported as ADJACENT rather than quietly widened
  into that story — the right call, since this crosses two crates and four documents.
- The blast radius `DP-12` mapped: F11 and row `PB-F-1` in `proxy-behavior.md`, then
  `docs/designs/extension-framework.md:160,315,1080` and `docs/specs/hook-framework.md:261`, then
  the driver wiring. Check each rather than trusting the list.
- Timer C is the only bound on a branch that has gone quiet since its last provisional response
  (§16.7 restarts it on each 101–199), so this is not a cosmetic number: every extra minute is a
  minute a wedged branch holds a proxied transaction and, since `DP-11`, an admission slot.
- `AGENTS.md` non-negotiable #6 applies to the arming itself: transaction timers are protocol-generic
  and may belong in the kernel. Record the answer either way.
