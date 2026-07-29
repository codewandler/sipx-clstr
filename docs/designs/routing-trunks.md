# Design: Outbound routing & trunks

**Status:** accepted (RT-1) · **Pillar:** Signalling · **Epic:** `routing-trunks` ·
**Stories:** RT-1 … RT-9

## Why

Which egress, in what order, and when to stop — routing as plans, trunks as stateful objects.

Outbound routing is not "resolve an A record and send UDP." Getting a request off the cluster
means an ordered plan of attempts (RFC 3263: NAPTR → SRV → A/AAAA, priorities, weights, TTLs),
operational state above DNS (a carrier that answers `503` to every third INVITE is *up* in DNS
and *down* in reality), and honest behavior under overload (RFC 7339/7415 exist because naive
`503` retry storms cause congestion collapse). The sipx kernel provides correct RFC 3263
*selection* with RFC 2782 weighting behind a `Resolver` trait, and since `v0.4.0` (`T-17`) an
async path over a shared, TTL-honouring cache as well. This epic owns what the kernel does not:
"which egress, in what order, and when to stop."

## Approach

**RoutePlan as a value.** Resolution produces an ordered `RoutePlan` of attempts (transport,
address, discovery source, priority, weight), consumed attempt-by-attempt by the proxy driver.
Failover rules are explicit and normative-to-be: which failures advance to the next candidate
(transport error, timeout, 503) versus which terminate the plan (definitive final responses), and
the RFC 3263 §4.4 rule that after first failover the element becomes transaction-stateful so
retransmissions follow the selected destination.

### RT-1: the resolver decision, settled upstream

**Decided: the resolver is the kernel's.** `T-17` shipped in sipx `v0.4.0` and closes every
requirement this epic had of it, so there is no caching layer here and no local resolver to
maintain. What the kernel now provides:

| What RT-1 needed | What `v0.4.0` has |
|---|---|
| An async path that never blocks the signalling loop | `dns::resolve_uri(uri, resolver, rng).await -> Vec<Target>`, and the two-step `Prefetched::for_domain(...).await` the endpoint itself uses |
| One cache shared across callers, TTL-honouring both ways | `DnsResolver` caches positive answers and SOA-backed negatives (RFC 2308 §5), capped by `with_max_ttl` |
| "No such record" distinguished from "could not ask" | `Answer::{Records, Unavailable}` — `Unavailable` is deliberately *not* cached, so a nameserver blip cannot become a permanent routing failure |
| `_sip._ws` / `_sips._wss` prefetched, not paid for serially | All five RFC 3263 §4.1 and RFC 7118 §6 prefixes are prefetched together |
| Concurrent lookups of one name collapsing to one query | Provided by `hickory-resolver`; the kernel keeps the test rather than the redundant layer it first wrote |
| The synchronous trait unchanged for UA callers | `Resolver` is still sync, and selection is still pure computation over records |

**The seam is `Prefetched`, and it is the whole reason this decision was available.** RFC 3263
selection is pure — records in, ordered candidates out — so the kernel does the awaiting first and
hands the answers to `resolve::resolve`, which is synchronous. That split is what lets a proxy use
the same selection logic a UA does without either becoming async. Building a caching layer here
would have meant re-deciding it worse.

**What stays local is the plan, not the resolution.** The kernel answers *where could this URI be
reached*; it does not and should not answer *which egress should this call take, in what order,
and when do we stop trying*. `RoutePlan` is that second question:

```text
RoutePlan  = ordered [Attempt]  + the trunk it belongs to + the policy version it was built under
Attempt    = kernel Target (addr, transport, verify_as)
           + source   (Naptr | Srv | AddressRecord | Configured | Literal)
           + priority, weight   (carried through for observability, not re-sorted here)
```

`Attempt` **wraps** the kernel's `Target` rather than restating it: `addr`, `transport` and
`verify_as` are already right, and a second address type in this repo would be one more thing that
can disagree with the kernel about which host a TLS certificate must be valid for. What the
wrapper adds is provenance — a plan whose attempts do not record whether they came from an SRV
record or from static trunk configuration cannot explain, at three in the morning, why traffic went
somewhere surprising — and the trunk context `RT-2`'s breaker and CPS limits are keyed on.

**Consumption contract.** The sans-IO core never resolves. It emits the `ResolveTargets` effect it
already has ([proxy-behavior](../specs/proxy-behavior.md)), the driver awaits `resolve_uri`, builds
the plan, and feeds it back as one input. Every await is therefore in the driver by construction —
the same seam non-negotiable 2 draws everywhere else, and the reason a plan can be a fixture in the
harness rather than a live nameserver. Advancing an attempt is likewise an input, so
`RT-4`'s failover vectors are ordinary harness scenarios.

