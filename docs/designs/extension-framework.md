# Design: Extension framework & RFC registry

**Status:** proposed · **Pillar:** Platform · **Epic:** `extension-framework` ·
**Stories:** EX-1 … EX-7

## Why

Extensions become declared modules over typed hook phases, never edits to the core.

The stated goal of the platform is to make the marginal cost of SIP extensions very low — while
being honest that literal zero-cost, 100%-coverage of every SIP RFC is impossible (RFCs update
and contradict each other; behavior cannot be generated from ABNF). The achievable target: syntax
additions become data, behavioral additions become isolated modules, and every deployment runs an
explicitly chosen, verified-compatible set. Without this epic, every extension lands as edits
across the proxy pipeline — the per-customer `if` jungle the vision forbids. With it, the M3
reachability work (Path, Outbound, GRUU, push, timers, 100rel) becomes a sequence of modules
instead of a rewrite. EX-1 must land before the proxy engine API hardens, because hook points are
part of that API.

## Approach

**Typed hook phases.** The proxy and registrar pipelines expose a fixed, ordered set of phases —
message parsed, request validated, before/after authentication, before/after registrar update,
before target resolution, targets resolved, before forward, response received, before response
forward, dialog-forming events — each with a typed context (what the module may read and effect
it may produce). No unrestricted "do anything" callback: a module's power is bounded by the
phases it declares.

**The module manifest.** Each extension declares: hooks used, dependencies and conflicts (by
capability, e.g. two modules both claiming ownership of `Supported: outbound` is a startup
error), methods/headers/option tags consumed and advertised, whether it needs transaction or
dialog-adjacent state, stored-state schema, and timers. The framework computes a valid extension
graph at startup — ordering, conflict detection, capability advertisement (`Supported`,
`Allow`) — so an invalid combination fails deployment, not a call at 3 a.m.

**The machine-readable RFC registry** (EX-2) is one data model feeding two consumers: codegen for
syntax artifacts (header names, compact forms, option tags, URI parameters, response codes,
event-package names — generated constants and, where practical, parsers) and the conformance
database (`conformance-harness`), so "what RFC 5626 defines" and "how much of it we implement"
share a single source. Registry entries record dependencies between RFCs and profile
compatibility. Where generated syntax belongs in the kernel (a new `HeaderName`), the generation
target is a sipx contribution — an [upstream](../upstream.md) decision per artifact (EX-4).

**Deployment profiles** (EX-5) are named, compatibility-checked module sets — CoreProxy,
ModernRegistrar, CarrierInterconnect, WebSocketUA — the deployable unit of "which SIP we speak."
A profile is verified at startup against the registry (dependencies present, conflicts absent,
IMS-restricted mechanisms only in profiles that assert the trust domain).

### EX-6: the async external routing hook

**The problem, concretely.** A deployment picks the egress carrier pool and the asserted caller
identity for every outbound INVITE with a **synchronous HTTP lookup on the request path**. For
some pools that lookup is the only selection mechanism, so it cannot simply be deleted. It
violates non-negotiable 2 twice: the forwarding decision reads a socket, and the decision
therefore cannot run under the deterministic harness at all. Its failure handling is *coded* —
a client error fails the call, a server error is tolerated — so the policy is invisible to review
and untestable without the service. EX-6 decides what replaces it.

**What replaces it: nothing new in the pipeline.** EX-1 already put the mechanism in place. The
`Query` effect is a **suspension, never a blocking call** ([hook-framework](../specs/hook-framework.md)
E4): a handler emits it and returns; the engine performs the lookup as driver I/O and re-invokes
the handler with the answer as a fact. That is the same effect-out/fact-in shape as the engine's
own `ResolveTargets`/`TargetsResolved` ([proxy-behavior §2](../specs/proxy-behavior.md)). What
EX-6 adds is not a phase and not an effect — it is the **declaration** that makes the suspension's
deadline and its failure handling data rather than code, and the two bounds that keep it off the
transaction's back. A module that consults an external oracle is an ordinary module; the thing
that was special about it — the blocking call — stops existing.

