# Spec: End-to-end call probe

**Status:** normative · **Crate:** `sipx-clstr-probe` · **Stories:** ET-1 … ET-6 ·
**Design:** [e2e-tester](../designs/e2e-tester.md)

## 1. Normative references

- RFC 3261 §10 (REGISTER), §13 (dialog establishment), §15 (BYE) — the probe is an ordinary UAC and
  its SIP behaviour is the sipx kernel's UA and dialog layers used unmodified.
- RFC 3261 §8.1.1.4 (`Call-ID` uniqueness) and §20.10 (`Contact`) — the correlation marker (§4) is
  carried in headers the RFC already makes opaque to intermediaries.
- RFC 3263 — the probe resolves its target the way a UA does, because resolving differently would
  stop testing the thing that breaks.
- RFC 3261 §20.42 (`Subject`) — a header proxies pass through untouched, which is why §4 uses it for
  the marker rather than inventing a `X-` header no element is required to preserve.
- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- sipx kernel contracts consumed: the UA layer, the dialog layer, digest *client* authentication,
  and the transport layer's RFC 3263 resolution.
- This repo's specs consumed: [proxy-behavior](proxy-behavior.md) (the platform the probe traverses),
  [location-service](location-service.md) §5 (the probe and the echo both register as ordinary UAs),
  [hook-framework](hook-framework.md) (nothing hooks the probe path — see §8).

