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

### EX-7: carrier quirk profiles

**The problem, concretely.** Peers differ in ways no RFC settles: one wants a security-mechanism
header present on every request it sees, another rejects an offer that leaves out a direction
attribute the grammar makes optional. Accommodated in code, each of these becomes a branch in the
routing path — the per-customer `if` jungle the vision forbids, and the shape a live deployment is
in today, where one peer's header requirement and an SDP rewrite sit as an inline domain test inside
a routing script. EX-7 makes accommodating a peer **configuration plus a vector**. The requirement
is the mechanism and not any one peer: more will follow, and the design is worth nothing if the
second one needs a patch.

The harder half is the negative requirement. A configuration language that can express an arbitrary
message transformation is the routing script again with better syntax — the `if` jungle relocated,
now unreviewable *and* unversioned. So the vocabulary is bounded, deliberately and checkably, and
§*The bound* below states the property in a form a reviewer can verify against the types rather than
believe about the semantics.

**Nothing new in the pipeline — same posture as EX-6.** A quirk profile needs **no new phase and no
new effect**: it is `PatchHeaders` and `RewriteBody` at phases that already permit them
([hook-framework](../specs/hook-framework.md) §3 H9/H11, §4). One spec change *is* required, and it
is not in the H-table — the `SyntaxDecl` body claim has to distinguish replacing a body from writing
a named field of one (§*Claiming SDP* below). It is named for
[EX-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/EX-8-make-the-async-query-declaration-normative.md)
with the rest of the deltas rather than edited into a closed spec here.

**One module, many profiles.** Modules are statically compiled and a profile selects a subset
(hook-framework §2), so a quirk cannot *be* a module: adding a peer would be a code change, which is
the thing this story exists to remove. A **single** compiled module, `carrier-quirks`, holds the
entire vocabulary in its manifest; a **quirk profile** is a data value it interprets; a **binding**
in configuration says which peers get which profiles. The module's power is therefore fixed at
compile time and reviewed once, and what a deployment changes is only which of that fixed power is
exercised, and where.

#### The profile — data, and only data

```rust
pub struct QuirkProfile {
    pub id: ProfileId,                       // lowercase-kebab, unique in the shipped catalogue
    pub version: Version,                    // semver — a profile change is a versioned change
    pub headers: &'static [HeaderRule],
    pub sdp:     &'static [SdpRule],
    pub requires_media: &'static [MediaAssertion],   // asserted at startup, never applied
}

pub struct HeaderRule { pub header: CatalogHeader, pub on: MessageClass, pub op: HeaderOp }
pub struct SdpRule    { pub scope: SdpScope, pub field: CatalogSdpField,
                        pub op: SdpOp, pub on: MessageClass }

pub enum MessageClass {                      // a message *class*, never message content
    Request  { methods: &'static [Method] },
    Response { to_methods: &'static [Method], classes: &'static [StatusClass] },
}

pub enum HeaderOp {
    Set(HeaderValue),          // the header has exactly this value; existing values removed
    AddIfAbsent(HeaderValue),  // present only where the message carried none
    Remove,                    // the header is not present
}

pub struct HeaderValue {
    pub base:   ValueLeaf,
    pub params: &'static [(ParamName, ValueLeaf)],   // depth stops here — params do not nest
}

pub enum ValueLeaf {
    Literal(&'static str),     // fixed in the profile; parsed at startup against the row's ABNF
    TrunkConfig(ConfigKey),    // a typed value from the bound trunk's configuration
    DomainConfig(ConfigKey),   // a typed value from the bound domain's configuration
}

pub enum SdpScope    { Session, Media(MediaKind) }
pub enum MediaKind   { Audio, Video, Application, Image, Text }
pub enum SdpOp       { EnsureExplicit, Set(ValueLeaf) }
pub enum CatalogHeader   { SecurityClient, SecurityServer, SecurityVerify,
                           PEarlyMedia, PChargingVector, PChargingFunctionAddresses }
pub enum CatalogSdpField { Direction, SessionName }
pub enum MediaAssertion  { Srtp }   // satisfied by any SrtpPolicy but Disabled — media-relay §13.1
```

`CatalogHeader` is deliberately **not** `HeaderName` and `CatalogSdpField` is deliberately not a
string: the set of things a quirk can write is a closed enum in code, so "which headers may a
profile touch" has a type-level answer rather than a documented convention.

**[sipx-clstr] rules:**

- **There is no `Fact` leaf, on purpose.** A value drawn from the message would make the transform a
  function of the message — a condition, wearing a data structure. A quirk that genuinely needs a
  message-derived value is a module with a manifest, not a profile.
- **A profile does not know where it applies.** It carries no selector, no domain pattern, no peer
  match — and, since EX-10, **no `overrides`**: it names no other profile it beats, because which
  profiles it is composed with is a fact about a binding and not about the profile. Applicability and
  contest resolution are both decided entirely by **binding** (below), before any rule runs. This is
  what keeps "which transform did this peer get" answerable from configuration alone.
- **`Remove` reaches catalogue rows only.** Suppressing `P-Early-Media` toward a peer that
  mishandles it is a quirk; deciding which of *our* application headers may leave the platform is
  RT-5's per-trunk allowlist, a default-deny filter over a prefix. Two mechanisms with two
  defaults — see the table below for why collapsing them would give one of them the wrong one.
- **A quirk transforms; it never decides.** `carrier-quirks` declares exactly four effects —
  `PatchHeaders`, `RewriteBody`, `Annotate` (one observability fact), `Record`. It has no `Reject`,
  no `RewriteTargetQuery`, no `SelectTargets`, and **no `Query`**, so it never suspends and never
  draws on EX-6's `hook_budget` (G7). A quirk that wants to reject a message is a policy module.

#### The bound — stated so it can be checked rather than believed

A quirk profile is a **finite set of assignments, not a program**. Four properties carry that, and
each is checkable against the *types*, which is the point — a semantic argument about what
configuration "should" express decays; a grammar does not.