#### Where it attaches

The consult lands at **H7 `BeforeTargetResolution`**, and nowhere else. H7 already permits
`query` *and* `rewrite-target-query`, and it is the last point before the engine emits
`ResolveTargets` — so the answer can still choose the egress. The rest of the module reads the
answer as a published request-scoped fact (§5 class (a) `Scratch { published: true }`):

| Phase | The `external-route` module does | Effects (all already permitted by the H-table) |
|---|---|---|
| H7 `BeforeTargetResolution` | Emits one `Query` keyed on the request; on the answer, publishes the decision as a fact and narrows the target query to the chosen pool | `Query` → `Annotate` · `RewriteTargetQuery` (or `Reject`) |
| H8 `TargetsResolved` | Filters/orders the resolved candidates to the chosen pool — reading the fact, **never querying** (H8 permits no `query`, deliberately) | `SelectTargets` |
| H9 `BeforeForward` | Applies the asserted identity — reading the same fact, **never querying** | `PatchHeaders` |

**[sipx-clstr] rules:**

- **One consult per request, at H7.** A routing oracle MUST NOT be queried at H9. H9 fires *per
  branch*, so a query there multiplies the external load by the fork width and makes the egress
  decision depend on which branch asked first. The capability `external-route-decision` is
  `provides`d by such a module, which makes a second one a G2 conflict at startup rather than a
  race at 3 a.m.
- **Identity is asserted, not forged.** The identity a routing oracle influences is
  `P-Asserted-Identity` and `Privacy` (RFC 3325 §5 proxy behavior, RFC 3323) — the module's
  `headers_owned`. It is **not** `From`: the From tag is half the dialog identifier
  (RFC 3261 §12), which is why E2 makes `From` unpatchable, and a proxy that rewrites it is a
  B2BUA wearing a manifest ([services-b2bua](services-b2bua.md)). Per-trunk asserted-identity
  and privacy policy is RT-7's; the oracle *selects within* that policy, it does not replace it.
- **Closed world.** The answer selects from sets that already exist in configuration: a pool id
  MUST resolve to a configured trunk, an identity MUST satisfy the selected trunk's RT-7 policy.
  An answer naming an unknown pool resolves as `Malformed` (below), not as a new egress.
  Rationale: an external service that can name an arbitrary egress and assert an arbitrary
  identity is a toll-fraud path with an HTTP front end; the hook exists to *choose*, not to
  *configure*.
- **The module names a resource; the deployment binds it.** The manifest declares
  `Resource("route-oracle")`; configuration binds that name to an endpoint and a wire format, and
  the harness binds it to a scenario value. No module ever sees a URL, a socket or a status code —
  it sees the closed outcome enum below.

#### The suspension, in sans-IO terms

The decision stays a pure state machine; the whole external round trip is one input and one timer.

```text
H7 run, module k          engine                              driver
──────────────────        ───────────────────────────         ─────────────────
[.., Query(ref)]  ──────► Effect::Query { id, key }   ──────► resource client
                          arm QueryDeadline(id, after)         (HTTP, gRPC, …)
                          suspend the H7 run at k
                                    ▲                                │
     re-invoked with   ◄──── Input::QueryAnswered(id, gen, outcome) ◄─┘
     Fact(outcome)          or Input::TimerFired(QueryDeadline, id)
                          clear the deadline; resume the H7 run at k+1
```

**[sipx-clstr] rules:**

- **`Query` is the last effect of the invocation that emits it.** Effects preceding it in the
  returned list are applied first; anything the module wants to do with the answer happens in the
  re-invocation. A closed rule, checkable at runtime and in the vectors.