**Out of scope:** any media assertion (deferred; when it lands it goes through `MediaRelay`, never
RTP in this process — [media-control](../designs/media-control.md)); probing the *egress* path
through a trunk to a partner endpoint (needs an interop partner and cost controls); the metric and
alerting definitions themselves (`ET-5`, joining [deployment](../designs/deployment.md)'s set).

**Upstream considerations** (AGENTS.md rule 6): nothing here is protocol-generic. A probe is a UA
composed from kernel layers plus scheduling, correlation, assertion and verdict logic, all of which
are properties of *this platform's* operability. Considered for upstream: **no**.

## 2. What a probe run is

A run is one traversal of the platform's front door by an ordinary external UA, and a verdict about
it. The engine is a state machine; time enters as fired timers and jitter as an injected random
source, so the whole of §3–§6 runs unmodified inside the deterministic harness
([conformance-harness](../designs/conformance-harness.md)). A probe whose failure scenarios cannot
be replayed from a seed is a probe nobody will trust at 03:00.

```rust
struct ProbeRun {
    probe: ProbeName,          // which configured probe
    run_id: RunId,             // unique; identical shape for scheduled and triggered runs (§7)
    target: Target,            // §5 — the exact address, transport and zone dialled
    marker: Marker,            // §4 — planted by the probe, reflected by the echo
    started_at: Instant,       // an input, never a clock read
    steps: Vec<StepOutcome>,   // one per §3 step attempted, in order
    verdict: Verdict,          // §6
}
```

**Every run records every step it attempted**, including the ones that succeeded before the one that
failed. A verdict without the steps behind it tells an operator that something is wrong and nothing
about where, which is the difference between an alert and a page someone can act on.

## 3. The probe plan

Four steps, in order. Each has its own timeout; the first failure ends the run and the remaining
steps are **not** attempted.

| # | Step | Succeeds when | Default timeout |
|---|---|---|---|
| P1 | `Resolve` | The target's address is resolved per RFC 3263 (or is a literal) | 2 s |
| P2 | `Register` | A `2xx` to REGISTER, having answered any challenge | 5 s |
| P3 | `Invite` | A `2xx` to INVITE **and** the marker reflected (§4) | 10 s |
| P4 | `Bye` | A `2xx` to BYE | 5 s |

Rules:

| # | Rule |
|---|---|
| S1 | Steps run in the order above. A step is attempted only if every earlier one succeeded. |
| S2 | The ACK for P3's `2xx` is sent before P4. A probe that hung up without acknowledging would leave the echo retransmitting and would test the platform's cleanup rather than its call path. |
| S3 | **A run always attempts to clean up.** If P3 succeeded and a later step fails, the probe still sends BYE, and a failure of *that* BYE does not change the verdict already determined. A probe that abandons dialogs is a probe that becomes the outage. |
| S4 | The registration created by P2 is removed at the end of the run (`Contact` with `expires=0`), successful or not. A probe that accumulates bindings degrades the thing it measures. |
| S5 | Per-step elapsed time is recorded for every attempted step, including failed ones — the latency of a failure is diagnostic. |
| S6 | Timeouts are per step, not per run: a run's total budget is their sum, and there is no separate overall deadline to disagree with them. |
| S7 | **Retries are not the probe's job.** One attempt per step; the transport layer retransmits per RFC 3261 and that is the only retry. A probe that retried would mask exactly the marginal loss it exists to report. |

## 4. The correlation marker

A run plants a marker and requires it back. Without one, "the call was answered" cannot distinguish
*this* probe's call from a stale dialog, a duplicate, or another probe's traffic arriving at the
same echo.

| # | Rule |
|---|---|
| M1 | The marker is a run-unique opaque token, at least 128 bits of entropy from the injected random source, rendered as URL-safe base64. |
| M2 | It travels **outbound** in `Subject: sipx-probe/<marker>` — a header RFC 3261 §20.42 gives no intermediary reason to alter, so a marker that does not come back is evidence about the path rather than about header rewriting. |
| M3 | It travels **back** in the `2xx` to INVITE, in the same header. The echo copies it verbatim (§9). |
| M4 | P3 fails with cause `MarkerMismatch` when the `2xx` carries no marker or a different one. It is **not** treated as a network failure: the call was answered by something that is not our echo, which is a routing fault and reads differently. |
| M5 | The marker also names the run in every log line and metric label the run produces, so a verdict can be traced to a packet capture without a timestamp search. |
| M6 | The marker MUST NOT be derived from the `Call-ID`. They are correlated but not equal: a marker that could be predicted from a `Call-ID` could be reflected by something that merely saw the request go past. |

## 5. The target matrix

**The probe enters through the public path.** A probe that skips the front door cannot detect a
broken front door, and the front door is what breaks: a listener that stopped binding after a
restart, a VIP that stopped preserving source addresses, an expired certificate, a NAPTR record
pointing at a drained zone.

| # | Rule |
|---|---|
| T1 | A target is `(address, transport, zone)`. `address` is one of: a DNS name resolved per RFC 3263, the L4 VIP, or one specific edge address. |
| T2 | An internal address — a pod IP, a service name, anything that bypasses the VIP or the DNS record customers use — MUST NOT be a target. |
| T3 | Targets are enumerated in configuration, never discovered, so a failure is attributable to a named target and a target that vanishes from config is a deliberate act. |
| T4 | **The matrix is per zone, not full mesh.** Each `e2e-tester` instance probes the edges of its own zone plus the VIP and the DNS name. N testers × M edges × T transports grows quadratically for information that is mostly duplicated: what a cross-zone probe would tell you about zone B's edges is what zone B's own tester already says, and it costs a wide-area network path in every measurement. Cross-zone reachability is a separate, deliberately smaller matrix: the VIP only. |
| T5 | Each target is probed on its own schedule: a base interval with **jitter**, so a fleet of testers does not synchronize into a spike. Jitter comes from the injected random source, which is what lets a scenario replay it. |
| T6 | Probe rate is bounded per target and in total, and counted against ordinary CPS caps (§8). |

## 6. Verdicts

Three values, and the third is load-bearing.

```rust
enum Verdict {
    Pass,
    Fail { step: Step, cause: Cause },
    Inconclusive { fault: ProbeFault },
}
```

| # | Rule |
|---|---|
| V1 | `Pass` — every step of §3 succeeded, marker reflected. |
| V2 | `Fail { step, cause }` — a step the *platform* is responsible for did not succeed. Always carries which step and why. |
| V3 | `Inconclusive { fault }` — the probe could not conduct a valid test. **Never reported as a platform failure**, never counted in the success ratio, and alerted on separately: conflating "the service is down" with "the prober is broken" trains operators to ignore the alert, and an ignored alert is worse than no alert. |
| V4 | The cause and fault lists below are closed. A condition that fits neither is `Inconclusive { fault: Internal }` — because a probe that invents a platform failure it cannot substantiate is worse than one that admits confusion. |

**Causes** (a platform fault — `Fail`):

| Cause | Means |
|---|---|
| `Timeout` | The step's timeout elapsed with no final response. |
| `Rejected { status }` | A non-2xx final response. The status is carried: `403` and `503` are different incidents. |
| `MarkerMismatch` | Answered, but not by our echo (M4). |
| `Unreachable` | Transport-level failure: connection refused, TLS handshake failure, no route. |
| `ResolutionFailed` | P1 could not resolve the target — which for a DNS-name target *is* a platform fault, because that record is part of the service. |
| `Shed { retry_after }` | Overload control shed the probe (§8 B4). A distinct cause: shedding is the platform working as designed under load, and it must not read as a broken listener. |

**Faults** (the probe's own — `Inconclusive`):

| Fault | Means |
|---|---|
| `BadCredentials` | The probe's own credentials were rejected. Its account, not the platform. |
| `Misconfigured` | The probe's configuration is unusable — no targets, no echo AoR, contradictory timeouts. |
| `NoLocalResource` | The probe could not obtain a local socket or port. |
| `ClockSkew` | Local time moved in a way that invalidates the measurement. |
| `Cancelled` | The run was cancelled — shutdown, or a superseding trigger. |
| `Internal` | A probe-side error with no better name (V4). |

| # | Rule |
|---|---|
| V5 | A `Rejected` status of `401`/`407` after the probe has already answered a challenge is a `Fail`, not `BadCredentials`: repeated challenges are a platform behaviour. A challenge the probe *cannot* answer at all is `BadCredentials`. The distinction is whether the probe had a credential to offer. |
| V6 | Cleanup failures (S3, S4) never change a verdict. They are recorded and counted separately, because a leaked binding is a probe defect and a failed call is a platform one. |

## 7. The control API

A trigger, not a second code path: an on-demand run and a scheduled run produce the **identical**
run record. That is what makes the API usable as a deployment gate — CI, a release pipeline or the
operator can call it after a rollout and refuse to proceed on a failed verdict, knowing it is
looking at the same measurement the schedule produces.

| # | Rule |
|---|---|
| A1 | The API is bound **only** to the private management interface, never to a listener that carries SIP or faces a public network. |
| A2 | Every request is authenticated (mTLS client certificate, or a bearer token). An unauthenticated request is refused with `401`; there is no anonymous read, because the target matrix names a deployment's edges. |
| A3 | `GET /probes` → the configured probes and their targets. |
| A4 | `POST /probes/{name}/runs` → triggers a run. `202` with `{ run_id }` by default; `?wait=<seconds>` blocks up to that long and returns the completed record, or `202` with the id if it is still running. Blocking is the deployment-gate shape and the bound is the caller's, not ours. |
| A5 | `GET /probes/{name}/runs/{id}` → the run record: steps, timings, verdict. `404` when unknown, which includes "expired from the buffer" — a bounded buffer that lied about history would be worse than one that admits its horizon. |
| A6 | A triggered run is subject to the same rate bounds as a scheduled one (§5 T6). Exceeding them is `429` with `Retry-After` — an unbounded trigger is a denial-of-service tool wearing an API. |
| A7 | The run record's schema is identical whichever way the run started, and carries `trigger: scheduled | api` so the *provenance* is visible without the *shape* differing. |

## 8. Blast radius

Normative, because a probe that can hurt the deployment it watches is worse than no probe.

| # | Rule |
|---|---|
| B1 | Probe traffic lives in a dedicated **test tenant** whose route policy has **no external trunks**. A probe can never place a call towards a carrier — which is the failure mode that would otherwise cost real money and reach a real telephone. |
| B2 | That tenant is not privileged: no cross-tenant lookup, no trunk access, rate-capped. It is a normal tenant that happens to contain two UAs. |
| B3 | Probe calls carry the §4 marker, and every element that emits business metrics or CDRs **MUST** exclude marked traffic ([deployment](../designs/deployment.md)'s CDR field set, `DP-6`). Probe traffic is counted in probe metrics instead. A synthetic call in a revenue report is a data-integrity defect. |
| B4 | Overload control (`RT-3`) **may** shed a probe, and a shed probe is recorded as `Fail { cause: Shed }` (§6) rather than as silence. Silence and shedding must not look alike: one means the listener is gone and the other means the platform is protecting itself. |
| B5 | The probe MUST NOT be the load that tips an overloaded cluster: its rate is bounded (T6), and a probe is shed before ordinary traffic when the two compete. |
| B6 | **Nothing hooks the probe path.** No `hook-framework` phase fires differently for probe traffic, because a probe that took a different path through the platform would stop measuring the path customers take. B3's exclusion is applied at the *reporting* boundary, not by routing probe traffic differently. |

## 9. The echo endpoint

**Decided: the same binary, in `echo` mode** — a role in `DP-1`'s role set, selected by config,
alongside `edge`, `registrar`, `inbound-proxy`, `outbound-proxy` and `e2e-tester`.

Rationale, and the alternative rejected: a separate service would be a second artifact to build,
version, deploy and secure, for a component whose entire job is to answer `200` and copy one header.
One binary means the echo is covered by the same gate, the same provenance check and the same
release. The hard constraint that made this a real question is honoured either way, and it is
absolute: **no proxy role ever links a UAS.** The echo is a UAS; therefore `echo` is a role a
process runs *instead of* a proxy role, never in addition to one, and a configuration that asks for
both is rejected at load rather than at runtime.

| # | Rule |
|---|---|
| E1 | The echo registers in the test tenant as an ordinary UA, so the probe exercises the real path: edge → authentication → location lookup → forwarding → the answering leg. |
| E2 | It answers INVITE with `200` and **copies the `Subject` marker verbatim** (M3). |
| E3 | It answers BYE with `200` and holds no state between calls. |
| E4 | **Signalling only in v1.** The `200` carries an SDP answer negotiated by the kernel's layers, and the echo neither sends nor receives RTP. A media assertion, when it comes, goes through `MediaRelay` — never RTP in this process, which the vision rules out permanently. |
| E5 | The echo answers **only** calls carrying a marker, and only within its tenant. An echo that answered anything would be an open relay's more embarrassing cousin. |

## 10. Test vectors

Normative; `ET-2`'s harness scenarios derive from these, and `ET-3`'s echo from E1–E5.

**A passing run (EP-P).**

| # | Given | Expect |
|---|---|---|
| EP-P-1 | Every step succeeds; the echo reflects the marker | `Pass`; four step outcomes recorded, in order; the registration removed (S4) |
| EP-P-2 | The same run under jitter and reordering | `Pass`; identical verdict, and the run replays byte for byte from its seed |

**One failure per step (EP-F).**

| # | Given | Expect |
|---|---|---|
| EP-F-1 | P1: the DNS name does not resolve | `Fail { step: Resolve, cause: ResolutionFailed }`; no REGISTER attempted |
| EP-F-2 | P2: REGISTER times out | `Fail { step: Register, cause: Timeout }`; no INVITE attempted |
| EP-F-3 | P2: REGISTER answered `503` | `Fail { step: Register, cause: Rejected { status: 503 } }` |
| EP-F-4 | P3: INVITE answered `480` | `Fail { step: Invite, cause: Rejected { status: 480 } }`; no BYE sent (no dialog to end) |
| EP-F-5 | P3: `200` arrives with **no** marker | `Fail { step: Invite, cause: MarkerMismatch }`; **BYE still sent** (S3 — a dialog exists) |
| EP-F-6 | P3: `200` arrives with **another run's** marker | `Fail { step: Invite, cause: MarkerMismatch }` |
| EP-F-7 | P3: the edge answers `200` but the echo never rang | `Fail { step: Invite, cause: MarkerMismatch }` — the design's named scenario, and the marker is what detects it |
| EP-F-8 | P4: BYE times out after a successful call | `Fail { step: Bye, cause: Timeout }` |
| EP-F-9 | Connection refused on the target's transport | `Fail { step: Register, cause: Unreachable }` |
| EP-F-10 | The platform sheds the probe with `503` + `Retry-After` | `Fail { cause: Shed { retry_after } }`, distinct from EP-F-3 |

**Probe-side faults (EP-I).**

| # | Given | Expect |
|---|---|---|
| EP-I-1 | The probe's credentials are rejected with a challenge it cannot answer | `Inconclusive { fault: BadCredentials }`; **not** counted as a platform failure (V3) |
| EP-I-2 | Repeated `407` after the probe answered a challenge | `Fail { step: Register, cause: Rejected { status: 407 } }` (V5) — the distinction is whether a credential was offered |
| EP-I-3 | No targets configured | `Inconclusive { fault: Misconfigured }`; no SIP traffic emitted at all |
| EP-I-4 | A local socket cannot be obtained | `Inconclusive { fault: NoLocalResource }` |

**Cleanup (EP-C).**

| # | Given | Expect |
|---|---|---|
| EP-C-1 | P3 succeeds, P4's BYE fails | The verdict is `Fail { step: Bye, … }`; the registration is still removed (S4) |
| EP-C-2 | A run ends in any verdict | No binding for the probe AoR remains |
| EP-C-3 | Cleanup itself fails | The verdict is unchanged (V6); the cleanup failure is counted separately |

## 11. How the role slots into DP-1

`e2e-tester` and `echo` are two entries in `DP-1`'s role set, each selected by configuration on the
same binary. Neither is a signalling role: no proxy role links them, neither appears in a route
plan, and neither holds cluster state. `DP-1` owns the schema; what this spec fixes is the
constraint the schema must enforce — **a process running `echo` runs no proxy role**, and a
configuration asking for both is refused at load, where a human is still watching, rather than at
runtime, where nobody is.