Open, and deliberately deferred to `RT-2`: whether a plan is rebuilt or resumed when a trunk's
policy version changes mid-transaction. It depends on the `AF-1` policy-version field and cannot be
settled before the trunk model is real.

**Trunks are stateful objects.** Above DNS, a trunk carries: concurrent-call and calls-per-second
limits, a circuit breaker fed by response-code and timeout history, retry budget, preferred
transports, number normalization ([number-normalisation](../specs/number-normalisation.md), RT-6)
and identity rules, and media policy hooks. Trunk selection is a typed routing module (see
`extension-framework`), not a DSL.

### RT-6: normalisation is data, and its vocabulary is closed

Number normalisation arrives as regexes embedded in route blocks — a form in which no rule can be
reviewed on its own, tested on its own, or diffed without reading the routing logic around it.
[number-normalisation](../specs/number-normalisation.md) makes it a **profile**: a named value
bound at exactly two points, ingress scope and trunk, and evaluated by a pure function of
`(profile, numbers)` that the harness drives with no message in sight.

The load-bearing decision is not that normalisation became declarative — it is *how small the
declaration language is allowed to get*. Four fields (Request-URI, To, From, P-Asserted-Identity,
and only the number position of each), four transforms (`replace_prefix`, `strip_leading_zeros`,
`add_prefix`, `ensure_prefix`), and one guard per field with two condition forms. Three
consequences follow, and each was worth more than the expressiveness given up:

- **Every profile terminates in a bounded number of steps** — 4 × (4 + 1) — because the three
  evaluation phases each read the previous phase's snapshot and nothing reads its own output.
  There is no fixed point to reach and no rule that can reference another.
- **The E.164 egress policy is a rule, not a code path.** A leading `+` reaches a carrier only
  because some `ensure_prefix` in a bound profile put it there, and `e164: global` is the
  15-digit ITU-T bound as a guard condition. Nothing in the platform assumes a `+` anywhere else.
- **"We need one more transformation" has a deliberately expensive answer**: a typed module under
  the [hook framework](../specs/hook-framework.md) with a declared manifest, or an external
  routing hook (EX-6) — never a new config keyword. The vision's non-goal is a routing DSL, and
  the way a repo acquires one is by adding a keyword per quarter to a config grammar that already
  works.