- **The deadline is a fired timer, not a stopwatch.** `QueryDeadline` is an **engine-owned**
  timer armed from the *declared* `timeout` when the `Query` is emitted, and cleared when the
  answer arrives. It is deliberately not a module timer: E6 forbids a module timer from altering
  a transaction's outcome, and this one does. Engine ownership keeps that rule intact and puts
  the deadline where every other outcome timer already lives (Timer C, proxy spec §9). The driver
  MAY also set a transport-level timeout as a backstop, but the **authoritative** deadline is the
  one the state machine armed — because that is the one the harness advances.
- **The outcome is decided once.** The `Query` carries a generation counter, bumped on arm and
  clear — the same discipline the kernel's `TimerQueue` uses and the harness reuses
  ([conformance-harness](conformance-harness.md)). A late answer arriving after the deadline fired
  is discarded at the input boundary; a deadline firing after an answer landed is discarded at
  pop. Without this, "same seed, same trace" is a coincidence.
- **Ordering is module order, not answer order.** Two query-emitting modules at the same phase run
  **serially**, in the G3 computed order, because EX-1 requires each module to see the message as
  patched by its predecessors. Dispatching them in parallel would make the decision depend on
  which answer returned first — deterministic under a seed, but not deterministic under
  *deployment*, which is the property that matters. The cost is that their deadlines sum, which is
  exactly why the budget below is a sum.
- **CANCEL during a suspension.** An upstream CANCEL while suspended is answered `200` (proxy spec
  C1), the context concludes `487` (C3), the deadline is cleared and the pending answer discarded.
  No branch exists yet, so there is nothing to cancel downstream.
- **Stateless mode.** E5 already covers it: a `Query` breaks applicability condition A3 and
  promotes the request to stateful **before anything is sent** (proxy spec S7). There is no
  "suspended stateless" state to design.

#### Why the transaction's timers are not coupled to the service

The normative claim, and it is testable: **no engine timer's duration, and no engine timer's
arming point, is a function of a query's latency.**

| Timer | Why the suspension cannot move it |
|---|---|
| Upstream INVITE retransmission (RFC 3261 §17.1.1.2, Timer A from T1) | E5 promotes the request to stateful, so an INVITE **server** transaction exists, and RFC 3261 §17.2.1 requires it to emit `100 (Trying)` unless a response is certain within 200 ms. The caller goes to `Proceeding` and stops retransmitting while we are suspended. The server transaction also absorbs retransmissions that do arrive — they never reach the engine, so they never start a second query |
| Timer C (proxy spec F11, ≥ 180 s per RFC 3261 §16.8) | Armed at **F11**, which is after F10 `Forward`. The query concludes at H7, before `ResolveTargets`, before any branch exists. Timer C's deadline is `t_forward + 180 s` and never `t_invite + 180 s` |
| Branch timers (kernel client transactions) | Nothing is forwarded until the query concludes; there is no branch to time |
| Session timers (RFC 4028), Timer B/F downstream | Downstream of F10, same argument |

What the suspension *does* consume is caller-perceived setup time, and that is bounded
explicitly rather than by accident:

- **Per query: `timeout`, default 500 ms.** T1's default (RFC 3261 §17.1.1.1, Appendix A) — chosen
  so that in the ordinary case the answer is in hand before the caller's first INVITE
  retransmission would even be due had the `100` been lost.
- **Per request: `hook_budget`, a profile value, default 2000 ms.** The sum of the `timeout`s a
  request can accrue along one path — H3 + H5 + H7 once each, plus H9's, since parallel branches
  overlap in wall-clock rather than summing. Checked at **startup** (a new G-rule, below), so a
  profile that could exceed it fails to boot instead of producing a slow call.
- **No retries inside the budget by default.** `retries` is declarable and capped at 1, and the
  deadline is a wall around the *whole* query, attempts included. Retrying into a call-setup
  budget multiplies load on a service that is by hypothesis already failing — the amplification
  RFC 5390 §3 states as an overload requirement and RFC 7339 exists to control.

