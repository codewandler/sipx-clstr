# Spec: Hook framework

**Status:** normative · **Crate:** _future — created by CX-2_ ·
**Stories:** EX-1, EX-8 · **Implemented by:** EX-3 ·
**Design:** [extension-framework](../designs/extension-framework.md)

## 1. Normative references

- RFC 3261 §16 (proxy behavior — the pipeline this framework attaches to, via
  [proxy-behavior](proxy-behavior.md)), §10.3 (registrar processing), §19.2 (option tags),
  §20.5 (`Allow`), §20.29 (`Proxy-Require`), §20.37 (`Supported`), §20.40 (`Unsupported`),
  §22 (authentication challenges: `401` from the registrar role, `407` from the proxy role).
- RFC 3261 §16.7 and §21.6 (a 6xx is a claim about the user everywhere, and makes an upstream
  forking proxy cancel every other branch — why G8 forbids one), §21.4.4 (`404` claims the user
  does not exist) and §21.5.4 (`503` is the "temporarily overloaded" status), §16.8 (Timer C),
  §17.1.1.1 with Appendix A (T1 — the per-query `timeout` default), §17.2.1 (the INVITE server
  transaction's `100 (Trying)`, which is why a suspension does not provoke a retransmission).
- RFC 5390 §3 (overload: retrying into a failing element amplifies the load that is failing it —
  why `retries` is capped at 1 and lives inside the same deadline).
- RFC 2308 §5 (a negative *answer* is cacheable, a failure to ask is not — the distinction §6.1's
  `CacheDecl` draws between `Declined` and every failure outcome).
- RFC 3327 (`Path`, option tag `path`), RFC 4028 (`Session-Expires`, option tag `timer`,
  method `UPDATE`) and RFC 3325 §5 with RFC 3323 (`P-Asserted-Identity`, `Privacy`) — used as
  example modules in the vectors; their behavior is specified by their own future stories, not
  here.
- sipx kernel contract consumed: the lossless message model — a module changes a message only
  through typed surgery on declared headers/bodies; every untouched byte re-serializes verbatim
  (the `Headers` surgery primitives, upstream ledger, PX-3).
- Own specs consumed: [proxy-behavior](proxy-behavior.md) §§3–10 (the pipeline, validation
  table, stateless applicability); [affinity-token](affinity-token.md) (token layout and byte
  budget); [location-service](location-service.md) (the registrar-path pipeline around the
  `LocationStore` CAS); `docs/specs/rfc-registry.md` (_future — EX-2's deliverable_: syntax
  constants for header names and option tags).

**Out of scope:** the RFC registry data model (EX-2), the hook runtime implementation (EX-3),
registry codegen (EX-4), the profile catalog and registry-driven RFC-compatibility checking
(EX-5 — §8 draws the boundary), authentication policy content (RG-2), media anchoring behavior
(ME-4/ME-5 — it is a module *under* this spec, not part of it).

**Upstream considerations** (AGENTS.md rule 6): the hook framework is platform orchestration —
its phase names, effect enums and manifest are bound to this platform's proxy/registrar
pipeline and stay here. Considered for upstream: no, cluster-specific because the phase set
*is* the proxy engine's API surface. Syntax artifacts a module advertises (header names,
option tags) may become generated kernel constants via the registry — an upstream decision per
artifact (EX-4).

## 2. Model — modules over a fixed pipeline

A **module** is a statically compiled unit that exports a **manifest** (§6) and one handler
per subscribed phase. A **profile** (§8) selects which compiled-in modules are active.

**[sipx-clstr] rules:**

- **One binary, runtime-selected, statically compiled.** All modules ship compiled into the
  node binary; a profile activates a subset at startup. There is no dynamic loading.
  Rationale: the graph validation of §7 is only worth anything if the set it validated is the
  set that runs; loadable code reintroduces the unverified combination at 3 a.m. that the
  design exists to prevent.
- **Handlers are sans-IO.** A handler is a pure function: typed context in, ordered effect
  list out. It reads no clock, owns no socket, holds no store connection. Time enters as a
  fired timer, store answers enter as facts (§4). Anything that cannot run under the
  deterministic harness (CF-1/CF-5) is rejected at review, per AGENTS.md rule 2.
- **No module ever receives the socket, the raw transport, or an arbitrary-mutation handle to
  the message.** A module's entire power is the closed per-phase effect set of §3/§4.
  Rationale: the framework must *see* extension interactions to check them (the design's case
  against free-form middleware); an open mutation surface makes the §7 checks vacuous.

## 3. The phase pipeline

The pipeline is fixed and ordered; modules subscribe to phases, never insert new ones. Phase
names are shared vocabulary with [proxy-behavior](proxy-behavior.md) (its F5 row names
`BeforeForward`) and with the future location-service spec (RG-1).

```rust
enum Phase {
    MessageParsed, RequestValidated, BeforeAuth, AfterAuth,          // shared head
    BeforeRegistrarUpdate, AfterRegistrarUpdate,                     // registrar path
    BeforeTargetResolution, TargetsResolved, BeforeForward,          // proxy path, request
    ResponseReceived, BeforeResponseForward,                         // response side
    DialogCreated, DialogTerminated,                                 // observational events
}
```

