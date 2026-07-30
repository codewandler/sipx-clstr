---
id: CF-15
title: No end-to-end proof runs in CI, and the check that would notice is inert
pillar: Foundation
status: done
epic: conformance-harness
areas: [ci, build]
note: all four proofs took the "recorded reason" branch; the "or it runs in CI" branch has never been exercised
---

# No end-to-end proof runs in CI, and the check that would notice is inert

## Goal

`DX-12` established a good rule: a script the site offers as **proof** must be run by the gate or by
CI, **or** carry a recorded reason why not. Every one of the four took the second branch.

Verified at integration — zero references, in either place:

```
e2e-call.sh              gate.sh:0   workflows:0
two-node-call.sh         gate.sh:0   workflows:0
k8s-two-node-call.sh     gate.sh:0   workflows:0
sip_demo.py              gate.sh:0   workflows:0
```

So the evidence for this project's headline claims — a call completes with audio, two nodes share one
location service, a registrar stores a binding — **runs only when a human remembers to run it**. That
is exactly how `FC-4`'s domain enforcement broke `two-node-call.sh` and `k8s-two-node-call.sh` for a
whole release without anything noticing: the proofs were red and nothing was watching.

Second, the check that is supposed to enforce the rule cannot currently distinguish the branches.
`check_proofs_are_gated` (`scripts/check-site.py:489-500`) resolves "is this proof in CI" with
`name in text` over `gate.sh` and `.github/workflows/` — a bare substring test, so a commented-out
line, or a mention in prose, would satisfy it. It is inert today only because *nothing* mentions any
proof anywhere; the moment one does, the test starts passing for the wrong reason.

## Acceptance

- [x] **Failing-first**: add a commented-out invocation of a proof script to `scripts/gate.sh` and
      show `check-site.py` accepts it as gated. That is the substring test failing open, and it must
      be red after.
- [x] "Runs in CI" means an invocation, not a mention. Parse a real invocation out of the shell and
      the workflow YAML, or move the declaration into a form that cannot be faked.
- [x] At least `scripts/e2e-call.sh` actually runs in CI. It is the one proof with no external
      dependency beyond the `sipx` CLI, it is what `website/docs/guides/registrations-and-calls.md`
      offers as *the* end-to-end evidence, and it has already been broken twice this cycle — once by
      `DP-10` removing the flags it used, once by `FC-4`. If the `sipx` CLI cannot be built in CI,
      say so with the error, and make `scripts/sip_demo.py` the CI proof instead: it needs nothing
      but standard-library Python.
- [x] The proofs that genuinely cannot run in CI (`k8s-two-node-call.sh` needs a cluster) keep their
      recorded reason, and the reason names what would change that — not merely that it is hard.
- [x] `scripts/gate.sh` green.

## Progress

- **The failing-first, on the real files.** With `e2e-call.sh`'s `not-in-ci:` directive removed (it
  is going into CI, so it no longer has one), adding the single line `# scripts/e2e-call.sh` to
  `scripts/gate.sh` took `check-site.py` from `FAIL — 1 problem(s)` / exit 1 to `clean` / exit 0. One
  commented-out line, nothing running the proof, and the checker satisfied. Red again after the fix.
- **"Runs in CI" is now an invocation.** `check-site.py` grew `strip_comment`, `workflow_run_lines`,
  `shell_commands` and `invokes`: shell comments are removed, `run:` bodies (inline and block
  scalar) are the only part of a workflow read, and the name must appear in *command position* —
  after env assignments and wrappers like `timeout`, but never as an argument to something else.
  Heredocs and `$(…)` are not tracked, which fails **closed**. A `self_test()` runs on every
  invocation (`check-vectors.py`'s convention) and pins fifteen cases, including the exact gate.sh
  line above and a step merely *named* after a proof.
- **`scripts/e2e-call.sh` runs in CI.** The `sipx` CLI turned out to build fine — a shallow clone and
  `cargo build --release --bin sipx` in **42s**, at both `v0.7.0` (the pinned tag) and `v0.10.0` — so
  the `sip_demo.py` fallback was not needed and `DX-12`'s recorded reason was a true premise with a
  wrong conclusion. New `e2e` job in `.github/workflows/ci.yml` builds the CLI at the tag read out of
  `Cargo.toml` (so a kernel bump moves the phone with it) and runs the script. Verified locally end
  to end: bob heard audio, 24000 samples, one socket on the node, transaction store drained.
- **The port residue `CF-13` handed over — solved by constraining, not by varying.** The first
  attempt gave each run its own address in `127.0.0.0/8`; two concurrent runs then passed together.
  It was reverted, because it is closed off from three directions: N7 forbids varying the *port*;
  `sipx dial` has no `--target`, so the call leg's address is the request-URI's, which drags the
  domain with it; and `check-proof-domains.py` (`FC-5`) deliberately refuses a domain that is not a
  static literal. That check is what makes `FC-4`'s `403`s impossible to repeat silently and is worth
  more than parallel local runs. So the address stays literal, a `preflight` refuses to start with
  exit 2 in 0s when `127.0.0.1:5060` is held, and the CI job is safe because `ubuntu-latest` is a
  fresh VM per job — one run per machine, nothing else on 5060.
- **The other three reasons now name their unblocking condition.** `k8s-two-node-call.sh`: the phone
  *image*, not the cluster, is the blocker. `two-node-call.sh`: both of `DX-12`'s reasons are settled
  (the CLI builds in 42s, PostgreSQL is a service container the `postgres` job already runs), so the
  honest reason is now "nobody has written the job" — recorded as such rather than left stale.
  `sip_demo.py`: it would have to start its own node, at which point it *is* `e2e-call.sh` with less
  coverage.

## Notes

- Found by `CF-14` while assessing its sibling checkers, and confirmed independently by the
  coordinator by grepping both sources for all four names.
- This is the difference between a rule and an enforced rule. `DX-12` correctly made the *decision*
  explicit — an unverified proof and a deliberately unverified one no longer look identical. This
  story is about the fact that, having made the decision visible, the decision we took for all four
  was "not verified".
- `two-node-call.sh` needs PostgreSQL, which a CI service container supplies; that one is closer to
  runnable than it looks, and it is the proof for the `0.11.0` headline.
- **Ports, before you wire anything in.** `CF-13` removed the fixed ports from the node test suites
  but deliberately left the proof scripts: `scripts/two-node-call.sh` and
  `scripts/k8s-two-node-call.sh` still hard-code `15081`/`15091`, and `scripts/e2e-call.sh` still
  defaults its node to `5060`, so two concurrent runs of the script collide with each other. That is
  harmless while nothing runs them automatically and becomes a flaky CI job the moment something
  does. `e2e-call.sh`'s node port is constrained: `sipx dial` addresses the node through the
  request-URI, and a request-URI with an explicit port is a different address-of-record from one
  without it (location-service §3.2 N7), so it cannot simply become ephemeral.
- Do not close this by wiring the scripts in and letting them be flaky. `CF-13` is fixing the fixed
  ports that make the driver suites unsafe to run concurrently; a proof that fails one run in five
  in CI will be muted within a week, which is worse than not running it.