| # | Property | What it forbids | How a reviewer checks it |
|---|---|---|---|
| B1 | **Non-recursive grammar, fixed depth** | `Concat(Value, Value)`, `If(Cond, Rule, Rule)`, `Map(Rule, …)`, nested parameters | No type above names itself, directly or through another. Adding a self-referential constructor is a one-line diff, and it is the line where this became a language |
| B2 | **No environment** | variables, bindings, names a profile introduces, captures | Every name a rule writes is a `CatalogHeader`/`CatalogSdpField` variant or a `ConfigKey` declared by the trunk/domain schema (DP-1). There is nowhere to put a name |
| B3 | **No condition inside a rule** | `if header X contains Y then …`, regex match, predicate on content | `MessageClass` ranges over method, status class and direction — enumerable message *classes*. Nothing in the grammar reads a header value or a body |
| B4 | **No iteration, no positional addressing** | loops over headers or `m=` sections, "the second `m=` line", indices | `SdpScope` addresses a media **kind**; there is no index type and nothing to iterate |

Three consequences follow, and they are the properties the vectors assert:

- **Evaluation is total and terminates in O(rules).** There is no construct that can fail to
  terminate and none that can fail to produce a value, so a profile cannot make a call hang or a
  node panic (non-negotiable 3). Every leaf is validated at startup; a malformed value fails the
  boot, never the call.
- **Application is idempotent.** Each op is idempotent by construction (`Set`, `AddIfAbsent`,
  `Remove`, `EnsureExplicit`), and targets are disjoint (G10), so applying a bound set twice equals
  applying it once. A retransmission that re-runs H9 cannot accumulate headers.
- **Application is order-independent — confluent.** Because the composed rule set is disjoint on
  targets, the result does not depend on the order profiles or rules are applied in. "Several
  profiles may apply" therefore creates no emergent behaviour, which is the whole reason the
  disjointness check is a startup error rather than a precedence rule. The one escape does not
  weaken this: an override deletes the losing rule at **composition** (G13), so the set that reaches
  a message is disjoint by construction and nothing at runtime consults a precedence.

**What the vocabulary cannot express, and where the need goes instead.** This table is the design;
the grammar above is its consequence.

| Not expressible | Why it stays out | Where a real need goes |
|---|---|---|
| Regular expressions, capture groups, substitution | The single largest source of unreviewable config. A regex over a header is a program with no type | A catalogue row, or a module |
| String concatenation, templating, arithmetic | B1 — every value is a leaf | A config key holding the finished value |
| Conditions on message content | B3. Two peers with the same binding would receive different transforms, so "what does this peer get" would stop being answerable from configuration | A distinct profile on a distinct trunk; or a module |
| Reading one header to write another | Makes rules order-dependent and destroys confluence | A published fact, i.e. a module |
| Introducing a header name that is not a catalogue row | The escape hatch that unbounds everything at once | A catalogue row (spec + vector), or an RFC registry entry via EX-2 |
| Anything on an option-tag-bearing header (`Require`, `Supported`, `Proxy-Require`, `Unsupported`) | `Supported`/`Allow` are **derived**, never hand-listed (G4). A quirk that could append a tag would defeat exactly the derivation G4 exists for, and would advertise behaviour no module implements | A module — a negotiation is behaviour, and behaviour is a module |
| Removing application headers wholesale on egress | A different mechanism with a different default: RT-5's per-trunk allowlist is **default-deny** over a prefix; the quirk catalogue is **default-absent** over an enumerated set. Neither can express the other, and collapsing them would give one of them the wrong default | [RT-5](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/RT-5-implement-per-trunk-egress-header-allowlist.md) |
| Engine-owned headers — `Via`, `Route`, `Record-Route`, `Max-Forwards`, `From`, `To`, `Call-ID`, `CSeq`, `Content-Length` | E2. They carry RFC 3261 §16 correctness and the token placement | The core |
| A header another selected module owns | G2 exclusivity, for free — see the catalogue invariant below | That module |
| Transport addresses, `m=` proto, `a=crypto`, ICE candidates, adding or removing whole media sections | Media control's, and an offer/answer change rather than a quirk | ME-4/ME-5, ME-6, or [services-b2bua](services-b2bua.md) |
| Overwriting a value an endpoint negotiated | P3 below | Nothing — this one has no legitimate form on the proxy path |
| Rejecting, routing, forking, cancelling, or timing out | A quirk is a transform, not a decision | H2/H7 modules; EX-6 for an external decision |

#### The catalogues, and how they grow

The two catalogues are the bound made concrete. **Adding a peer is a config change plus a vector;
adding a new *kind* of quirk is a catalogue row, a spec change and a vector.** That asymmetry is the
design: the common case is data, and widening the vocabulary is deliberately not.

**Catalogue invariant.** A row is admissible only while no other module in any shipped profile
claims that header or field. This needs no new machinery — G2's exclusive-claim rule already makes a
contested claim a startup error naming both modules (vector QP-G-10), so the catalogue is bounded
from below by the rest of the profile as well as from above by review.

**Header catalogue, v1.** Small on purpose.

| Row | Header(s) | Normative reference | Why it is a quirk rather than a module |
|---|---|---|---|
| `SecurityClient` · `SecurityServer` · `SecurityVerify` | `Security-Client`, `Security-Server`, `Security-Verify` | RFC 3329 §3 (header syntax, option tag `sec-agree`) | **Injection only.** The peer wants the headers present with an agreed value. The RFC 3329 *negotiation* — the `sec-agree` option tag, comparing a server list against a client list, the client re-issuing the request under the chosen mechanism (§2, §5) — is behaviour, and the vocabulary above cannot express behaviour. That is a module with a registry entry, and drawing the line here is the bound doing its job: it tells you the moment you have left configuration |
| `PEarlyMedia` | `P-Early-Media` | RFC 5009 (the `P-Early-Media` header field) | A per-peer declaration about early media, carried out of band; no state, no negotiation |
| `PChargingVector` · `PChargingFunctionAddresses` | `P-Charging-Vector`, `P-Charging-Function-Addresses` | RFC 3455 §§4.6, 4.5 | Opaque per-peer values sourced from trunk configuration. The platform neither reads nor derives them |

