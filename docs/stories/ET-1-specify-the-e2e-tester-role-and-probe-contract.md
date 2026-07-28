---
id: ET-1
title: Specify the e2e-tester role and the probe contract
pillar: Platform
status: done
priority:
design: docs/designs/e2e-tester.md
epic: e2e-tester
areas: [probe]
note: M1 #11 · docs/specs/e2e-probe.md — verdict taxonomy, blast radius, 19 vectors; gates ET-2/ET-3
---

# Specify the e2e-tester role and the probe contract

## Goal
Write `docs/specs/e2e-probe.md`: what a probe run is, what it asserts, what a verdict means, and the shape of the trigger API — so the engine, the echo endpoint and the API are derived from one contract rather than three opinions.

## Acceptance
- [x] The spec defines the probe plan as a state machine (`register → invite → assert → bye`) with per-step timeouts, the correlation marker carried in the request, and the inputs (fired timers, injected randomness) that keep it sans-IO.
- [x] Verdict taxonomy is normative: `pass`, `fail(step, cause)`, `inconclusive(probe-side fault)` — with the cause list and the rule that a probe-side fault is never reported as a platform failure.
- [x] The target matrix is specified (per edge address, per transport, per zone; public path only — DNS/VIP, never an internal shortcut) along with schedule, interval jitter and rate bounds.
- [x] The control API schema is specified (`GET /probes`, `POST /probes/{name}/runs`, `GET /probes/{name}/runs/{id}`), private-interface-only, authenticated, with the run record identical for scheduled and triggered runs.
- [x] Blast-radius rules are normative: test tenant with no trunk access, identifying marker excluding probe traffic from business metrics/CDR, behavior when overload control sheds a probe.
- [x] Test vectors: at least one passing run and one failure per step, expressed so ET-2's harness scenarios derive from them.
- [x] Decided and recorded: whether the echo endpoint is the same binary in `echo` mode or a separate service, and how the role slots into DP-1's role set.

## Progress
[`docs/specs/e2e-probe.md`](../specs/e2e-probe.md) — eleven sections, **19 vectors** (`EP-P-*`,
`EP-F-*`, `EP-I-*`, `EP-C-*`), all registered in the conformance report and deferred to `ET-2`/`ET-3`
with reasons. The report now covers **two** specs rather than one: 34 of 61 rows proved.

### The decisions this story existed to make

**Five steps, not four.** The design's plan is `register → invite → assert → bye`; the spec makes
`Resolve` a step of its own, because for a DNS-name target the record *is* part of the service. A
NAPTR pointing at a drained zone is one of the failures the design lists as motivating the whole
role — folding it into "register failed" would report it as the wrong fault.

**`Subject` carries the marker, not a header we invented.** RFC 3261 §20.42 gives no intermediary a
reason to alter `Subject`, so a marker that does not come back is evidence about the *path*. An `X-`
header nothing is required to preserve would let a header-stripping element look like a routing
fault. The marker also must not be derivable from the `Call-ID`: something that merely saw the
request go past could otherwise reflect it.

**`inconclusive` is why the taxonomy is three-valued.** A probe that could not run is not an outage,
and conflating them trains operators to ignore the alert — an ignored alert being worse than none.
The cause and fault lists are **closed**, and anything fitting neither is `Inconclusive { Internal }`,
because a probe inventing a platform failure it cannot substantiate is worse than one admitting
confusion. `V5` draws the line that matters in practice: a `407` the probe *answered* and got again is
a platform behaviour; a challenge it cannot answer at all is its own credentials.

**`Shed` is a cause of its own.** Overload control shedding a probe is the platform working as
designed; silence means the listener is gone. They must not read alike, so `RT-3` shedding produces
`Fail { cause: Shed { retry_after } }` and `EP-F-10` distinguishes it from an ordinary `503`.

**The matrix is per zone, not full mesh.** N testers × M edges × T transports grows quadratically for
information that is mostly duplicated — what a cross-zone probe says about zone B's edges is what
zone B's own tester already says, and it costs a wide-area path in every measurement. Cross-zone
reachability gets its own, deliberately smaller matrix: the VIP only.

**Retries are not the probe's job** (`S7`). One attempt per step; the transport layer's
retransmission is the only retry. A probe that retried would mask exactly the marginal loss it exists
to report.

**A run always cleans up** (`S3`, `S4`), and cleanup never changes a verdict (`V6`). A probe that
abandons dialogs or accumulates bindings becomes the outage it was watching for; a probe whose
cleanup failure rewrote its verdict would report its own defect as the platform's.

**Nothing hooks the probe path** (`B6`). A probe that took a different path would stop measuring the
path customers take, so the CDR/metric exclusion is applied at the *reporting* boundary rather than by
routing probe traffic differently.

### The open question the design left, decided

**The echo is the same binary in `echo` mode**, a role in `DP-1`'s set. A separate service would be a
second artifact to build, version, deploy and secure for a component whose whole job is to answer
`200` and copy one header; one binary means the echo is covered by the same gate, the same provenance
check and the same release.

The hard constraint that made it a real question is honoured either way and is absolute: **no proxy
role ever links a UAS.** So `echo` is a role a process runs *instead of* a proxy role, and a
configuration asking for both is refused **at load** — where a human is still watching — rather than
at runtime, where nobody is. That is the constraint `DP-1`'s schema must enforce; `DP-1` still owns the
schema itself.

## Notes
- Design: [e2e-tester](../designs/e2e-tester.md). Role set and config schema:
  [DP-1](DP-1-design-roles-and-the-config-schema.md).
- Media assertions stay out of scope, as the story asked: `E4` states signalling-only for v1, and when
  a media assertion comes it goes through `MediaRelay` — never RTP in this process, which the vision
  rules out permanently.
- `scripts/check-vectors.py` now reads **every** spec that carries a vector table, keyed by row prefix,
  so a row is only claimed by the spec that owns its prefix — a design doc citing `PB-F-1` in passing
  cannot invent a row somebody has to prove. The report moved from `proxy-conformance.md` to
  `conformance.md` accordingly.
- `EP-F-5`/`-6`/`-7` are deferred to `ET-3` rather than `ET-2`: they are the marker rows, and a marker
  cannot be tested without something that reflects it.
