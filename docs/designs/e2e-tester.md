# Design: End-to-end call probe (`e2e-tester` role)

**Status:** proposed · **Pillar:** Platform · **Epic:** `e2e-tester` ·
**Stories:** ET-1 … ET-6

## Why

The outside view: can a real call be placed through this deployment, right now?

Every other epic proves the platform from the inside — spec vectors, seeded simulation, invariant
metrics — and none of that answers the operator's actual question: *can a real call be placed
through this deployment, right now?* Internal telemetry is structurally blind to the failures that
take a SIP service down without any node looking unhealthy: a listener that stopped binding after
a restart, a VIP that stopped preserving source addresses, an expired TLS certificate, a DNS
NAPTR/SRV record pointing at a drained zone, a firewall change upstream of every process that
could have reported it. The `e2e-tester` role closes that loop by dialling the border from
outside, on a schedule and on demand, and turning the result into a verdict the deployment can be
alerted on. It is also the acceptance instrument for the other epics: "the cluster is
indistinguishable from one correct proxy" becomes a claim someone can check continuously against a
running system, not only against a build.

## Approach

**One more role, deliberately off the call path.** `e2e-tester` joins `edge`, `registrar`,
`inbound-proxy` and `outbound-proxy` in the role set DP-1 designs — same binary, selected by
config. It is *not* a signalling role: no proxy role links it, it never appears in a route plan,
and it holds no cluster state. Its SIP behavior is the sipx kernel's UA and dialog layers used
unmodified (*upstream first*); this epic adds only orchestration — scheduling, correlation,
assertions, the trigger API and the verdict.

**It dials the border, from outside.** The probe enters the platform exactly the way a customer
does: through the public path (DNS NAPTR/SRV, or the L4 VIP, or a specific edge address), never
over an internal shortcut — a probe that skips the front door cannot detect a broken front door.
Targets are enumerated in config so a failure is attributable: per edge address, per transport
(UDP/TCP/TLS/WS/WSS), per zone. The probe registers as a probe AoR in a dedicated test tenant,
places a call, asserts, and hangs up.

**The echo, for now.** The call target is a minimal answering endpoint — the same binary in `echo`
mode — registered in the test tenant like any other UA, so the probe exercises the real path:
edge → authentication → registrar/location lookup → forwarding → the answering leg. v1 asserts
signalling only: the call is answered, a correlation marker planted by the probe is reflected back
by the echo, and per-step latency is recorded. **Media echo is deliberately deferred** and, when it
lands, is done through `MediaRelay`/rtpengine ([media-control](media-control.md)) — never by
moving RTP into the process, which the vision rules out permanently.

**The probe engine is sans-IO.** A probe run is a plan (`register → invite → assert → bye`) driven
as a state machine: fired timers are inputs, jitter comes from an injected random source, and
transport is a driver. The scheduler and the verdict logic therefore run unmodified inside the
CF-1 harness ([conformance-harness](conformance-harness.md)), so probe scenarios — including
failure scenarios like "the edge answers 200 but the echo never rings" — are seeded tests before
they are ever a cron.

**The trigger API.** A small HTTP control API on the private management interface, never on a
public listener, authenticated (mTLS or bearer): `GET /probes` lists configured probes,
`POST /probes/{name}/runs` triggers a run and returns a run id (or the verdict, when asked to
block), `GET /probes/{name}/runs/{id}` returns the result. The API is a *trigger*, not a second
code path: an on-demand run and a scheduled run produce the same run record. That makes it usable
as a deployment gate — CI, a release pipeline, or the Kubernetes operator
([k8s-deployment-operator](k8s-deployment-operator.md)) can call it after a rollout and refuse to
proceed on a failed verdict.

**Verdicts, not just booleans.** A run yields `pass`, `fail(step, cause)`, or
`inconclusive(probe-side fault)`. The third value is load-bearing: a probe that could not run —
bad credentials, no route to the target, its own clock skew — is not an outage, and conflating the
two trains operators to ignore the alert. Results feed the DP-3 metric set
([deployment](deployment.md)): success ratio and per-step latency histograms, broken down by edge,
transport and zone, with alerting on consecutive failures at one target because that is what
localizes a fault.

**Blast-radius rules.** Probe traffic lives in a test tenant whose route policy has no external
trunks, so a probe can never place a call towards a carrier. Probe calls carry an identifying
marker so they are excluded from business metrics and CDR while being counted in probe metrics.
Probe rate is bounded and counted against normal CPS caps. Overload control (RT-3) may shed a
probe — but a shed probe is recorded as such, because silence and shedding must not look alike.

## Alternatives considered

- **Rely on internal metrics only (DP-3).** Rejected as insufficient: internal metrics cannot
  observe what never reached a process. They stay the primary diagnostic; the probe supplies the
  outside view that says whether the service exists at all.
- **CI-only synthetic calls (CF-3's SIPp/CLI interop harness).** Kept, but not a substitute: CF-3
  proves a *build*; the probe proves a *running deployment*, continuously, including its
  configuration, certificates and network path.
- **Make the probe a loopback B2BUA inside the platform.** Rejected: a dialog-terminating element
  on the platform path contradicts *proxy-first*, and a loopback would not traverse the border.
  The probe is an ordinary external UA.
- **Echo the media in the probe process.** Rejected by the *no embedded media* non-goal; the media
  assertion, when it comes, is a relay-mediated one.
- **Drive probes from an external monitoring service.** Not exclusive — an external prober is
  welcome and the API supports it — but the role ships with the platform so a deployment has an
  outside view by default, and so probe behavior is testable in the same harness as everything
  else.

## Risks & open questions

- **Where the echo endpoint runs** — same binary in `echo` mode, or a separate small service.
  Decided in ET-1; the hard constraint is that no proxy role ever links a UAS.
- **Probe credentials.** The probe AoR needs real digest credentials; that tenant must not become
  a privileged backdoor. Scope: no trunk access, no cross-tenant lookup, rate-capped.
- **Probing the egress path.** Dialling *out* through a trunk to a partner echo endpoint would
  prove the outbound half; deferred — it needs an interop partner and cost controls.
- **Probing while unhealthy.** The probe must not be the load that tips an overloaded cluster;
  interaction with RT-3 shedding and CPS caps needs an explicit rule (bounded rate, shed-aware
  reporting).
- **Scale of the target matrix.** N testers × M edges × T transports grows fast; ET-1 decides
  whether the matrix is per-zone or full-mesh.
- **Alert quality.** Consecutive-failure thresholds and jitter have to be chosen so that a single
  lost UDP packet is not an incident and a dead listener is one within a bounded time.

## Acceptance / done

The union of ET-1 … ET-6: the probe contract is specified with a verdict taxonomy and API schema;
the probe engine and its failure scenarios run as seeded tests in the deterministic harness; the
echo endpoint answers and reflects the correlation marker; the control API triggers a run on
demand and returns the same record the schedule produces; probe results appear as metrics with
per-edge/per-transport breakdown and alerting; and the reference deployment runs continuous probes
whose failure is demonstrably caused by killing a listener, not by the probe itself.