**SDP field catalogue, v1.** Two rows, and the reason there are only two is P3.

| Row | Element | Reference | Op set (total, checked by G10) |
|---|---|---|---|
| `Direction` | `a=sendrecv` / `a=sendonly` / `a=recvonly` / `a=inactive`, at session or media scope | RFC 8866 §6.7 | `{ EnsureExplicit }` — and *only* that |
| `SessionName` | `s=` | RFC 8866 §5.3 | `{ Set(ValueLeaf) }` |

**[sipx-clstr] rule P3 — a quirk may materialize a default or carry an out-of-band value; it may
never overwrite a value an endpoint negotiated.** Every op in the SDP catalogue is either
*default-materializing* — it writes the value the RFC already assigns when the element is absent,
and is a **no-op when the element is present** — or scoped to an element no endpoint negotiates
(`s=`, which RFC 8866 §5.3 requires to exist and permits to be `-`).

This is what makes `Direction` safe to catalogue and `Set(Direction)` unsafe. RFC 8866 §6.7 already
assigns `sendrecv` when no direction attribute appears, so `EnsureExplicit` writes what the peer
would have inferred and changes no negotiated state. A rule that could *force* a direction would be
able to turn a held call active (RFC 3264 §8.4 puts hold in exactly this attribute) or contradict an
answer's §6.1 consistency requirement — a correctness defect with a configuration file in front of
it. The vocabulary does not have that op, so no profile can contain it and no review can miss it.

*Candidates the catalogue does not yet admit, with what each would have to argue:*

| Candidate | The argument it owes |
|---|---|
| `a=ptime` / `a=maxptime` | An endpoint's stated preference (RFC 8866 §§6.4–6.5), so P3 admits at most an `EnsureExplicit` against a declared default — and there is no RFC default to materialize |
| `b=AS:` | Same shape; and a bandwidth hint interacts with what the media plane is asked to carry, so it needs ME-6's agreement before it is a quirk at all |
| `a=rtcp-mux`, `a=rtcp-rsize` | Changes what the media plane must do (RFC 5761, RFC 5506) — ME-4/ME-6's, not a quirk's |
| `User-Agent` / `Server` | Header privacy is RFC 3323's, and PX-8 settled that it lives in [services-b2bua](services-b2bua.md) and RT-5, not the proxy path |

#### Binding — which peers, and several at once

**Configuration binds a profile to a trunk or a domain; the profile never binds itself.**

```toml
[trunks.trunk-a]                       # RT-1's trunk object; RoutePlan carries the trunk it belongs to
quirks = ["sec-agree-headers", "sdp-direction-explicit"]

[trunks.trunk-a.quirk_config.sec-agree-headers]
security_client = "sdes-srtp;mediasec"     # parsed at startup against the row's ABNF
security_verify = "sdes-srtp;mediasec"

[domains."example.net"]
quirks    = ["sdp-direction-explicit"]

# A contested target, resolved where the contest is. Illustrative: the v1 catalogue's two profiles
# write disjoint targets, so a contest needs a third profile before it can arise at all.
[trunks.trunk-b]
quirks = ["house-charging-headers", "peer-b-charging"]   # both write P-Charging-Vector

[[trunks.trunk-b.quirk_overrides]]
target = { header = "P-Charging-Vector", on = "request:INVITE" }
winner = "peer-b-charging"
```

The SDP form of `target` names a scope and a field instead of a header —
`{ sdp_scope = "media:audio", sdp_field = "Direction", on = "response:INVITE:2xx" }` — and is
otherwise the same entry.

**[sipx-clstr] rules:**

- **Two attachment points, and they are the two the platform already has.** A **trunk**
  ([routing-trunks](routing-trunks.md)) is the egress peer; a **domain** is the registering side.
  There is no third, and no free-form selector — a selector is a condition (B3) with a config file
  around it.
- **Several may apply, and the union is disjoint.** The composed rule set for one attachment is
  every trunk-bound and domain-bound profile's rules together. G10 requires the composition to be
  **disjoint on targets**. Two *distinct* applicable profiles writing the same target is a **startup
  error naming both**, not a precedence rule. One profile bound at both attachment points — as
  `sdp-direction-explicit` is above — contributes each of its rules once and contests nothing; a
  contest needs two profiles.
- **A target is elementary, so disjointness is overlap and not tuple equality.** A target is one
  catalogue row at one *elementary* message class: `(CatalogHeader, ElementaryClass)` or
  `(SdpScope, CatalogSdpField, ElementaryClass)`, where an elementary class is one method for a
  request and one `(method, status class)` pair for a response. A rule's `MessageClass` ranges over
  a **set** of those, and two rules contest on every elementary class their sets share. Stated as
  tuple equality instead, `Request { methods: [INVITE, REGISTER] }` and `Request { methods: [INVITE] }`
  would be different targets and the disjointness check would be defeated by widening a method set —
  and an override could not name what it was conceding.
- **The one escape is a declared override, and it lives in the binding rather than in a profile.**
  A contest is a property of the *composition at one attachment point*, and only the binding knows
  the composition. A profile cannot, and must not: "a profile does not know where it applies" is what
  keeps "which transform did this peer get" answerable from configuration alone, and a shipped,
  versioned catalogue profile naming one deployment's other profiles is not a thing that can be
  written down. So the escape is a binding-level entry naming two different things — the contested
  **target**, and the **winning profile id**. They are not interchangeable: a profile id says who
  wins, and only a target says what is won. It is also **not directional**: the commonest contest is
  between two profiles bound to the same trunk, which a trunk-beats-domain escape could not reach at
  all, and which does not depend on the trunk-bound and domain-bound sets ever intersecting (a
  question this design asserts more thinly than it states — see *Risks*).