#### Why capacity is not coupled either

Latency coupling is the visible half; capacity coupling is the half that takes the node down. A
blocking call makes in-flight concurrency `offered_rate × service_latency`, so a service that
slows by 20× costs 20× the resources for the same call rate, and the node sheds calls it could
otherwise have handled. Under the suspension model a waiting request is a response-context entry
in a map, but "cheap" is not "free", so the bound is declared:

**[sipx-clstr] rules:**

- **A per-resource in-flight cap and an explicitly sized queue.** `in_flight_max` (default 512
  per node) and `queue_max` (**default 0** — shed rather than queue). Overflow does not wait: the
  query resolves **immediately** as `Unavailable` and the declared disposition applies. A request
  that is going to fail should fail at the start of its budget, not at the end of it.
- **A circuit breaker on the resource,** fed by real outcomes, not by probes — the same reasoning
  routing-trunks gives for trunk breakers. Declared `failure_threshold` / `window` / `cooldown` /
  `half_open_probes`; while open, queries resolve `Unavailable` **without a driver call at all**,
  which is what makes a flapping service cost nothing. Per-node, matching RT-2's inclination for
  trunk breaker scope; EX-6 does not re-open that question.
- **A declared cache is how the common path never touches the network.** `cache { ttl,
  negative_ttl, max_entries }`, keyed on `(query id, key)`. The key MUST be derived only from the
  module's `headers_read` and published facts, so the same request produces the same key on every
  edge. `Declined` (an answer) is cacheable under `negative_ttl`; `Unavailable`, `Timeout`,
  `ServerError` and `Malformed` are **never** cached — the RFC 2308 §5 distinction routing-trunks
  already relies on for DNS: "no route" is an answer, "could not ask" is not, and caching the
  second turns a blip into an outage.
- **The cheapest query is the one never made.** Where the data allows it — a pool map that changes
  hourly, not per call — the right shape is not this hook at all: a module timer (E6, *module*
  class: periodic maintenance, `Query` and `Record` only) refreshes an owned store (§5 class (c))
  off any pipeline, and H7 reads it with no suspension. The async hook is for the irreducible
  per-call case. A design that uses it for data that could have been prefetched has bought a
  failure mode it did not need.

#### The declaration — timeout and failure handling as data

Acceptance: *declared per hook, not coded*. EX-1's `QueryDecl` gains the fields that make it so.
`on` is a **total** map over the outcome enum: a missing arm is a startup error, never a default.

```rust
pub struct QueryDecl {
    pub id: QueryId,
    pub target: QueryTarget,          // Store(name) | Resource(name)
    pub phases: &'static [Phase],
    pub timeout: Duration,            // arms QueryDeadline; ≤ the profile's hook_budget
    pub retries: u8,                  // 0 | 1, inside the same deadline
    pub on: &'static [(QueryOutcome, Disposition)],   // total over the enum — G7
    pub defaults: &'static [(DefaultRef, FactValue)], // resolved at startup — G8
    pub cache: Option<CacheDecl>,     // ttl, negative_ttl, max_entries
    pub limits: LimitDecl,            // in_flight_max, queue_max, breaker
}

enum QueryOutcome {
    Answered,       // a well-formed answer carrying a decision
    Declined,       // a well-formed answer that says "no route" — an answer, not a failure
    ClientError,    // the resource rejected our request (HTTP 4xx and equivalents)
    ServerError,    // the resource failed (HTTP 5xx and equivalents)
    Malformed,      // answered, but undecodable — or decodable and outside the closed world
    Timeout,        // the engine's QueryDeadline fired first
    Unavailable,    // never sent: breaker open, in-flight cap, queue full, resource unreachable
}

enum Disposition {
    Reject(Status),        // fail-closed — terminal at H7; Status from the closed set below
    Proceed(DefaultRef),   // fail-open — continue with a named, startup-validated default fact
    ProceedWithout,        // continue with no fact — only if the fact is declared optional
}
```