| # | Phase | Fires | Context (module reads) | Permitted effects | Path |
|---|---|---|---|---|---|
| H1 | `MessageParsed` | Kernel parse succeeded (proxy spec V1), before V2 | Request (read-only), parse/validation findings | annotate · record · observe | both |
| H2 | `RequestValidated` | V2–V6 passed | Request, published facts | reject · annotate · record · observe | both |
| H3 | `BeforeAuth` | V7 opens: no token-verified identity satisfies auth (§5 note below) | Request, facts, tenant auth-config view | reject (challenge: `407` proxy role, `401` registrar role; or `403`) · annotate (principal) · query † · record · observe | both |
| H4 | `AfterAuth` | Auth verdict fixed (principal or anonymous) | Verdict, request, facts | reject (`403` policy) · annotate · record · observe | both |
| H5 | `BeforeRegistrarUpdate` | REGISTER accepted, before the `LocationStore` CAS (RG-1) | `RegisterCommand` (contact ops, requested expiries), principal, facts | reject (e.g. `423`, `403` per RG-1 policy) · adjust-registration · annotate · query † · record · observe | registrar |
| H6 | `AfterRegistrarUpdate` | CAS applied; final binding set known; response drafted | Binding set, response draft, facts | patch-headers (response) · annotate · record · observe | registrar |
| H7 | `BeforeTargetResolution` | Route preprocessing (proxy spec §5) done; before the `ResolveTargets` effect | Request, token verdict (when present, read-only), facts | reject · rewrite-target-query · annotate · query † · record · observe | proxy |
| H8 | `TargetsResolved` | The `TargetsResolved` input arrived (proxy spec §2) | Candidate target set (ordered), request, facts | select-targets · reject · annotate · record · observe | proxy |
| H9 | `BeforeForward` | Per branch, at proxy spec F5 — after F1–F4, before F6–F10 | Outgoing request draft, target, branch id, token-mint draft (F4), facts | patch-headers (request) · rewrite-body · token-fact · annotate (branch-scoped) · query † · record · observe | proxy |
| H10 | `ResponseReceived` | Per branch response, after R2 (our Via popped); never for a 100 absorbed by R3 | Response, branch id, facts | annotate · record · observe | proxy |
| H11 | `BeforeResponseForward` | The chosen response (provisional or final) is about to go upstream — before the `Respond` effect; on the registrar path, on the locally originated final | Outgoing response draft, facts | patch-headers (response) · rewrite-body · annotate · query † · record · observe | both |
| H12 | `DialogCreated` | A dialog-establishing 2xx was forwarded upstream (edge-local observation) | Dialog event record: Call-ID, tags, token facts | record · observe | proxy |
| H13 | `DialogTerminated` | A 2xx to an in-dialog BYE was forwarded (edge-local, best-effort) | Dialog event record | record · observe | proxy |

† `query` is a suspension (§4): rules E4/E4a/E4b, its deadline E7, its declaration §6.1, its
failure handling G7/G8. Its stateless-mode interaction is rule E5. Five phases permit it — H3,
H5, H7, H9, H11 — and of those only H3, H5 and H7 also permit `reject`, which G8 makes a
declaration-time constraint rather than a runtime surprise.

**[sipx-clstr] rules:**