- **An override deletes the losing rule at composition, before any message is evaluated.** The
  losing profiles' rules for that one target are dropped from the composed set at **startup**, so the
  set that reaches H9/H11 is disjoint by construction and the idempotence and confluence claims above
  keep their premise. This is not a runtime precedence rule; there is still no such thing. The
  resulting composed set, with the overrides that shaped it named, is exported at startup the way
  [media-relay](../specs/media-relay.md) §13.5 exports the effective per-trunk policy, so "why did
  that peer not get that header" is answerable from configuration rather than from a capture.
- **An override that is not resolving a live contest fails the boot** (G13). Its target must actually
  be contested at that attachment point, and its winner must be one of the profiles contesting it.
  That is what answers the failure mode precedence has, and it is EX-6's rule 1 restated: an implicit
  "more specific wins" is a policy nobody wrote down, and its failure mode is a profile that silently
  stops applying long after anyone remembers binding it. A declared override that has outlived its
  contest — because the other profile was unbound, or its rules changed — is exactly that failure
  arriving quietly, so the node refuses to boot instead.
- **An override deletes rules; it never touches an assertion.** `requires_media` is not a rule and
  has no target, so no override can name one — structurally, not by convention. A trunk-bound profile
  therefore cannot override away a domain-bound profile's G12 violation, and a profile whose every
  rule an override has deleted is still *bound*, so G11 still checks its assertion against the trunk
  (QP-G-15). The escape is scoped to the thing it exists for — two writers on one target — and cannot
  reach a boot check.
- **Closed world.** Every bound profile id is in the shipped catalogue; every trunk and domain named
  exists; every `ConfigKey` is declared by the DP-1 schema and type-checks; every literal parses;
  every override names a target and a winner that exist. All at startup — an invalid combination
  fails deployment, never a call.

#### Where it runs — one egress point per direction

| What | Phase | Effect | How the binding is known |
|---|---|---|---|
| The ingress binding, published once | H2 `RequestValidated` | `Annotate` (fact `quirks:ingress`) | The source peer is known to the engine; the fact is class (a) `Scratch { published: true }` |
| Headers and SDP fields on a request leaving toward a peer | H9 `BeforeForward` | `PatchHeaders` · `RewriteBody` | The branch's target, whose `RoutePlan` carries its trunk (RT-1) |
| Headers and SDP fields on a response leaving toward a peer | H11 `BeforeResponseForward` | `PatchHeaders` · `RewriteBody` | The `quirks:ingress` fact — the response goes back the way the request came |
| Which profiles applied | any of the above | `Annotate` · `Record` | — for CDRs (DP-6) and for answering "why did that peer get that header" |

**[sipx-clstr] rules:**

- **Egress only. A quirk never patches an inbound message.** There is no phase at which it could:
  H1–H4 and H7 permit no `PatchHeaders`, and that is deliberate rather than an oversight to route
  around. Patching on ingress would mean the engine's own validation (proxy spec V2–V6),
  authentication and route preprocessing ran against a message a module had already changed, so the
  pipeline's guarantees would become conditional on module order. What deployments actually want
  from "inbound normalization" — *do not relay this header onward* — is already reachable, because
  H9 patches the outgoing draft. What is genuinely not reachable is a peer whose messages break our
  own validation, and that is a core concern with a `420`/`400` answer, not a quirk.
- **One egress point per direction.** H6 `AfterRegistrarUpdate` also permits response
  `patch-headers`, and `carrier-quirks` deliberately does **not** subscribe it: two subscriptions on
  the registrar response path would give the same header two writers and re-open by the back door
  the disjointness the whole composition rule rests on. H11 fires on the locally originated
  registrar final as well, so nothing is lost.
- **H9 is per branch, and that is correct here.** Unlike EX-6's oracle, a quirk is a pure function
  of `(binding, message class)` with no external cost, so evaluating it per branch is not only safe
  but required — a fork can have two branches on two trunks wanting two different header sets.

#### Claiming SDP — the one thing that needs a spec change

`media-anchor`'s `RewriteBody` is a **whole-body replacement**: the relay takes complete SDP and
returns rewritten SDP as opaque bytes end to end ([media-relay](../specs/media-relay.md) §3.2 O3,
media-control). `carrier-quirks`'s `RewriteBody` is a **field write**. Under hook-framework §6 both
declare `media_types_rewritten: ["application/sdp"]`, and G2 makes exclusive claims an implicit
conflict — so as the spec stands today, **a deployment cannot anchor media and run an SDP quirk at
the same time.** That is not an acceptable answer; it is the common case.

The fix is the same refinement `headers_owned` already embodies. Ownership is per header *name*, not
"all headers"; body ownership should likewise be per claim *kind* and per field:

```rust
pub enum BodyClaim {
    Replace(MediaType),              // exclusive per (media type, phase) — the relay's claim
    Field(MediaType, CatalogSdpField),  // exclusive per (media type, field, phase) — a quirk's
}
```

**[sipx-clstr] rule — a `Field` writer is ordered after every `Replace` writer at the same phase,
computed by the framework and not hand-declared.** A field write onto a body that is about to be
replaced is silently lost, and "silently" is the disqualifying word: it would be a correct-looking
manifest producing a message the vectors did not predict. Leaving it to an `After("media-anchoring")`
constraint in the quirk module's own manifest would work exactly until someone wrote a second
replacer, which is what G-rules are for.

This is a change to a **closed** spec (hook-framework §6 `SyntaxDecl`, §7 G2). It is not edited in
here; it is handed to EX-8 below with the rest.

#### Media policy — a quirk asserts, and never configures

Acceptance asks what happens when a quirk implies SRTP: a profile that injects a security-mechanism
header is saying something on the wire about media security, and if the media plane does not do it,
the platform is lying to a peer that may act on it. MP11
([media-relay](../specs/media-relay.md) §13.6) is the rule this section restates on EX-7's side of
the seam: **a quirk profile may require an SRTP mode; it may never assign one.**

**[sipx-clstr] rules:**

- **A quirk cannot configure media, structurally.** There is no op for it: `m=` proto, `a=crypto`,
  ICE candidates and transport addresses are not catalogue rows and cannot become rule targets. SRTP
  mode, codecs and transcoding are the **trunk's** —
  [ME-6](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/ME-6-specify-per-trunk-codec-and-srtp-policy.md)
  landed them as `TrunkMediaPolicy`, a per-trunk, startup-readable declared value
  ([media-relay](../specs/media-relay.md) §13.1) — never a value computed per call. One owner, and
  it is not this one.