The two application points exist because `add_prefix` is deliberately not idempotent: a
double-applied profile is *visible* rather than silent, which is what makes "at most one profile
per direction, never chained" enforceable instead of aspirational. And normalisation never runs
on a request inside a dialog, on CANCEL, on an ACK for a non-2xx, or on REGISTER — RFC 3261 §9.1
and §17.1.1.3 require those to repeat the original Request-URI, `From` and `Call-ID` byte for byte
(§17.1.1.3 takes the ACK's `To` from the **response being acknowledged**, which normally differs
from the request's by the added tag, so `To` is not in that list), and
the address-of-record key belongs to [location-service](../specs/location-service.md) §3.

Where the work lands (rule 6): the **policy** is orchestration and stays here; two **syntax**
primitives are the kernel's — replacing a URI's user part losslessly, and structured access to a
`tel:` body, which sipx currently keeps opaque. The spec's §1 flags both for `CX-1`; the
implementation, which lands with the trunk model, is what waits on them.

### RT-7: a trunk's identity policy is a per-peer trust declaration

`P-Asserted-Identity` is not a header the platform decorates a message with. RFC 3325 §1 confines
the whole mechanism to a **Trust Domain**, and RFC 3324 §2.3 defines membership operationally: `B`
trusts `A` if and only if there is a secure connection between them *and* `B` holds configuration
saying `A` is a member. The second half is a configuration fact, and the grain that fact actually
has is the **peer** — this carrier, that SBC — never the cluster.

So the per-trunk policy [asserted-identity](../specs/asserted-identity.md) specifies is not a
convenience wrapper over a header. It is the deployment writing down where the edge of its trust
domain runs, one peer at a time, and RFC 3325 §1's `Spec(T)` checklist is what it has to answer.
Three of that checklist's eight items — how trust-domain membership is determined (item 4), what
happens when no `Privacy` header is present (item 5), and privacy handling for identity (item 8) —
have no defensible platform-wide answer, so the spec makes each of them a required field on the
trunk with **no default**. A trunk that does not say is a trunk that does not start.

Three decisions in that spec are worth reading even if the rest is skimmed:

- **Trust is one axis and assertion is another, because RFC 3325 splits them.** §5 governs whether
  a proxy *creates* an identity; §5 and §7 together govern whether an identity it holds *leaves*
  toward a given next hop. Collapsing the two into one "PAI on/off" switch is what makes "assert
  for regulatory trace, withhold from an untrusted peer" inexpressible — and that combination is
  the entire reason RFC 3325 §7 has a `Privacy: id` token at all. The spec keeps them apart
  (`A9`, `A14`), which is also what lets the vector table cover a product of axes rather than a
  path through a flowchart.
- **A `Privacy: none` header can never be overridden by trunk configuration.** RFC 3323 §4.2 and
  RFC 3325 §7 both make it a `MUST NOT`, and a per-trunk policy that could out-vote it would turn
  a compliance control into a suggestion (`A16`). The one thing it does *not* protect is a
  `P-Asserted-Identity` this platform received from a peer it does not trust: RFC 3325 §5 requires
  that one to be replaced or removed regardless, or `Privacy: none` would be a two-word identity
  spoof (`A6`).
- **The pipeline order is derived, not chosen.** A step runs *before* the trunk's normalisation
  profile if its output is a number whose shape is the trunk's business, and *after* if its output
  is a constant the RFC fixes byte for byte. Identity synthesis is the first kind and runs before;
  the anonymous-`From` form of RFC 5379 §5.1.4 is the second and runs after. One criterion, both
  placements, and no ordering knob anywhere (`A11`).

**The seam with normalisation, from this side.** [number-normalisation](../specs/number-normalisation.md)
`N32` says normalisation rewrites the number inside an *existing* URI and never creates a field,
and defers "whether and how `PAssertedIdentity` is created" here. The two halves meet at a point
rather than overlapping: by the time the egress profile runs, this spec has already settled whether
a `P-Asserted-Identity` exists on that branch and whose identity it names, so `NN-G-10`'s
`Skipped { GuardedFieldAbsent }` is the trunk's view of a decision already taken — a trunk that
declared `assert: never`, or a privacy gate that removed the field — rather than an open question.
The identity policy chooses *whose* number; the normalisation profile chooses *what shape* it takes.

Where the work lands (rule 6): considered for upstream — **no for the policy, and two candidate
rows for the syntax.** Parsing `Privacy` into the closed `priv-value` set of RFC 3323 §4.2, and a
typed `P-Asserted-Identity`/`P-Preferred-Identity` value list that enforces RFC 3325 §9.1's
one-or-two constraint, are protocol-generic and belong to the kernel on exactly the argument the
`Headers` surgery API was upstreamed on. Whether an identity is asserted, which one, and toward
whom has a *trunk* as its subject, and the kernel has no trunks. The spec's §2 flags both for
`CX-1`.

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
- **A regex per field for normalisation** (RT-6's obvious option, and the shape the requirement
  arrives in). Rejected: one regex per field is an interpreter admitted through the side door —
  unbounded in cost, unreviewable in a diff, and impossible to enumerate vectors for. The closed
  four-transform vocabulary covers the same ground as a table a reviewer can read, and what it
  cannot express is meant to be a module, not a longer pattern.
- **A caching layer here that snapshots into the sync `Resolver` trait** (RT-1's local option).
  Rejected once `T-17` landed: it would duplicate a cache the kernel now has, and a second cache
  with its own TTL handling is a second thing to be wrong about negative answers — the failure
  mode being a domain that briefly could not be asked about becoming one that is permanently
  unroutable.
- **A local `Target` type carrying address and transport.** Rejected: `Attempt` wraps the kernel's
  `Target` instead. Two address types eventually disagree about which host a TLS certificate must
  be valid for, and that disagreement is a silent downgrade rather than a visible error.

## Risks & open questions

- ~~Upstream vs local for the async resolver (RT-1's headline decision).~~ **Settled upstream**:
  sipx `T-17` shipped the async path, the shared TTL cache and the WS/WSS prefixes in `v0.4.0`.
  See *RT-1: the resolver decision* above and the [upstream ledger](../upstream.md).
- Circuit-breaker state scope: per-node or shared? Inclination: per-node with observability,
  since shared breaker state reintroduces cluster coupling for a heuristic. The concurrent-call
  cap has the same scope question — a per-node cap divided by node count drifts as the cluster
  scales — and must be answered alongside it (RT-2).
- How trunk configuration versions interact with the affinity token's `policy version` field —
  settled against AF-1 before RT-2 closes.
- The overload-collapse scenario (RT-3) needs load modeling the harness's current transport
  model does not express; CF-1 must take this as an input.
- Ingress normalisation rewrites the Request-URI *before* target determination, so it changes the
  key a location lookup uses (`NN-C-6`). A deployment that normalises on ingress but registers
  unnormalised addresses-of-record looks up a number nobody registered. The binding is per ingress
  scope and there is no default, which makes this a decision rather than a surprise — but whether
  the configuration loader should refuse the combination outright is open until RT-9 fixes the
  scope vocabulary.

## Acceptance / done

The union of RT-1 … RT-4: a RoutePlan consumed by the proxy driver with specified failover
semantics passing harness vectors (including DNS failover mid-transaction); trunks enforcing
CPS/concurrency with breaker transitions observable; overload control demonstrated in the
simulated cluster without collapse (M3 exit criterion).