- **Alignment is normative.** The phase boundaries land exactly on the proxy spec's pipeline;
  the mapping below is part of this spec's contract, and a change to either side re-reviews
  the other:

  | Proxy spec anchor | Hook framework |
  |---|---|
  | §4 V1 (parse) | `MessageParsed` fires after it |
  | §4 V2–V6 | Engine-internal; precede `RequestValidated` |
  | §4 V7 (authentication — "hook phase") | Realized as the `BeforeAuth`/`AfterAuth` pair |
  | §5 P1–P3 (route preprocessing, token verdicts) | Engine-internal; the `TokenFact` verdict enters phase contexts as a read-only fact from H7 on |
  | §2 `ResolveTargets` / `TargetsResolved` | `BeforeTargetResolution` / `TargetsResolved` |
  | §7 F5 ("hook phase `BeforeForward`") | `BeforeForward` |
  | §8 R2–R11 (response processing) | `ResponseReceived` after R2; `BeforeResponseForward` before the `Respond` effect |
  | §9 CANCEL / Timer C, §6 Via branch | Engine-internal — no phases; no module can cancel a branch, time out a call, or touch a Via |
  | REGISTER (out of the proxy spec's scope) | `BeforeRegistrarUpdate`/`AfterRegistrarUpdate` bracket the `LocationStore` CAS; the location-service spec (RG-1) MUST name these phases |

- **Dialog events are observations, not state.** The platform holds no dialog store
  (invariant 5), so H12/H13 are derived per-edge from forwarded messages: they are
  best-effort (mid-dialog traffic may take another edge), observational only, and MUST never
  be load-bearing for routing or admission. Rationale: an "authoritative" dialog event would
  require exactly the global dialog view the invariant forbids.
- **Mid-dialog authentication is the token.** A request whose Route carried a token that
  verified (proxy spec P2) is authenticated by that verdict; `BeforeAuth`/`AfterAuth` do not
  fire for it. Rationale: re-challenging mid-dialog would force a credential lookup onto the
  token hot path — the exact lookup the token exists to remove.
- **Reject is terminal.** The first `reject` in a phase run stops the pipeline for that
  request; later modules in the order are not invoked. The rejection response still passes
  `BeforeResponseForward`. Rationale: deterministic outcomes require that module order (§7 G3)
  fully determines which module answered.
- **Within a phase, modules run in the §7 computed order,** and each sees the message as
  patched by its predecessors. Because header, body-type and token-fact claims are exclusive
  (§7 G2), no two modules ever write the same field — order affects only what a module reads,
  never who wins a write.

## 4. Effect discipline

Per-phase effects are closed enums; the per-phase permitted sets are the H-table above. There
is no "call me with the message and I'll return a new one" escape hatch.

```rust
enum HookEffect {
    Reject(Response),                     // terminal; engine responds upstream
    Annotate(Fact),                       // publish a declared fact into the request context
    PatchHeaders(HeaderPatch),            // add/replace/remove — declared owned headers only
    RewriteBody(MediaTypePatch),          // declared media types only
    RewriteTargetQuery(TargetQuery),      // H7 only
    SelectTargets(TargetSelection),       // H8 only: filter / reorder / cap the candidate set
    ContributeTokenFact(TokenFact),       // H9 only: declared id, within declared byte bound
    AdjustRegistration(RegisterAdjust),   // H5 only: ClampExpiry | AttachBindingMetadata
    Query(QueryRef),                      // suspension: answered as a Fact re-invocation
    Record(StoreWrite),                   // asynchronous fire-and-forget write
}
```

A `Query` resolves to exactly one **outcome**, and the module's manifest maps every outcome to a
**disposition** (§6.1). Both are closed enums; a module never sees a URL, a socket, a status code
or an exception:

```rust
enum QueryOutcome {
    Answered,       // a well-formed answer carrying a decision
    Declined,       // a well-formed answer that says "no" — an answer, not a failure
    ClientError,    // the resource rejected our request (HTTP 4xx and equivalents)
    ServerError,    // the resource failed (HTTP 5xx and equivalents)
    Malformed,      // answered, but undecodable — or decodable and outside the closed world
    Timeout,        // the engine's QueryDeadline fired first (E7)
    Unavailable,    // never sent: breaker open, in-flight cap, queue full, resource unreachable
}

enum Disposition {
    Apply,                 // the answer's own decision applies — the Answered arm, and no other
    Reject(Status),        // fail-closed — terminal; Status from the closed set of G8
    Proceed(DefaultRef),   // fail-open — continue with a named, startup-validated default fact
    ProceedWithout,        // continue with no fact — only if the consuming Scratch is optional
}
```

`Malformed` is what non-negotiable 3 requires of this surface: garbage from the network
**decodes to an outcome**. Decoding lives in the driver's resource client, it is fallible by
signature, and its failure is an ordinary declared arm — there is no path from a malformed answer
to a panic.

**The framework is fail-closed by construction.** There is no implicit disposition anywhere: an
outcome with no declared arm is a *startup* error (G7), never a runtime fallback, and
`Proceed`/`ProceedWithout` exist only where a manifest declares them and startup has validated
them (G8). A query that times out, exceeds a budget, or resolves to an outcome the manifest does
not answer for never becomes "proceed" by omission.

**[sipx-clstr] rules:**

- **E1 — Observation is the empty effect list.** A subscription with no effects (plus
  `Record`) is purely observational; every phase permits it.
- **E2 — Engine-owned headers are unpatchable.** `headers_owned` may never include `Via`,
  `Route`, `Record-Route`, `Max-Forwards`, `From`, `To`, `Call-ID`, `CSeq`,
  `Content-Length`. Rationale: these carry §16 correctness and the token placement (proxy
  spec §§5–7); a module that needs them is a core edit wearing a manifest. (`Path` is not on
  the list — inserting `Path` on a proxied REGISTER is a legitimate module, RFC 3327.)
- **E3 — Patches are scoped by declaration.** `PatchHeaders` may touch only the module's
  `headers_owned`; `RewriteBody` only its `media_types_rewritten`; `ContributeTokenFact` only
  its declared fact ids within their byte bounds. A violating effect at runtime is a bug the
  harness must catch, but the *declaration* is checked at startup (§7 G6).
- **E4 — `Query` is a suspension, never a blocking call.** A `Query` names a declared store
  (§5 class c) or a declared owned-resource client (e.g. `MediaRelay`, ME-1). The engine
  suspends that module's phase run, performs the query as driver I/O, and re-invokes the
  module with the answer as a fact. This is the same effect-out/fact-in shape as the engine's
  own `ResolveTargets`/`TargetsResolved` (proxy spec §2). Which store or resource a query may
  name, its deadline, and what happens to every outcome are declared in the manifest (§6.1) and
  validated at startup (G7, G8) — none of it is coded in the handler.
  - **E4a — `Query` is the last effect of the invocation that emits it.** A returned effect list
    contains at most one `Query`, and it is last. Effects preceding it are applied first;
    everything the module wants to do with the answer happens in the re-invocation. A list with
    an effect *after* a `Query` is a module defect: the engine applies none of the effects that
    follow it, concludes the request fail-closed (no `ResolveTargets`, no `Forward`) and records
    the defect. It never applies them in order and hopes. Rationale: the alternative is an
    invocation whose second half runs against a message state that does not exist yet, which is
    not expressible as a pure function of the context it was given.
  - **E4b — The outcome is decided exactly once.** A `Query` carries a generation counter, bumped
    when the deadline is armed and when it is cleared. An answer whose generation is not the
    current one is discarded **at the input boundary**, and a `QueryDeadline` firing after an
    answer has landed is discarded at pop — so a late answer never overwrites a concluded
    outcome and a stale deadline never re-concludes one. Exactly one disposition is applied per
    query, and it appears exactly once in the trace. Rationale: without this, "same seed, same
    trace" (CF-1) is a coincidence rather than a property, and a late answer arriving after a
    fail-closed rejection would resurrect a decision the transaction already made.
- **E5 — Stateless interaction.** The same phases fire in stateless mode (proxy spec §10). A
  `Query` on a stateless-eligible request breaks applicability condition A3 and promotes the
  request to stateful *before anything is sent* — proxy spec S7 governs. A challenge
  (`reject` at H3) does the same. Module order being deterministic (§7 G3) preserves S4's
  deterministic-output requirement.
- **E6 — Timers.** Two classes only, both declared (§6). *Module timers* are periodic
  maintenance off any pipeline (store GC, cache refresh); their fire permits `Query` and
  `Record` only. *Transaction timers* are armed during request phases, auto-cleared when the
  transaction terminates, and their fire permits `Annotate` and `Record` only. No module
  timer can alter a transaction's outcome — outcome timers (Timer C et al.) are engine-owned
  (proxy spec §9). Rationale: a module that can time a call out is a module that can take the
  platform's §16 conformance with it.
- **E7 — `QueryDeadline` is an engine-owned timer class, and deliberately not a module timer.**
  It is armed by the **engine** from the emitting `QueryDecl`'s declared `timeout` at the moment
  the `Query` effect is applied, and cleared when the outcome is decided (E4b). It is **not** one
  of E6's two module-declared classes, and it may not be declared in a manifest at all.
  Rationale, stated because it is the whole reason for the split: E6 forbids a module timer from
  altering a transaction's outcome, and this timer does exactly that — its fire *is* the
  `Timeout` outcome, and the declared arm for `Timeout` can reject the request. Engine ownership
  keeps E6 intact and puts the deadline where every other outcome timer already lives (proxy
  spec §9). Three consequences are normative:
  - **The armed deadline is the authoritative one.** A driver MAY set a transport-level timeout
    as a backstop, but the deadline the state machine armed is the one that decides `Timeout`,
    because that is the one the harness advances in virtual time.
  - **`retries` live inside it.** A declared retry re-issues the request to the resource under
    the *same* deadline and the same generation; the deadline is a wall around the whole query,
    attempts included. Retries never extend it.
  - **No engine timer's duration or arming point is a function of a query's latency.** Every
    engine timer is armed at a fixed pipeline anchor rather than at a wall-clock offset from the
    request's arrival: Timer C at proxy spec F11, after F10 `Forward`, so its deadline is
    `t_forward + 180 s` and never `t_invite + 180 s`. A suspension at H3, H5 or H7 precedes
    `ResolveTargets`, so no branch and no branch timer exists yet; one at H9 or H11 delays when
    the anchor is reached and does not change what is armed there. What a suspension does
    consume is caller-perceived setup time, which §8's `hook_budget` bounds explicitly rather
    than by accident.

## 5. Module state — the three classes

The design's central risk: "declared module state" must not become a euphemism for a dialog
database. **[sipx-clstr] rule — a module may declare state in exactly three classes; nothing
else exists:**

| Class | Declaration | Lifetime & access | Bound |
|---|---|---|---|
| **(a) Request-scoped scratch** | `Scratch { key, ty, published, optional }` | Lives in the request's response context; dies with the transaction. `published: false` = module-private; `true` = readable by later modules as a fact. `optional: true` (default `false`) declares that every reader handles its absence — the one thing that makes a query's `ProceedWithout` disposition declarable (G8). Never serialized, never crosses a node | Type-checked at compile time |
| **(b) Token-carried facts** | `TokenFact { id, max_bytes }` | Contributed at H9 into the token minted at proxy spec F4; returned verbatim inside the token verdict on every mid-dialog request, on any edge, zero lookups | Σ `max_bytes` over the profile ≤ the module-fact sub-budget of the affinity token. The budget authority is [affinity-token](affinity-token.md) (AF-1); **placeholder until AF-1 fixes the layout: 64 bytes** of the ≤ 200-byte token parameter (proxy spec F4) — this row is re-reviewed when AF-1 lands |
| **(c) Off-hot-path owned stores** | `Store { name, keyed_by, schema }` | Module-owned (single writer); read and written only via `Query`/`Record` (E4) — asynchronous, never awaited on the forwarding decision path of a token-routed request (E5 makes that structurally true: a query forces stateful handling) | `keyed_by` is drawn from `StateKeyDomain` below |

```rust
enum StateKeyDomain { Tenant, Principal, Aor, ModuleConfig }   // deliberately: no Dialog, no CallId
```

**Rationale against invariant 5 (state rides the message):**

- Class (a) adds nothing the stateful proxy does not already hold — it *is* the response
  context's lifetime, and it is gone before any mid-dialog message exists.
- Class (b) is the invariant's own mechanism: the fact travels in Record-Route/Route and
  comes back with every mid-dialog request, so any edge acts on it with zero lookups. The
  byte bound keeps the token inside the F4 budget, so the mechanism cannot bloat the
  signalling it rides on.
- Class (c) is the invariant's stated carve-out — durable state in owned stores off the hot
  path — made *mechanically checkable*: `StateKeyDomain` has no dialog- or call-scoped
  variant, so a store that would answer "how do I route this mid-dialog request?" is
  unrepresentable in a manifest. The temptation the invariant forbids cannot be declared,
  therefore it cannot ship. A module needing a per-dialog fact has exactly one home: class
  (b), inside the token.

## 6. The module manifest

The manifest is declarative Rust — a `const` the module exports, checked by the compiler and
consumed by §7 validation at startup. **[sipx-clstr]:** Rust-const over an external file
format, because the modules are statically compiled (§2) and a manifest that can drift from
the code it describes is worse than none.

```rust
pub struct Manifest {
    pub id: ModuleId,                  // lowercase-kebab, unique in the binary
    pub version: Version,              // semver; feeds conformance reporting (EX-2/CF-2)
    pub provides: &'static [Capability],   // names used by requires/conflicts/ordering
    pub requires: &'static [Capability],
    pub conflicts: &'static [Capability],
    pub hooks: &'static [HookDecl],
    pub syntax: SyntaxDecl,
    pub state: &'static [StateDecl],       // §5: Scratch | TokenFact | Store
    pub queries: &'static [QueryDecl],     // id, target: Store(name) | Resource(name), phases
    pub timers: &'static [TimerDecl],      // id, class: Module { period } | Transaction { max }
}

pub struct HookDecl {
    pub phase: Phase,
    pub order: &'static [Constraint],      // Before(Capability) | After(Capability) — never indices
    pub effects: &'static [EffectKind],    // must be ⊆ the phase's closed set (§3 table)
}

pub struct SyntaxDecl {
    pub methods_consumed: &'static [Method],
    pub methods_advertised: &'static [Method],        // → Allow (§7 G4)
    pub headers_read: &'static [HeaderName],
    pub headers_owned: &'static [HeaderName],         // exclusive · PatchHeaders scope · E2 applies
    pub option_tags_consumed: &'static [OptionTag],
    pub option_tags_advertised: &'static [OptionTag], // exclusive · → Supported, V6 acceptance
    pub media_types_rewritten: &'static [MediaType],  // exclusive · RewriteBody scope
}
```

Ordering constraints name **capabilities, not modules and not indices** — `After("auth-provider")`
survives swapping which module provides authentication; a raw position number is a hidden
coupling to the rest of the profile. Header names and option tags SHOULD be the generated
registry constants (EX-2/EX-4), not string literals.

### 6.1 `QueryDecl` — the deadline and the failure handling, as data

A `Query` (E4) is declared, never coded. The declaration carries the deadline the engine arms
(E7), the disposition of **every** outcome, the fallbacks those dispositions may name, and the
two bounds that keep an external resource off the transaction's back:

```rust
pub struct QueryDecl {
    pub id: QueryId,
    pub target: QueryTarget,          // Store(name) | Resource(name)
    pub phases: &'static [Phase],     // ⊆ the phases permitting `query` — H3, H5, H7, H9, H11
    pub timeout: Duration,            // arms QueryDeadline (E7); > 0 and ≤ the profile's hook_budget
    pub retries: u8,                  // 0 | 1, inside the same deadline and the same generation
    pub on: &'static [(QueryOutcome, Disposition)],   // total over the enum — G7
    pub defaults: &'static [(DefaultRef, FactValue)], // resolved and closed-world checked — G8
    pub cache: Option<CacheDecl>,
    pub limits: LimitDecl,
}

pub struct CacheDecl { pub ttl: Duration, pub negative_ttl: Duration, pub max_entries: u32 }

pub struct LimitDecl {
    pub in_flight_max: u32,           // per node; default 512
    pub queue_max: u32,               // default 0 — shed rather than queue
    pub breaker: BreakerDecl,         // failure_threshold, window, cooldown, half_open_probes
}
```

**[sipx-clstr] rules:**

- **Q1 — The outcome map is the module's entire failure policy.** `on` is a total map over
  `QueryOutcome` (G7). `Answered` maps to `Apply` and to nothing else, and no other outcome maps
  to `Apply` (G7) — the arm carries no choice, and it is written out anyway so that a reader of
  the manifest sees all seven outcomes answered rather than six answered and one implied.
- **Q2 — The declaration is the only place a fallback exists.** `Proceed(DefaultRef)` names an
  entry of `defaults`; `defaults` is checked at startup against the fact it feeds and against the
  closed world it draws from (G8). Fail-open is therefore a decision made in advance and visible
  in the manifest, never the absence of one.
- **Q3 — Caching distinguishes an answer from a failure to ask.** The cache is keyed on
  `(query id, key)`, and the key MUST be derived only from the module's `headers_read` and from
  published facts, so the same request produces the same key on every edge. Only `Answered`
  (under `ttl`) and `Declined` (under `negative_ttl`) are cacheable. `ClientError`, `ServerError`,
  `Malformed`, `Timeout` and `Unavailable` are **never** cached — "no route" is an answer,
  "could not ask" is not (RFC 2308 §5), and caching the second turns a blip into an outage.
- **Q4 — Overflow and an open breaker resolve, they do not wait.** When `in_flight_max` is
  reached and the queue (`queue_max`, default 0) is full, and whenever the resource's breaker is
  open, the query resolves **immediately** as `Unavailable` and the declared arm applies — with
  no driver call at all in the breaker case. A request that is going to fail should fail at the
  start of its budget rather than at the end of it, and a flapping resource should cost nothing.
  Breaker state is per node.
- **Q5 — Queries at one phase run serially, in the G3 order.** Two query-emitting modules at the
  same phase are not dispatched in parallel: §3 requires each module to see the message as
  patched by its predecessors, and answer-order dispatch would make the outcome depend on which
  resource replied first — deterministic under a seed, but not under deployment. Their deadlines
  therefore sum, which is why G7's budget is a sum.
- **Q6 — The cheapest query is the one never made.** Where the data changes slowly enough to be
  prefetched, the right shape is not a query on the request path at all: a *module*-class timer
  (E6) refreshes an owned store (§5 class (c)) off any pipeline and the handler reads it with no
  suspension. The suspension is for the irreducible per-call case.

Example (the media-anchoring module, ME-5 — abbreviated):

```rust
pub const MANIFEST: Manifest = Manifest {
    id: "media-anchor", version: "1.0.0",
    provides: &["media-anchoring"],
    requires: &[],
    conflicts: &["b2bua-media"],
    hooks: &[
        HookDecl { phase: BeforeForward,        order: &[After("session-timers")],
                   effects: &[RewriteBody, ContributeTokenFact, Query] },
        HookDecl { phase: ResponseReceived,     order: &[], effects: &[Annotate] },
        HookDecl { phase: BeforeResponseForward, order: &[], effects: &[RewriteBody, Query] },
        HookDecl { phase: DialogTerminated,     order: &[], effects: &[Record] },
    ],
    syntax: SyntaxDecl { media_types_rewritten: &["application/sdp"], ..SyntaxDecl::NONE },
    state: &[TokenFact { id: "media-node", max_bytes: 8 }],
    queries: &[QueryDecl { id: "relay", target: Resource("MediaRelay"),
                           phases: &[BeforeForward, BeforeResponseForward] }],
    timers: &[],
};
```

That example elides the §6.1 fields, and an elision is not a default: G7 rejects a `QueryDecl`
whose `on` is not total, so the abbreviated form above is not a manifest that could boot. The
`external-route` module (§9) is the complete one — the async external routing consult, at H7 and
nowhere else:

```rust
pub const MANIFEST: Manifest = Manifest {
    id: "external-route", version: "1.0.0",
    provides: &["external-route-decision"],   // a second such module is a G2 conflict at startup
    requires: &[],
    conflicts: &[],
    hooks: &[
        HookDecl { phase: BeforeTargetResolution, order: &[],
                   effects: &[Query, Annotate, RewriteTargetQuery, Reject] },
        HookDecl { phase: TargetsResolved,        order: &[], effects: &[SelectTargets] },
        HookDecl { phase: BeforeForward,          order: &[], effects: &[PatchHeaders] },
    ],
    syntax: SyntaxDecl { headers_read:  &["To", "From"],
                         headers_owned: &["P-Asserted-Identity", "Privacy"],
                         ..SyntaxDecl::NONE },
    // `optional: false` — H8 and H9 read this fact and cannot act without it, which is exactly
    // why every failure arm below is a Reject rather than a ProceedWithout (G8).
    state: &[Scratch { key: "route-decision", ty: RouteDecision,
                       published: true, optional: false }],
    queries: &[QueryDecl {
        id: "route",
        target: Resource("route-oracle"),     // configuration binds the name to an endpoint
        phases: &[BeforeTargetResolution],
        timeout: Duration::from_millis(500),  // T1 (RFC 3261 Appendix A)
        retries: 0,
        on: &[(Answered,    Apply),
              (Declined,    Reject(480)),     // the oracle answered "no egress"
              (ClientError, Reject(500)),
              (ServerError, Reject(500)),     // a fallback pool is Proceed(_), declared, not assumed
              (Malformed,   Reject(500)),
              (Timeout,     Reject(500)),
              (Unavailable, Reject(503))],    // RFC 3261 §21.5.4 — retry elsewhere is the right hint
        defaults: &[],
        cache: Some(CacheDecl { ttl: Duration::from_secs(30),
                                negative_ttl: Duration::from_secs(5), max_entries: 10_000 }),
        limits: LimitDecl { in_flight_max: 512, queue_max: 0, breaker: BreakerDecl::DEFAULT },
    }],
    timers: &[],
};
```

Two properties of that manifest are the point of §6.1 rather than incidental to the example.
The consult is declared at **H7 only**: H9 fires per branch, so a query there would multiply the
external load by the fork width and make the egress decision depend on which branch asked first.
And the identity the oracle influences is `P-Asserted-Identity`/`Privacy` (RFC 3325 §5,
RFC 3323) — never `From`, which E2 makes unpatchable because its tag is half the dialog
identifier (RFC 3261 §12). The oracle *selects within* the egress and identity policy that
configuration already holds; it does not extend it (G8's closed world).

Which dispositions this module's arms carry per deployment — the shipped catalog's defaults and
their per-deployment overrides — is EX-5's profile catalog (§8), not this section's; §6.1 fixes
only that every arm exists and what each one is allowed to say.

## 7. Startup graph validation

Validation runs at startup for every profile the node's configuration selects; any failure
fails the boot with the offending module and rule named — an invalid combination fails
deployment, never a call. **[sipx-clstr] rules:**

| # | Rule | On violation |
|---|---|---|
| G1 | **Dependency closure.** Every `requires` capability is provided by some selected module | Startup error naming the module and the missing capability (HF-2) |
| G2 | **Conflict rejection.** No selected module names a `conflicts` capability that another selected module provides (symmetric — either side's declaration suffices). Exclusive claims are implicit conflicts: two modules owning the same header, advertising the same option tag, or rewriting the same media type | Startup error naming both modules and the contested capability/claim (HF-3, HF-6) |
| G3 | **Deterministic total order per phase.** Topological sort of the `Before`/`After` constraint edges; a constraint naming a capability absent from the profile is ignored (it constrains nothing). **Tiebreak:** among modules with no unsatisfied constraint, the byte-wise lexicographically smallest `id` runs first — repeatedly. The resulting order is a pure function of the selected set | Cycle → startup error listing the cycle (HF-8) |
| G4 | **Capability advertisement is derived, never hand-listed.** `Supported` = the union of `option_tags_advertised` over selected modules. `Allow` = the union of `methods_advertised` plus only what the engine itself terminates (`CANCEL` per proxy spec C1; `REGISTER` when the registrar role is active). No configuration key can append a bare string to either. The derived `Supported` set **is** the acceptance set for proxy spec V6: a `Proxy-Require` tag outside it → `420` with `Unsupported` listing the offenders | — (derivation; HF-5) |
| G5 | **Token budget.** Σ `TokenFact.max_bytes` over the selected set ≤ the module-fact sub-budget (§5 class b; provisional 64 bytes until AF-1) | Startup error naming the modules and the sum (HF-7) |
| G6 | **Effect and scope legality.** Every `HookDecl.effects` ⊆ its phase's closed set (§3 table); `headers_owned` respects E2; every `ContributeTokenFact`, `Query`, timer and patch scope refers to a declared manifest entry; `option_tags_consumed` are advertised by some selected module (self counts) | Startup error naming the module, phase and effect (HF-4) |
| G7 | **The outcome map is total, and the budget holds.** Every `QueryDecl.on` covers all seven `QueryOutcome` variants (§4) exactly once; `Answered` maps to `Apply` and no other arm does; `retries` ≤ 1; every declared `timeout` is > 0 and ≤ the profile's `hook_budget` (§8); and for each of the two request paths, Σ `timeout` ≤ `hook_budget`, summed over every selected module's query declared at each query-permitting phase on that path — proxy: H3, H7, H9, H11; registrar: H3, H5, H11. Within a phase the terms sum because queries there run serially (Q5); H9 contributes once because branches overlap in wall-clock rather than summing | Startup error naming the module, the query id and the missing or duplicated outcome, or the offending sum (HF-9, HF-10) |
| G8 | **Dispositions resolve.** Every `Proceed(DefaultRef)` names a `defaults` entry that exists, type-checks against the fact it feeds, and satisfies the **closed world** — a pool that is a configured trunk, an identity the trunk's asserted-identity and privacy policy admits (RT-7); every `Reject(Status)` is in the closed set `{403, 404, 480, 500, 503}`, and **6xx is forbidden**; a `Reject` arm is declared only for a query whose `phases` all permit the `Reject` effect (H3, H5, H7 — see §3's † footnote), and at H9 and H11 every non-`Answered` arm is therefore `Proceed(_)` or `ProceedWithout`; `ProceedWithout` appears only where the consuming `Scratch` (§5 class (a)) is declared optional | Startup error naming the module, the query id and the offending disposition (HF-11, HF-12, HF-13) |

Rationale: G3's tiebreak makes the order a function of the module set alone — the same
profile computes the same order on every node and every boot. Two edges running different
module orders are two different proxies, which the north star (a cluster indistinguishable
from one correct proxy) forbids.

G7 and G8 apply the same discipline to the one effect that reaches outside the node: an
invalid combination fails deployment, never a call. Neither rule has a runtime half — there is
no "unhandled outcome" path at request time to fall through, because a profile with one does
not boot. **6xx is forbidden** because a 6xx is a claim about the user everywhere rather than
about one egress path (RFC 3261 §21.6) and makes an upstream forking proxy cancel every other
branch (§16.7, proxy spec R6); a routing-policy failure cannot know that, so a module must not
be able to say it. One interaction is carried rather than decided here: proxy spec R8 rewrites a
`503` *received from a branch* into `500` upstream. A `Reject(503)` is locally originated, so R8
does not apply to it — but a second platform node upstream of this one will rewrite it, and
whether a locally originated `503` needs a distinguishing marker is open in the design's *Risks*
until the multi-node topology exists.

## 8. Profiles

A **profile** is a named set of module ids, role flags (proxy, registrar) and **profile values**.
This spec governs the *graph* validation of that set: G1–G8 run per profile at startup, and a
node boots only if every profile it is configured to serve validates.

**`hook_budget` is a profile value — default 2000 ms.** It is the wall on the caller-perceived
setup time one request may spend suspended in queries (E4, E7): the sum G7 checks, per request
path, across every selected module. It is a property of the *profile* rather than of any module,
because no module can know what else is in the profile, and the quantity being bounded is what
the caller waits for rather than what any one module costs. A profile whose declared timeouts
could exceed it fails to boot — the alternative is a slow call discovered in production. EX-5's
catalog carries the value for each shipped profile, and a deployment may override it per profile;
whether the budget should *shrink under load* is the load-shedding question RT-3 owns, and is
deliberately not answered by a fixed value here.

The EX-5 boundary: registry-driven checking — RFC dependency closure across entries,
trust-domain assertions (e.g. mechanisms valid only where the profile asserts the trust
domain), the shipped profile catalog (CoreProxy, ModernRegistrar, CarrierInterconnect,
WebSocketUA) and its compatibility semantics — is specified by EX-5 on top of this section.
EX-5 adds checks; it does not relax any G-rule.

## 9. Test vectors

Vectors are set-in / result-out; the graph vectors run against §7 with manifests as given.
Module manifests referenced: `media-anchor` (§6 example), `session-timer` (provides
`session-timers`; advertises tag `timer`, method `UPDATE`; owns `Session-Expires`,
`Min-SE`), `topology-hide` (owns no exclusive claims; hooks `BeforeForward`), `digest-auth`
(provides `auth-provider`; hooks `BeforeAuth`/`AfterAuth`), `tenant-acl` (requires
`auth-provider`), `b2bua-bridge` (provides `b2bua-media`), `path-support` (advertises tag
`path`; owns `Path`), `external-route` (§6.1's complete example: provides
`external-route-decision`; owns `P-Asserted-Identity`, `Privacy`; one `QueryDecl` `route` against
`Resource("route-oracle")` at `BeforeTargetResolution`), `geo-tag` (one `QueryDecl` `lookup`
against `Store("geo")` at `BeforeForward`, effects `Annotate`).

The `HF-9` … `HF-13` rows all run against a profile whose `hook_budget` is the default 2000 ms,
and each names the single manifest field it changes relative to that example — so what fails the
boot is exactly the declaration under test.

| # | Given | Expect |
|---|---|---|
| HF-1 | Profile `{media-anchor, session-timer, topology-hide}`; constraint: `media-anchor` is `After("session-timers")` at `BeforeForward` | Valid. Computed `BeforeForward` order: `session-timer`, `media-anchor`, `topology-hide` — the constraint holds `media-anchor` back despite its lexicographically smaller id; `topology-hide` is last purely by G3 tiebreak |
| HF-2 | Profile `{media-anchor, tenant-acl}` | Startup error: `invalid profile "EdgeProxy": module "tenant-acl" requires capability "auth-provider"; no selected module provides it` |
| HF-3 | Profile `{media-anchor, b2bua-bridge, digest-auth}` | Startup error: `invalid profile "EdgeProxy": module "media-anchor" conflicts with capability "b2bua-media", provided by module "b2bua-bridge"` |
| HF-4 | A module declares effect `SelectTargets` at phase `BeforeResponseForward` | Rejected at startup graph build (G6): `module "geo-router": effect SelectTargets not permitted at BeforeResponseForward (permitted: PatchHeaders, RewriteBody, Annotate, Query, Record)` |
| HF-5 | Profile `{digest-auth, session-timer, path-support}`, registrar role active | Derived `Supported: path, timer`; derived `Allow: CANCEL, REGISTER, UPDATE`. Request with `Proxy-Require: timer` → accepted (V6); with `Proxy-Require: foo` → `420` + `Unsupported: foo` |
| HF-6 | Two selected modules both advertise option tag `timer` | Startup error (G2 implicit conflict): both modules and the tag named |
| HF-7 | `TokenFact` declarations summing to 72 bytes against the provisional 64-byte sub-budget | Startup error (G5) naming the contributing modules and the 72 > 64 sum |
| HF-8 | `a-mod` declares `Before("b-cap")`, `b-mod` (provides `b-cap`) declares `Before("a-cap")` (provided by `a-mod`), same phase | Startup error (G3): ordering cycle `a-mod → b-mod → a-mod` listed |
| HF-9 | `external-route`'s `on` omits the `ServerError` arm; the other six are declared | Startup error (G7): `module "external-route": query "route" declares no disposition for outcome ServerError; the outcome map must be total`. The omission is **not** read as "proceed", "reject" or any other default — a profile that cannot say what happens does not boot |
| HF-10 | `external-route` and `geo-tag` declare queries at `BeforeTargetResolution` and `BeforeForward` with `timeout` 1500 ms each, both manifests otherwise valid, `hook_budget` 2000 ms | Startup error (G7): `invalid profile "EdgeProxy": queries on the proxy path sum to 3000 ms over hook_budget 2000 ms (external-route/"route" at BeforeTargetResolution 1500 ms, geo-tag/"lookup" at BeforeForward 1500 ms)`. Both modules, both phases and both terms named — the sum is not attributable to either alone |
| HF-11 | `external-route` declares `(ServerError, Proceed("fallback_pool"))` and an empty `defaults` | Startup error (G8): `module "external-route": query "route" disposition Proceed("fallback_pool") names no entry in defaults`. Same error when the entry exists but does not type-check against the fact it feeds |
| HF-12 | `external-route` declares `(Declined, Reject(603))` | Startup error (G8): `module "external-route": query "route" disposition Reject(603) is outside the permitted status set {403, 404, 480, 500, 503}; 6xx is forbidden`. `Reject(486)` fails the same way, on the set membership rather than on the 6xx clause |
| HF-13 | `external-route` declares `(ServerError, Proceed("fallback_pool"))` with `defaults: &[("fallback_pool", Pool("carrier-z"))]`, and no trunk `carrier-z` is configured | Startup error (G8): `module "external-route": query "route" default "fallback_pool" names pool "carrier-z", which is not a configured trunk`. The closed world is checked at the boundary the answer would cross, so an oracle — or a fallback — cannot name an egress into existence |
