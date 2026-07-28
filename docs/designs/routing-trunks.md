# Design: Outbound routing & trunks

**Status:** proposed · **Pillar:** Signalling · **Epic:** `routing-trunks` ·
**Stories:** RT-1 … RT-4

## Why

Outbound routing is not "resolve an A record and send UDP." Getting a request off the cluster
means an ordered plan of attempts (RFC 3263: NAPTR → SRV → A/AAAA, priorities, weights, TTLs),
operational state above DNS (a carrier that answers `503` to every third INVITE is *up* in DNS
and *down* in reality), and honest behavior under overload (RFC 7339/7415 exist because naive
`503` retry storms cause congestion collapse). The sipx kernel provides correct RFC 3263
*selection* with RFC 2782 weighting behind a `Resolver` trait — built for a UA: synchronous,
per-URI prefetch, no shared cache. This epic owns "which egress, in what order, and when to
stop."

## Approach

**RoutePlan as a value.** Resolution produces an ordered `RoutePlan` of attempts (transport,
address, discovery source, priority, weight), consumed attempt-by-attempt by the proxy driver.
Failover rules are explicit and normative-to-be: which failures advance to the next candidate
(transport error, timeout, 503) versus which terminate the plan (definitive final responses), and
the RFC 3263 §4.3 rule that after first failover the element becomes transaction-stateful so
retransmissions follow the selected destination.

**Resolver at proxy throughput.** A proxy resolves orders of magnitude more URIs than a UA and
mostly the same few carrier domains. RT-1 designs the async, shared, TTL- and negative-caching
resolver — either as an upstream evolution of the sipx `Resolver` (option recorded in the
[upstream ledger](../upstream.md)) or as a caching layer here that snapshots into the existing
sync trait, preserving the kernel's "no await on the endpoint loop" rule either way.

**Trunks are stateful objects.** Above DNS, a trunk carries: concurrent-call and calls-per-second
limits, a circuit breaker fed by response-code and timeout history, retry budget, preferred
transports, number normalization and identity rules, and media policy hooks. Trunk selection is a
typed routing module (see `extension-framework`), not a DSL.

**Overload control.** RFC 7339 signaling with RFC 7415 rate-based control, applied on both
directions: honoring overload feedback from downstream, and emitting it upstream when this
cluster sheds load (tying into the transport layer's existing backpressure-`503` behavior). Spec
work lands with M2/M3 when the trunk model is real.

## Alternatives considered

- **Per-request DNS with OS resolution.** Rejected: no NAPTR/SRV semantics, no negative caching,
  blocking lookups on the signalling path.
- **Health checks as the only trunk state (OPTIONS pings).** Rejected as sole mechanism: probes
  measure the probe path; circuit breakers fed by real transaction outcomes measure the truth.
  Probing may supplement for idle trunks.
- **A routing script language.** Rejected by the vision's non-goal: policy composes from typed
  modules with declared inputs, not an embedded interpreter.

## Risks & open questions

- Upstream vs local for the async resolver (RT-1's headline decision).
- Circuit-breaker state scope: per-node or shared? Inclination: per-node with observability,
  since shared breaker state reintroduces cluster coupling for a heuristic.
- How trunk configuration versions interact with the affinity token's `policy version` field.

## Acceptance / done

The union of RT-1 … RT-4: a RoutePlan consumed by the proxy driver with specified failover
semantics passing harness vectors (including DNS failover mid-transaction); trunks enforcing
CPS/concurrency with breaker transitions observable; overload control demonstrated in the
simulated cluster without collapse (M3 exit criterion).