`Malformed` is the answer to non-negotiable 3: garbage from the network **decodes to an outcome**.
Decoding lives in the driver's resource client, it is fallible by signature, and its failure is an
ordinary declared arm — there is no path from a malformed answer to a panic, and the harness
proves it with a scenario rather than an assertion about intent.

#### Failure semantics — the fail-open/fail-closed decision, made here

**Decided: the framework is fail-closed by construction, and fail-open is available only as an
explicitly declared, startup-validated default.** Two rules carry that:

1. **There is no implicit default anywhere.** An outcome without a declared arm is a *startup*
   error (G7), not a runtime fallback. A framework-wide "on error, continue" would be exactly the
   coded policy EX-6 exists to delete, moved one level up and made invisible.
2. **Fail-open is not the absence of a decision; it is a decision made in advance.**
   `Proceed(default)` names a fact value — a fallback pool, a fallback asserted identity — that
   G8 checks exists and satisfies the closed-world rule. Fail-open never means "skip the module."
   For a pool whose only selection mechanism is the oracle, "route anyway" would mean routing to
   an egress nobody chose with an identity nobody asserted; and there is no target set to route
   *to*, so proxy spec §7 would conclude `480` in any case. The honest version of that is a
   declared `Reject`, which at least says so in the manifest.

**The shipped defaults** — what the profile catalog (EX-5) ships, overridable per deployment:

| Outcome | Default disposition | Why |
|---|---|---|
| `Answered` | — (the decision applies) | |
| `Declined` | `Reject(480)` | The oracle answered, and the answer is "no egress." `480 Temporarily Unavailable` is what proxy spec §7 already concludes for an empty target set; `403` or `404` are permitted where the deployment has definitive information (RFC 3261 §21.4.4 makes `404` a claim about the user existing, so it is not the generic case) |
| `ClientError` | `Reject(500)` | We sent something the resource refused. That is our defect or a contract drift, not a caller error, and it is not safe to guess an egress from it. `500 Server Internal Error` says so honestly. This is the one place the decision is *stricter* than the deployment's current behavior, which is deliberate: a 4xx today fails the call, and it should keep failing the call |
| `ServerError` | `Reject(500)` | **Changed from the deployment's current "tolerated."** Tolerating a server error means continuing with an egress the oracle did not choose — the fail-open the rules above reject. A deployment that genuinely has a safe fallback pool declares `Proceed(fallback_pool)` and gets the same tolerance, checked, visible in the manifest, and covered by a vector |
| `Malformed` | `Reject(500)` | Same as `ClientError`, from the other side of the wire |
| `Timeout` | `Reject(500)` | |
| `Unavailable` | `Reject(503)` | The one outcome that is genuinely "temporarily overloaded or under maintenance" in RFC 3261 §21.5.4's sense, and the one where an upstream element retrying elsewhere is the right response |

**The permitted `Reject` statuses are a closed set: `403`, `404`, `480`, `500`, `503`** — G8 checks
membership. **6xx is forbidden.** A 6xx is a claim about the user everywhere, not about one egress
path (RFC 3261 §21.6), and it makes an upstream forking proxy cancel every other branch
(§16.7; proxy spec R6). A routing-policy failure cannot know that, and a module must not be able
to say it.

**One interaction to carry forward:** proxy spec R8 turns a `503` *received from a branch* into
`500` upstream. `Reject(503)` here is locally originated, not a branch response, so R8 does not
apply to it — but when one platform node is upstream of another, the second node's R8 will rewrite
it. A deployment that wants `503` to survive that hop should say so when the topology exists; the
row is in *Risks* below rather than decided prematurely.

#### Startup validation — two new G-rules