- **The assertion vocabulary maps onto `SrtpPolicy`, which has no required/optional axis of its
  own.** `SrtpPolicy` (media-relay §13.1) is `Disabled`, `Sdes { suites }` or `DtlsSrtp { role }` —
  three keying mechanisms, not a required/optional pair. `Srtp` is therefore satisfied by **any
  variant but `Disabled`**: the assertion is "this leg runs some SRTP mode", not "this leg runs
  `Sdes`" or any other specific mechanism. Asserting a specific mechanism would let a profile pick
  the keying method by which profile matched — the per-call pattern MP6 forbids, wearing a
  better-typed hat, and exactly the reasoning media-relay §13.6 gives for MP11 being
  constrain-not-assign in the first place (two profiles requiring SRTP must be able to agree without
  a precedence rule, and only an unparametrised constraint agrees with itself for free — MR-C-4). A
  boolean-shaped constraint is also all `requires_srtp` — media-relay §13.6's own name for the field
  it leaves to EX-7 to spell — ever needed: a suite or role preference is a property of the
  mechanism, not of whether one runs.
- **A quirk may assert this precondition on the trunk's policy.** `requires_media: &[MediaAssertion]`
  is a closed set of claims — v1 is the single variant `Srtp` — checked at **startup** against the
  bound trunk's declared `SrtpPolicy`. A profile asserting `Srtp` bound to a trunk declaring
  `SrtpPolicy::Disabled` **fails the boot** (G11, media-relay's G-M5, vector QP-G-6). It is never
  coerced at runtime and never silently ignored.
- **The assertion is read-only and one-directional.** EX-7 never writes a media policy value, and
  ME-6 never emits a header — signalling the mechanism is the quirk's, doing the media is the
  trunk's. A deployment whose peer needs both writes both, and the boot check is what stops the two
  from drifting apart.
- **The assertion binds to a trunk only.** `MediaAssertion` is checked against a `TrunkMediaPolicy`,
  and only a trunk has one — a domain is the registering side and carries no media policy for §13 to
  read. A profile carrying a non-empty `requires_media` bound to a domain therefore **fails the
  boot**, naming the profile and the domain (G12, vector QP-G-11), rather than skipping the check
  silently or reading through to a trunk the binding does not name. `HeaderRule` and `SdpRule` carry
  no such restriction — only `requires_media` does, because only it needs a trunk to check against.

**The boundary, named.** EX-7 owns the assertion vocabulary and the G11 check; media-relay states the
same check from its own side as G-M5, vector `MR-C-3` — one rule, checked once, described from both
specs because each stands independently, not two checks. **The error content is G-M5's**: naming the
trunk and the profile. G11 does not add the assertion or the actual policy to that content — with
`MediaAssertion` one variant wide, "the assertion" is invariant across every violation and tells a
reader nothing G-M5's two names do not already imply, and "the actual policy" is exactly the trunk
configuration the error already points at. ME-6 owns the policy value the check reads: **per-trunk
SRTP mode is a declared enum, a field of `TrunkMediaPolicy`, readable at startup**
([media-relay](../specs/media-relay.md) §13.1, and exported per trunk by §13.5's effective-policy
record) — not a value computed per call. That is the one coupling either story has to the other, and
it is settled, not conditional: ME-6 landed exactly this shape, so G11 is a startup check now, not
something that becomes one. Nothing here is written into `media-relay.md`.

#### The shipped catalogue, and its vectors

Acceptance: *each shipped profile carries a test vector; adding one is a config change plus a
vector.* The catalogue ships two profiles at v1, and the worked example below is the two of them
composed on one trunk — so the shipped set demonstrates composition rather than asserting it.

| Profile | Rules | Vectors |
|---|---|---|
| `sec-agree-headers` | `Set(Security-Client)` and `Set(Security-Verify)` from trunk config, on a declared method set; `requires_media: [Srtp]` | QP-A-1 … QP-A-4, QP-G-4, QP-G-6, QP-G-7, QP-G-11, QP-G-15 |
| `sdp-direction-explicit` | `EnsureExplicit(Direction)` at `Session` and `Media(Audio)`, on requests and responses carrying a body | QP-A-5 … QP-A-9, QP-C-1, QP-C-3 |

Rows use the `QP` prefix in the three-part `QP-X-n` shape, so registering them in the vector gate is
registration only — [CF-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/CF-8-bring-every-spec-under-the-vector-gate.md)'s
work, and they are unenforced until it or EX-8 lands. Every "message unchanged" expectation below is
a **byte** expectation, which the kernel's lossless model makes meaningful: an untouched byte
re-serializes verbatim (hook-framework §1, PX-3).

*Application — `QP-A`:*

| # | Given | Expect |
|---|---|---|
| QP-A-1 | `sec-agree-headers` bound to `trunk-a`; outbound INVITE on a branch to `trunk-a` | Both headers present at the configured values, with the `mediasec` parameter carried through; every other byte of the F5 draft unchanged |
| QP-A-2 | Same, an outbound OPTIONS (method not in the rule's `MessageClass`) | Request byte-identical; **no** `PatchHeaders` effect in the trace |
| QP-A-3 | Same profile; a branch to `trunk-b`, which binds no profile | Byte-identical. The profile is bound, not matched — the assertion that binding is the only selector |
| QP-A-4 | Same, applied to a draft that already carries `Security-Client` at the configured value | Byte-identical result and one effect: idempotence |
| QP-A-5 | `sdp-direction-explicit`; offer with no direction attribute at session or `m=audio` | `a=sendrecv` materialized at both declared scopes (RFC 8866 §6.7); every other byte of the body unchanged |
| QP-A-6 | Same, an offer carrying `a=sendonly` on `m=audio` and nothing at session scope | `m=audio` **untouched**; session scope materialized. The literal P3 assertion — a negotiated value is never overwritten |
| QP-A-7 | Same profile on the response path (H11), 200 OK carrying an answer | Same two rules, same outcome; direction is materialized on the answer body |
| QP-A-8 | A request with no body | No `RewriteBody` effect, no error |
| QP-A-9 | A request whose body is `application/isup` | Untouched — the declared media type scopes the effect (E3) |

*Composition — `QP-C`:*

| # | Given | Expect |
|---|---|---|
| QP-C-1 | Both shipped profiles bound to `trunk-a` — the worked example | Both applied; and the forwarded bytes are identical under either evaluation order, which is the confluence claim asserted rather than argued |
| QP-C-2 | A domain-bound profile and a trunk-bound profile writing disjoint targets | Both applied; the trace names both |
| QP-C-3 | `media-anchor` selected alongside `sdp-direction-explicit` | The relay's replacement body is produced first, the field write lands on **it**, and the forwarded body carries both the relay's `c=`/`m=` and the materialized direction. The G9 ordering assertion |

*Startup validation — `QP-G`:*

| # | Given | Expect |
|---|---|---|
| QP-G-1 | A profile naming a header outside the catalogue | Startup error naming the profile, the rule and the header: not a catalogue row |
| QP-G-2 | Two distinct applicable profiles both writing `Security-Client` on an overlapping message class, with no override declared at the binding | Startup error (G10) naming both profiles and the contested target — the elementary class they share, not the classes they do not |
| QP-G-3 | Same, with the binding declaring an override naming that target and one of the two profiles as `winner` | Boots; the winner's rule is in the composed set and the loser's is not, and the startup composition record names the override |
| QP-G-4 | A configured value that does not parse against the row's ABNF | Startup error (G10) naming the profile, the key and the parse failure |
| QP-G-5 | A binding naming a trunk that does not exist | Startup error (G10) |
| QP-G-6 | `sec-agree-headers` (asserting `Srtp`) bound to a trunk declaring `SrtpPolicy::Disabled` | Startup error (G11, media-relay's G-M5/MR-C-3) naming the trunk and the profile |
| QP-G-7 | Same profile on a trunk declaring `SrtpPolicy::Sdes { .. }` or `SrtpPolicy::DtlsSrtp { .. }` | Boots |
| QP-G-8 | A second module declaring `Replace("application/sdp")` at `BeforeForward` alongside `media-anchor` | Startup error (G9) naming both modules and the claim |
| QP-G-9 | Two modules declaring `Field("application/sdp", Direction)` at the same phase | Startup error (G9) naming both and the field |
| QP-G-10 | A catalogue row for a header a selected module owns (`Session-Expires`, `session-timer`) | Startup error as an ordinary G2 exclusive-claim conflict, naming `carrier-quirks` and `session-timer` — the catalogue invariant, enforced by machinery that already exists |
| QP-G-11 | `sec-agree-headers` (asserting `Srtp`) bound to a domain rather than a trunk | Startup error (G12) naming the profile and the domain: no `TrunkMediaPolicy` exists for a domain to check the assertion against |
| QP-G-12 | Two profiles bound to the **same trunk** contesting `P-Charging-Vector` on INVITE, resolved by an override at that trunk | Boots. The escape is not trunk-over-domain: the commonest contest is at one attachment point, and a directional rule could not reach it |
| QP-G-13 | An override whose `target` no two applicable profiles write — the contest was removed, the override was not | Startup error (G13) naming the binding and the target. The override that outlives its contest fails the boot instead of silently doing nothing |
| QP-G-14 | An override whose `winner` is bound at that attachment point but writes no rule for the named target | Startup error (G13) naming the binding, the target and the winner: naming a profile does not name a target, and the schema does not let one stand in for the other |
| QP-G-15 | `sec-agree-headers` (asserting `Srtp`) on a trunk declaring `SrtpPolicy::Disabled`, with an override deleting **every** one of its rules at that trunk | Still a startup error (G11). An override deletes rules, never assertions; the profile is still bound, so the assertion is still checked |

B1–B4 get no vector, and should not: they are properties of the type, not of a run. Their test is
that the grammar has no recursive constructor, no environment and no conditional — visible in a
diff, which is a stronger guarantee than a passing assertion about one example.

#### Worked example

The live case is a peer that wants the RFC 3329 security-mechanism headers on the requests it
receives, and an SDP offer whose direction attribute is explicit. Today that is an inline domain test
inside a routing script. Under this design it is **one binding and two catalogue profiles**, no code:

```toml
[trunks.trunk-a]
quirks = ["sec-agree-headers", "sdp-direction-explicit"]

[trunks.trunk-a.quirk_config.sec-agree-headers]
methods         = ["REGISTER", "INVITE"]
security_client = "sdes-srtp;mediasec"
security_verify = "sdes-srtp;mediasec"
```

Three things the example is chosen to show, because each is a place the mechanism could have been
got wrong:

1. **REGISTER and INVITE are the same rule.** A REGISTER the platform proxies to a peer is an
   ordinary request on the proxy path, so it reaches H9 like any other; the method set is data. A
   REGISTER the platform's own registrar answers is not proxied at all, and the peer-facing surface
   there is the response — H11, same profile, no second mechanism.
2. **The SDP half is a *default*, not a rewrite.** The requirement reads as "rewrite to
   `a=sendrecv`", and taking that literally would have produced `Set(Direction)` and, with it, the
   ability to unhold a held call from a config file. `EnsureExplicit` satisfies the peer and cannot
   do that (P3, QP-A-6). Reading the requirement as a default rather than an assignment is the whole
   difference between a bounded vocabulary and a small scripting language.
3. **The line where configuration ends is visible.** If this peer later required the RFC 3329
   negotiation itself — `Require: sec-agree`, comparing `Security-Server` against `Security-Client`,
   the §2.2 re-request — no profile could express it, because negotiation is behaviour and the
   grammar has no behaviour in it. That is a module with an RFC-registry entry (EX-2) and its own
   story. A bound that tells you when you have left it is doing more work than a bound that merely
   holds.

#### Startup validation — five new G-rules

| # | Rule | On violation |
|---|---|---|
| G9 | **Body claims are kind-scoped and ordered.** `Replace(t)` is exclusive per `(t, phase)`; `Field(t, f)` is exclusive per `(t, f, phase)`; a `Replace` and a `Field` on the same media type coexist, and every `Field` writer is ordered after every `Replace` writer at that phase by the framework | Startup error naming both modules and the contested claim (QP-G-8, QP-G-9) |
| G10 | **Bindings resolve and compose disjointly.** Every bound profile id, trunk, domain and `ConfigKey` exists and type-checks; every literal parses against its row's ABNF; every rule names a catalogue row; every field's op is in that row's op set; the composed rule set at one attachment point is disjoint on targets — a target being one catalogue row at one *elementary* message class, so two rules contest on every elementary class their classes share — once the overrides declared at that binding (G13) have been applied | Startup error naming the profile, the rule and the offending id, value or contested target (QP-G-1 … QP-G-5) |
| G11 | **Media assertions hold.** Every `MediaAssertion` on a trunk-bound profile is satisfied by the bound trunk's declared `SrtpPolicy` — `Srtp` holds for any variant but `Disabled`. The same check as media-relay's G-M5; the error content is G-M5's, not restated richer here | Startup error naming the trunk and the profile (QP-G-6, = media-relay's MR-C-3) |
| G12 | **Media assertions need a trunk.** A profile carrying a non-empty `requires_media` bound to a domain is rejected: only a trunk binding has a `TrunkMediaPolicy` to check it against | Startup error naming the profile and the domain (QP-G-11) |
| G13 | **An override resolves one live contest, and reaches nothing else.** An override is declared at a **binding**, never in a profile, and names one contested `target` and one `winner` profile id. At startup that target must actually be contested at that attachment point by two or more distinct bound profiles, and the winner must be one of them; the losing rules for that target are then deleted from the composed set before any message is evaluated, so what G10 checks for disjointness is the set after overrides. A `requires_media` assertion has no target, so no override can name one and none suppresses G11 or G12 | Startup error naming the binding, the target, and the winner where the winner is not among the profiles contesting it (QP-G-13 … QP-G-15; the resolving cases are QP-G-3 and QP-G-12) |

Same discipline as G1–G8: an invalid combination fails deployment, never a call.

#### Upstream boundary

**Considered for upstream: no, cluster-specific** — a quirk profile is per-peer deployment policy
expressed over this platform's trunk and domain objects and its hook manifest, and the kernel has no
trunks, no domains, no hooks and no notion of "which peer needs which header." The header surgery the
module performs is already the kernel's and is consumed rather than re-made: `Headers::push`,
`remove_first` and `retain` landed as sipx `S-15` in `v0.4.0` and reach this platform through PX-3.

**One row this design does imply, and it is ME-4's to file.** `Field(application/sdp, …)` requires
locating a named element at session or media scope — which means *reading* the body.
[media-control](media-control.md) states the trigger precisely: the trait carries SDP as opaque bytes
end to end, and "if `ME-4` ends up having to *read* the body, that parser is protocol-generic and
becomes an [upstream ledger](../upstream.md) row then." EX-7 makes that condition fire. What is
needed is small and entirely protocol-generic — locate the direction attribute and the session name
under the RFC 8866 §5–6 grammar, with the lossless-reserialization property the `Headers` surgery API
already has — and it should be **one** ledger row filed alongside ME-4 rather than two rows for one
parser. This story does not file it: the trigger is ME-4's by media-control's own wording, and the
ledger's rules ask that a row be written against a re-read of the kernel rather than an assumption
about it. The requirement is recorded here so ME-4 inherits it rather than rediscovering it.

#### What this hands to the spec

EX-7 is a design decision, not a normative text — the same posture EX-6 took, and for the same
reason: [hook-framework](../specs/hook-framework.md) is closed. The deltas, named so the next agent
does not have to rediscover them, all belong to
[EX-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/EX-8-make-the-async-query-declaration-normative.md):

- **§4** — no change. The effect enum is sufficient; `PatchHeaders` and `RewriteBody` already exist.
- **§3** — no change. **No new phase.** H2, H9 and H11 already permit everything `carrier-quirks`
  emits, and the H-table is untouched.
- **§6** — `SyntaxDecl`'s `media_types_rewritten` becomes the `BodyClaim` split (`Replace` /
  `Field`); `carrier-quirks` joins the §9 manifest cast with its catalogue as `headers_owned`. This
  is the one **structural** change EX-7 needs, and the one worth reviewing hardest, because it edits
  a claim other modules already declare.
- **§7** — G9 (body-claim kinds and their computed ordering), G10 (bindings resolve and compose
  disjointly, over elementary targets), G11 (media assertions hold — the same check media-relay
  states as G-M5), G12 (media assertions need a trunk binding), G13 (a binding-level override
  resolves one contested target, and must be resolving a live one). G2 is unchanged and does the
  catalogue invariant for free.
- **§8** — the shipped profile catalogue is a profile-level value, so EX-5's catalogue carries the
  bindings — and the overrides declared beside them — alongside `hook_budget`.
- **§9** — vectors `QP-A-1` … `QP-G-15` above, and the `QP` prefix registered in the vector gate
  (CF-8's `SPECS`/`FAMILIES`, fenced from this story).

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

*On carrier quirks (EX-7):*

- **A small expression language in configuration** — matches, substitutions, a conditional or two.
  Rejected, and it is the alternative the story exists to refuse: an embedded interpreter is the
  vision's stated non-goal (also refused by [routing-trunks](routing-trunks.md) for trunk
  selection), it makes every deployment's behaviour un-reviewable and un-versionable, and it
  reproduces the routing script this epic is replacing with nicer syntax. The bound in B1–B4 is
  what makes the refusal checkable rather than aspirational.
- **"Just regexes, they are only for headers."** Rejected as the same thing arriving in
  installments. A regex over a header value is a program with no type, no totality argument and no
  vector that constrains what it will do to the *next* message; and once one exists, the argument
  against the second is gone.
- **One module per carrier.** Rejected: modules are statically compiled (hook-framework §2), so a
  new peer would be a code change, a release and a profile edit — exactly the patch this story
  removes. One module carrying a fixed vocabulary, many profiles carrying data, is the split that
  makes the common case config.
- **A profile carries its own selector** (domain regex, peer pattern). Rejected: a selector is a
  condition (B3) with a config file around it, and it makes "which transform did this peer get"
  unanswerable without replaying a message. Binding from the trunk/domain side keeps the answer in
  configuration.
- **Precedence instead of disjointness** — "trunk beats domain", last write wins. Rejected: it is a
  policy nobody writes down, and its failure mode is a domain profile that silently stops applying
  to one trunk long after anyone remembers binding it. Contests are startup errors; the one escape
  is a per-target override declared **in the binding**, which is data, names its winner explicitly,
  and fails the boot once the contest it was written for has gone away (G13).
- **Put the override on the profile** — a trunk-bound profile declaring
  `overrides = ["<domain-bound-profile-id>"]`, the shape this design carried until EX-10. Rejected
  three times over, and it is worth recording because it is the shape the rule reads its way into.
  It contradicts *a profile does not know where it applies*: a shipped, versioned catalogue profile
  would have to name one deployment's other profiles, and "which transform did this peer get" would
  stop being readable from the binding. It also names the wrong thing — a list of profile ids cannot
  say *which* target is conceded, while both the rule that consumes it (G10) and the vector that
  asserts it (QP-G-3) require a target; profile ids and targets are not interchangeable. And being
  trunk-over-domain it could not reach the commonest contest at all, two profiles bound to one
  trunk, while working only in the case where the trunk-bound and domain-bound sets intersect —
  which this design has not yet derived.
- **Let a quirk force an SDP direction** (the live requirement read literally). Rejected: it would
  let a configuration file unhold a held call (RFC 3264 §8.4) or contradict an answer's §6.1
  consistency. `EnsureExplicit` materializes the RFC 8866 §6.7 default, satisfies the same peer, and
  cannot change a negotiated value — P3.
- **Let a quirk turn SRTP on when it injects a security header.** Rejected: media policy is the
  trunk's (ME-6), and a vocabulary that can configure media from the signalling side gives one
  setting two owners. The quirk *asserts* the policy it needs and the node refuses to boot if the
  assertion fails (G11) — the same fails-deployment-never-a-call discipline as G7/G8.
- **Give `carrier-quirks` an `After("media-anchoring")` ordering constraint** instead of the G9
  computed rule. Rejected: it holds exactly until a second body-replacing module exists, and its
  failure is silent — a correct-looking manifest producing bytes no vector predicted.

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
- **The `SyntaxDecl` body-claim split (EX-7) edits a closed spec.** G9 needs
  `media_types_rewritten` to become `Replace`/`Field`, which changes a declaration `media-anchor`
  already makes. It is the sharpest review item EX-7 hands to EX-8, and until it lands **a
  deployment cannot both anchor media and run an SDP quirk** — G2 rejects the pair at startup. The
  ordering half is deliberately a G-rule rather than an `After` constraint, but that decision is
  worth re-testing once a second body-replacing module actually exists.
- **How narrow is too narrow (EX-7).** The v1 catalogues are three header families and two SDP
  fields. If the fourth peer needs a fourth row, the "config change plus a vector" promise holds
  only for peers the catalogue already anticipates, and everyone else waits for a release. The
  measure to watch is the ratio of new bindings to new catalogue rows over the first handful of
  peers; if it approaches 1:1 the vocabulary is drawn in the wrong place, and the answer is a wider
  catalogue rather than a looser grammar.
- **P3 is a judgment, not a theorem (EX-7).** "Materializes a default" and "overwrites a negotiated
  value" are clear for `a=sendrecv` and `s=`, and progressively less clear for `a=ptime` and `b=AS`,
  which is exactly why neither is catalogued yet. Each future row owes the argument, and a row that
  cannot make it is a module.
- **`requires_media` is one variant wide (EX-7)** — `Srtp`, satisfied by any `SrtpPolicy` but
  `Disabled` (media-relay §13.1; reconciled against the type ME-6 actually landed by EX-9). Codec
  and transcoding assertions are not in v1 and should not be added until a peer needs one; each would
  owe the same required/optional-axis argument `Srtp` just settled, made fresh against
  `CodecPolicy`/`Transcode` rather than assumed from this one.
- **When a trunk-bound and a domain-bound rule set actually intersect is asserted, not derived
  (EX-7).** The composition rule says the composed set at one attachment point is "every trunk-bound
  and domain-bound profile's rules together" and `QP-C-2` asserts both apply to one message, but
  under this design's own definitions — a trunk is the egress peer, a domain is the registering
  side — nothing says which messages carry both attachments at H9/H11. EX-10 removed the *escape's*
  dependence on the answer: a per-target override with a named winner resolves a contest between any
  two profiles at any one attachment point, and holds whether or not the two sets ever meet. The
  union sentence and `QP-C-2` still owe the derivation, and that is its own story rather than a
  guess made here.
- **Bindings are per trunk and per domain, and a large deployment has many (EX-7).** Nothing here
  bounds how many profiles one binding may carry or how many bindings a node validates at startup.
  Startup cost is the only exposure — evaluation is O(rules) per message — but a config surface that
  grows without a bound is worth a limit before it has one imposed by an incident. Belongs with
  DP-1's schema.

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

EX-7 adds the last: a `carrier-quirks` module over the two shipped profiles, with the `QP` vectors
passing — and the two demonstrations that are the point rather than the coverage. First, that
accommodating the worked-example peer is a **binding in configuration and a vector**, with no Rust
diff at all. Second, that the vocabulary cannot be talked into a transformation it does not have:
the `QP-G` rows show a profile that names an uncatalogued header, contests a target, or asserts a
media policy the trunk does not hold failing the **boot** rather than a call, and QP-A-6 shows the
one op that could have changed a negotiated value being absent from the grammar instead of
discouraged in prose.