| # | Rule | On violation |
|---|---|---|
| G7 | **The outcome map is total, and the budget holds.** Every `QueryDecl.on` covers all seven `QueryOutcome` variants exactly once; every declared `timeout` is > 0 and ≤ the profile's `hook_budget`; and Σ `timeout` over the query-emitting phases of one request path ≤ `hook_budget` | Startup error naming the module, the query id and the missing outcome or the sum |
| G8 | **Dispositions resolve.** Every `Proceed(DefaultRef)` names a `defaults` entry that exists, type-checks against the fact, and satisfies the closed-world rule (a pool that is a configured trunk, an identity the trunk's RT-7 policy admits); every `Reject(Status)` is in the closed set and is not 6xx; `ProceedWithout` appears only where the consuming `Scratch` is declared optional | Startup error naming the module, the query id and the offending disposition |

Both follow EX-1's discipline exactly: an invalid combination fails deployment, never a call.

#### Harness scenarios

The external service enters the simulation as a **`ResourceModel`** — a declarative value beside
`LinkPolicy` and `Fault` ([conformance-harness](conformance-harness.md)), holding a latency
distribution and an outcome schedule, drawing from the named RNG stream
`"resource:route-oracle"` so adding it reshuffles nothing else. No HTTP exists in the harness: the
sim driver's resource client is the `ResourceModel`, the same way the sim driver stands in for the
tokio endpoint driver. That is the payoff of the whole design — *slow*, *failing* and *flapping*
become scenario data, and every one of these runs in virtual time with no wall-clock sleep.

| Scenario | Asserts |
|---|---|
| `external_route_answer_under_deadline` | Answer at `timeout − ε`: the decision applies, `RewriteTargetQuery` carries the chosen pool, H9 patches `P-Asserted-Identity`, one `Query` effect in the trace |
| `external_route_timeout_applies_declared_disposition` | Answer at `timeout + ε`: `QueryDeadline` fires first, the declared arm applies, **and the late answer is discarded** — exactly one disposition in the trace |
| `external_route_timer_c_is_not_moved` | Timer C's queue entry is at `t_forward + 180 s` for both a fast and a slow oracle; the two runs differ in when F10 happens and not in Timer C's duration. The literal Acceptance-3 assertion |
| `external_route_upstream_does_not_retransmit` | With the oracle at 5× the deadline, the caller receives `100` and sends no INVITE retransmission; and when one is injected anyway, the server transaction absorbs it and **no second `Query` effect appears** |
| `external_route_client_error_fails_closed` | `ClientError` → `Reject(500)`, no `Forward` effect, no `ResolveTargets` |
| `external_route_server_error_proceeds_with_default` | Same scenario, a manifest declaring `Proceed(fallback_pool)`: the call completes over the fallback trunk and the trace records which default was used |
| `external_route_flapping_opens_breaker` | Alternating `Answered`/`ServerError` at the declared rate: the breaker opens at the threshold; while open, **zero** resource effects appear in the trace and every query resolves `Unavailable` within one event; it half-opens after `cooldown` |
| `external_route_slow_service_costs_no_capacity` | An open-loop source at fixed cps against an oracle at 10× the deadline: goodput stays flat (every call concludes by its declared disposition inside `hook_budget`), concurrent suspensions stay ≤ `in_flight_max`, and overflow sheds as `Unavailable` rather than growing the queue. The direct counter-model to the blocking call |
| `external_route_malformed_answer_is_an_outcome` | Truncated, oversized and out-of-closed-world answers each resolve as `Malformed` and apply the declared arm; the node does not panic (non-negotiable 3) |
| `external_route_cancel_during_suspension` | CANCEL while suspended: `200` to the CANCEL, `487` upstream, deadline cleared, pending answer discarded, no `Forward` |
| `external_route_is_deterministic` | The same scenario at the same seed produces a byte-identical trace with the `ResourceModel` in the loop, including the interleaving of answers and deadline fires |

#### Upstream boundary

**Considered for upstream: no, cluster-specific** — the suspension's declaration surface
(`QueryDecl`, the outcome taxonomy, the dispositions, G7/G8) is bound to this platform's hook
manifest and proxy pipeline, and the kernel has no hook phases, no manifest and no notion of a
routing-policy oracle. Two protocol-generic pieces this design leans on are already upstream and
are consumed rather than re-made. Both were read in the pinned kernel (`v0.7.0`) rather than
assumed, because this ledger has twice recorded a row that was believed instead of checked:

| What the design leans on | Where it actually is | Ledger standing |
|---|---|---|
| The INVITE server transaction emits `100 (Trying)` when the TU is still thinking, and absorbs upstream retransmissions so they never reach us mid-suspension | `sipx-sip`'s sans-IO `ServerTransaction`: `new` arms `Timer::Trying100` for INVITE citing RFC 3261 §17.2.1 (`transaction/server.rs:73`), `on_timer` sends the `100` only if the TU has not answered (`server.rs:200`), and `on_request` swallows a retransmission — resending the last response or nothing at all, but in either case "the TU hears nothing, which is the point of the layer" (`server.rs:100`) | No row, and none is needed: the ledger's standing decision is that the proxy transaction driver is built **here**, directly over this sans-IO `TransactionLayer` |
| A generation-counter `TimerQueue` generic over its instant, so `QueryDeadline` can be armed from virtual time | Landed in sipx **`v0.7.0`** as `TimerQueue<K, I = Instant>` — the ledger's *timer queue drivable from a virtual clock* row, which carries no sipx story because the kernel closed it directly | That row, not `X-14`. `X-14` is the row that generalized the queue over its **key** and explicitly **did not** close this gap — it left the instant as `tokio::time::Instant`, which a virtual clock cannot construct |

Nothing new is filed against the [ledger](../upstream.md) by this story.

#### What this hands to the spec

EX-6 is a design decision, not a normative text. Making it normative is a follow-up story against
[hook-framework](../specs/hook-framework.md), which EX-1 closed — the deltas, named so the next
agent does not have to rediscover them:

- **§4** — `QueryOutcome` and `Disposition` as closed enums; E4 gains the "`Query` is the last
  effect of its invocation" and "the outcome is decided once" rules; a `QueryDeadline` timer class
  stated as **engine-owned**, adjacent to E6 and explicitly not a module timer.
- **§6** — `QueryDecl` gains `timeout`, `retries`, `on`, `defaults`, `cache`, `limits`; the
  `external-route` module joins the §9 manifest cast.
- **§7** — G7 (total outcome map, per-query and per-request budget) and G8 (dispositions resolve;
  the closed status set; no 6xx).
- **§8** — `hook_budget` is a profile value, so EX-5's catalog carries it.
- **§9** — vectors `HF-9` … `HF-13` for the two G-rules: missing outcome arm, budget exceeded,
  unresolvable default, a 6xx disposition, an out-of-closed-world default.

## Alternatives considered

- **Free-form middleware chain (onion model).** Rejected: unordered, untyped interception makes
  extension interaction emergent behavior; SIP extensions interact through headers and state, and
  the framework must see those interactions to check them.
- **Generate behavior from ABNF/registry.** Rejected as impossible in general — normative
  behavior (state machines, timer rules, trust requirements) is prose; the registry generates
  syntax and *tracks* behavior.
- **Everything enabled, always.** Rejected: some RFCs define alternative or trust-domain-bound
  behavior; "all extensions on" is not a coherent SIP profile.

*On the async external hook (EX-6):*

- **Keep the blocking call, just give it a short timeout.** Rejected: it is I/O inside the
  forwarding decision, so the decision cannot run under the harness at all (non-negotiable 2) —
  and a *bounded* blocking call still couples capacity to latency, because in-flight concurrency
  is `offered_rate × service_latency` whatever the bound is.
- **A new dedicated `ExternalConsult` phase.** Rejected: H7 `BeforeTargetResolution` already
  permits `query` and `rewrite-target-query` at exactly the right point — after route
  preprocessing, before `ResolveTargets` — and a phase set that grows whenever an extension wants
  something is the free-form middleware this epic rejected, arriving by installments. A new phase
  would also need its own ordering, effect set and validation rules, all of which H7 has.
- **Dispatch same-phase queries in parallel.** Rejected: EX-1 requires each module to see the
  message as its predecessors patched it, so the decision would depend on which answer returned
  first — reproducible under a seed, but not reproducible under deployment, which is the property
  the north star is about. Serial execution costs a summed budget, which is why the budget is a
  startup-checked sum.
- **Fail open globally on any oracle failure** (the current downstream behavior for server
  errors). Rejected: continuing without the answer means an egress nobody chose and an identity
  nobody asserted, and for a pool whose only selection mechanism *is* the oracle there is no
  target set to continue with — proxy spec §7 concludes `480` anyway. Tolerance stays available,
  as a declared `Proceed(default)` naming a checked fallback.
- **Forward optimistically and correct afterwards.** Rejected: a forwarded INVITE cannot be
  unforwarded. Correcting means CANCEL plus a second attempt, which doubles egress load and has
  already shown the wrong carrier the caller's asserted identity.

## Risks & open questions

- Hook-phase granularity: too coarse forces modules back into core edits, too fine freezes the
  pipeline's internals into API. EX-1's central judgment call.
- Whether modules are compiled-in (feature flags, static graph) or dynamically assembled at
  startup from one binary. Inclination: one binary, runtime-selected, statically compiled — no
  dynamic loading.
- Registry format (likely declarative data checked into the repo, versioned with the code);
  how registry versions pin against sipx kernel versions.
- Where a module's dialog-adjacent state may live: the manifest lets a module declare state
  needs, but invariant 5 (state rides the message) bounds what that can mean on the hot path —
  EX-1 must constrain declared state to off-hot-path stores or token-carried facts, or the
  invariant leaks.
- **`Reject(503)` across a platform-to-platform hop (EX-6).** Proxy spec R8 rewrites a `503`
  *received from a branch* into `500`, so when one node sits upstream of another, a locally
  originated `503` becomes `500` one hop later and the failover signal is lost. R8 is right about
  branch responses; whether a locally originated `503` needs a distinguishing marker is a question
  for when the multi-node topology exists (DP-2, M2), not one to pre-decide here.
- **Whether `hook_budget` should shrink under load (EX-6).** A fixed 2 s budget is the wrong
  budget at saturation — the right one adapts, which is precisely the load-shedding conversation
  RT-3 owns. Deferred there rather than half-answered here.
- **Resource-model calibration (EX-6).** Until the `ResourceModel`'s latency and failure
  distributions are calibrated against a real oracle, the capacity scenarios prove the *shape* of
  the claim (goodput flat, suspensions bounded) and not its absolute numbers — the same caveat
  CF-1 already records for the node service model.
- **Breaker scope for external resources (EX-6)** is the same per-node-versus-shared question
  routing-trunks has open for trunk breakers (RT-2). EX-6 adopts per-node to match; if RT-2
  settles it the other way, both move together.

## Acceptance / done

The union of EX-1 … EX-5: `docs/specs/hook-framework.md` and `docs/specs/rfc-registry.md`; the
hook runtime executing a declared module graph in the harness; codegen producing at least the M3
syntax set from registry data; profile validation rejecting a deliberately conflicting set in a
test; and the demonstration that a syntax-only RFC lands as a registry entry with no hand-written
parser code.

EX-6 adds one more: an `external-route` module whose oracle is a harness `ResourceModel`, proving
the timer claim (Timer C armed at `t_forward + 180 s` regardless of oracle latency), the capacity
claim (goodput flat and suspensions bounded against a 10×-slow oracle), and the declared failure
semantics — the same scenario passing and failing purely by which `on` arms its manifest declares.
