# Spec: Hook framework

**Status:** normative · **Crate:** _future — created by CX-2_ ·
**Stories:** EX-1, EX-8, EX-7/EX-12 (§6 body claims, §7 G9–G14, §8.1, §9.1) ·
**Implemented by:** EX-3 ·
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
- RFC 8866 §5–6 (SDP: the session-level and media-level grammar §8.1's `SdpScope` ranges over)
  and §6.7 (the direction attributes `sendrecv`/`sendonly`/`recvonly`/`inactive`, and the rule
  that their absence at media level defaults to the session level rather than to nothing — why
  `EnsureExplicit` materializes a value and never overwrites a negotiated one).
- RFC 3329 §5 (`Security-Client`, `Security-Server`, `Security-Verify` — catalogue rows §8.1's
  shipped `sec-agree-headers` profile writes), RFC 5009 (`P-Early-Media`) and RFC 3455 §4.3–4.6
  (`P-Charging-Vector`, `P-Charging-Function-Addresses`) — catalogue rows only; this spec governs
  *whether a profile may write them*, never what a peer does with them.
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
  `headers_owned`; `RewriteBody` only what its `body` claims cover — a `Replace(t)` claim
  permits replacing a body of media type `t` whole, a `Field(t, f)` claim permits writing field
  `f` of such a body and nothing else of it; `ContributeTokenFact` only its declared fact ids
  within their byte bounds. A violating effect at runtime is a bug the harness must catch, but
  the *declaration* is checked at startup (§7 G6).
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
    `t_forward + timerC` and never `t_invite + timerC`. A suspension at H3, H5 or H7 precedes
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
| **(b) Token-carried facts** | `TokenFact { id, max_bytes }` | Contributed at H9 into the token minted at proxy spec F4; returned verbatim inside the token verdict on every mid-dialog request, on any edge, zero lookups | Σ `max_bytes` over the profile ≤ the module-fact sub-budget of the affinity token, carried inside the ≤ 200-byte token parameter (proxy spec F4). **The budget authority is [affinity-token](affinity-token.md) §3**, which fixes the sub-budget normatively; this spec sets no value of its own and owns only the summation over the selected module set (G5) |
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
    pub body: &'static [BodyClaim],                   // RewriteBody scope · exclusivity per G9
}

/// What a module claims of a body. Ownership is per claim *kind* and per field, the same
/// refinement `headers_owned` already embodies: ownership is per header *name*, never "all
/// headers".
pub enum BodyClaim {
    Replace(MediaType),                 // exclusive per (media type, phase)
    Field(MediaType, CatalogSdpField),  // exclusive per (media type, field, phase)
}
```

**A whole-body replacement and a field write are different claims, and a flat media-type list
could not tell them apart.** A relay that takes complete SDP and returns rewritten SDP as opaque
bytes (`media-anchor`; [media-relay](media-relay.md) §3.2 O3) and a module that materializes one
SDP field (`carrier-quirks`, §8.1) would both have declared `media_types_rewritten:
["application/sdp"]`, which G2 reads as an exclusive claim and therefore as an implicit conflict —
so **a deployment could not anchor media and run an SDP quirk at the same time**, which is the
common case rather than an exotic one. Split into `Replace` and `Field`, the two coexist, and G9
gives them the ordering the combination requires.

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
    syntax: SyntaxDecl { body: &[BodyClaim::Replace("application/sdp")], ..SyntaxDecl::NONE },
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
| G2 | **Conflict rejection.** No selected module names a `conflicts` capability that another selected module provides (symmetric — either side's declaration suffices). Exclusive claims are implicit conflicts: two modules owning the same header, or advertising the same option tag. Body claims are exclusive too, but per claim kind rather than per media type — G9 states them, and this rule defers to it | Startup error naming both modules and the contested capability/claim (HF-3, HF-6) |
| G3 | **Deterministic total order per phase.** Topological sort of the `Before`/`After` constraint edges; a constraint naming a capability absent from the profile is ignored (it constrains nothing). **Tiebreak:** among modules with no unsatisfied constraint, the byte-wise lexicographically smallest `id` runs first — repeatedly. The resulting order is a pure function of the selected set | Cycle → startup error listing the cycle (HF-8) |
| G4 | **Capability advertisement is derived, never hand-listed.** `Supported` = the union of `option_tags_advertised` over selected modules. `Allow` = the union of `methods_advertised` plus only what the engine itself terminates (`CANCEL` per proxy spec C1; `REGISTER` when the registrar role is active). No configuration key can append a bare string to either. The derived `Supported` set **is** the acceptance set for proxy spec V6: a `Proxy-Require` tag outside it → `420` with `Unsupported` listing the offenders | — (derivation; HF-5) |
| G5 | **Token budget.** Σ `TokenFact.max_bytes` over the selected set ≤ the module-fact sub-budget, which [affinity-token](affinity-token.md) §3 fixes normatively (§5 class b). This rule owns the summation, never the bound: if a future token version renegotiates the sub-budget, §3 changes and this rule follows it unmodified — the value is read from the authority, not restated here | Startup error naming the modules and the sum (HF-7) |
| G6 | **Effect and scope legality.** Every `HookDecl.effects` ⊆ its phase's closed set (§3 table); `headers_owned` respects E2; every `ContributeTokenFact`, `Query`, timer and patch scope refers to a declared manifest entry; `option_tags_consumed` are advertised by some selected module (self counts) | Startup error naming the module, phase and effect (HF-4) |
| G7 | **The outcome map is total, and the budget holds.** Every `QueryDecl.on` covers all seven `QueryOutcome` variants (§4) exactly once; `Answered` maps to `Apply` and no other arm does; `retries` ≤ 1; every declared `timeout` is > 0 and ≤ the profile's `hook_budget` (§8); and for each of the two request paths, Σ `timeout` ≤ `hook_budget`, summed over every selected module's query declared at each query-permitting phase on that path — proxy: H3, H7, H9, H11; registrar: H3, H5, H11. Within a phase the terms sum because queries there run serially (Q5); H9 contributes once because branches overlap in wall-clock rather than summing | Startup error naming the module, the query id and the missing or duplicated outcome, or the offending sum (HF-9, HF-10) |
| G8 | **Dispositions resolve.** Every `Proceed(DefaultRef)` names a `defaults` entry that exists, type-checks against the fact it feeds, and satisfies the **closed world** — a pool that is a configured trunk, an identity the trunk's asserted-identity and privacy policy admits (RT-7); every `Reject(Status)` is in the closed set `{403, 404, 480, 500, 503}`, and **6xx is forbidden**; a `Reject` arm is declared only for a query whose `phases` all permit the `Reject` effect (H3, H5, H7 — see §3's † footnote), and at H9 and H11 every non-`Answered` arm is therefore `Proceed(_)` or `ProceedWithout`; `ProceedWithout` appears only where the consuming `Scratch` (§5 class (a)) is declared optional | Startup error naming the module, the query id and the offending disposition (HF-11, HF-12, HF-13) |
| G9 | **Body claims are kind-scoped and ordered.** `Replace(t)` is exclusive per `(t, phase)`; `Field(t, f)` is exclusive per `(t, f, phase)`; a `Replace` and a `Field` on the same media type coexist, and every `Field` writer is ordered after every `Replace` writer at that phase by the framework | Startup error naming both modules and the contested claim (QP-G-8, QP-G-9) |
| G10 | **Bindings resolve and compose disjointly.** Every bound profile id, trunk, domain and `ConfigKey` exists and type-checks (and names the side its binding is on — G14); every literal parses against its row's ABNF; every rule names a catalogue row; every field's op is in that row's op set; the composed rule set at one attachment point — one `trunk[]` or one `domain[]` entry, and never the two together (§8.1) — is disjoint on targets, a target being one catalogue row at one *elementary* message class, so two rules contest on every elementary class their classes share, once the overrides declared at that binding (G13) have been applied | Startup error naming the profile, the rule and the offending id, value or contested target (QP-G-1 … QP-G-5) |
| G11 | **Media assertions hold.** Every `MediaAssertion` on a trunk-bound profile is satisfied by the bound trunk's declared `SrtpPolicy` — `Srtp` holds for any variant but `Disabled`. The same check as [media-relay](media-relay.md)'s G-M5; the error content is G-M5's, not restated richer here | Startup error naming the trunk and the profile (QP-G-6, = media-relay's MR-C-3) |
| G12 | **Media assertions need a trunk.** A profile carrying a non-empty `requires_media` bound to a domain is rejected: only a trunk binding has a `TrunkMediaPolicy` to check it against | Startup error naming the profile and the domain (QP-G-11) |
| G13 | **An override resolves one live contest, and reaches nothing else.** An override is declared at a **binding**, never in a profile, and names one contested `target` and one `winner` profile id. At startup that target must actually be contested at that attachment point by two or more distinct profiles bound **to that same object**, and the winner must be one of them — a winner bound only at the other attachment point is rejected by this rule and not accommodated by it, because no contest spans the two (§8.1, QP-G-17). Where three or more profiles contest one target, one entry names one winner and every other contesting profile's rule for that target is dropped (QP-G-18). The losing rules for that target are deleted from the composed set before any message is evaluated, so what G10 checks for disjointness is the set after overrides. A `requires_media` assertion has no target, so no override can name one and none suppresses G11 or G12 | Startup error naming the binding, the target, and the winner where the winner is not among the profiles contesting it (QP-G-13 … QP-G-15, QP-G-17; the resolving cases are QP-G-3, QP-G-12 and QP-G-18) |
| G14 | **A rule's config leaves name the side its binding is on.** `ValueLeaf::TrunkConfig` may appear in a profile bound to a **trunk** and `ValueLeaf::DomainConfig` in a profile bound to a **domain**; each mirror case is rejected. Checked per **binding**, never per profile, so a catalogue profile reading trunk configuration may be bound to any number of trunks and to no domain, and a profile with no side-specific leaf and no `requires_media` — `sdp-direction-explicit` — is bindable at both. This is G12's shape for the same reason: a leg evaluates one attachment object, so a leaf naming the other side is unresolvable at every evaluation rather than at some, which makes it a boot failure and not a runtime case | Startup error naming the binding, the profile, the rule and the key (QP-G-16) |

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

G9–G14 extend the same discipline to §8.1's quirk profiles, and every one of them is a *startup*
rule for the same reason G1–G8 are: an invalid combination fails deployment, never a call. Two of
them are worth their rationale here. **G9's ordering is computed, not declared**, because a field
write onto a body that is about to be replaced is silently lost — and "silently" is the
disqualifying word: it would be a correct-looking manifest producing a message the vectors did not
predict. Leaving it to an `After("media-anchoring")` constraint in the field writer's own manifest
would work exactly until someone wrote a second replacer, which is what G-rules are for.
**G13 refuses an override that resolves nothing** because an override that has outlived its
contest — the other profile unbound, or its rules changed — is a profile that silently stops
applying long after anyone remembers binding it. An implicit "more specific wins" is a policy
nobody wrote down; refusing to boot is the only version of it a reader can check.

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

### 8.1 Quirk profiles — the data G10–G14 range over

A **quirk profile** is a versioned, shipped-catalogue value carrying header and SDP rules for one
peer's deviations. It is the vocabulary G10–G14 check; the module that executes it,
`carrier-quirks`, is an ordinary module under this spec (§9's cast).

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
    TrunkConfig(ConfigKey),    // a typed value from the bound trunk's configuration — G14
    DomainConfig(ConfigKey),   // a typed value from the bound domain's configuration — G14
}

pub enum SdpScope    { Session, Media(MediaKind) }
pub enum MediaKind   { Audio, Video, Application, Image, Text }
pub enum SdpOp       { EnsureExplicit, Set(ValueLeaf) }
pub enum CatalogHeader   { SecurityClient, SecurityServer, SecurityVerify,
                           PEarlyMedia, PChargingVector, PChargingFunctionAddresses }
pub enum CatalogSdpField { Direction, SessionName }
pub enum MediaAssertion  { Srtp }   // any SrtpPolicy but Disabled — media-relay §13.1
```

**A profile is data, and only data.** There is no `Fact` leaf and no selector: a value drawn from
the message would make the transform a function of the message — a condition wearing a data
structure — and a profile that knew where it applied would put "which transform did this peer get"
back out of reach of the configuration file. `CatalogHeader` is deliberately not `HeaderName` and
`CatalogSdpField` is deliberately not a string, so *which* things a quirk may write has a
type-level answer rather than a documented convention. A quirk transforms and never decides:
`carrier-quirks` declares `PatchHeaders`, `RewriteBody`, `Annotate` and `Record` and no others, so
it never suspends and never draws on `hook_budget` (G7).

**Binding.** Configuration binds a profile to a **trunk** ([routing-trunks](../designs/routing-trunks.md) —
the egress peer) or to a **domain** (the registering side). There is no third attachment point.

```toml
[trunks.trunk-a]
quirks = ["sec-agree-headers", "sdp-direction-explicit"]

[trunks.trunk-a.quirk_config.sec-agree-headers]
security_client = "sdes-srtp;mediasec"     # parsed at startup against the row's ABNF
security_verify = "sdes-srtp;mediasec"

[domains."example.net"]
quirks = ["sdp-direction-explicit"]

[[trunks.trunk-b.quirk_overrides]]
target = { header = "P-Charging-Vector", on = "request:INVITE" }
winner = "peer-b-charging"
```

The SDP form of `target` names a scope and a field instead of a header —
`{ sdp_scope = "media:audio", sdp_field = "Direction", on = "response:INVITE:2xx" }` — and is
otherwise the same entry.

**The composition is per attachment object.** The composed rule set at one attachment point is
every profile bound to **that one object** — one `trunk[]` entry or one `domain[]` entry
([cluster-config](cluster-config.md) §7 S4) — and never a trunk's rules together with a domain's.
The two sets never intersect: a leg evaluates one attachment object, so no message ever carries
both attachments, and there is no composition spanning the two for a rule to be disjoint over.
One profile bound at both points contributes its rules once to each composed set and contests
nothing; a contest needs two distinct profiles at one object.

**A target is elementary.** A target is one catalogue row at one *elementary* message class:
`(CatalogHeader, ElementaryClass)` or `(SdpScope, CatalogSdpField, ElementaryClass)`, where an
elementary class is one method for a request and one `(method, status class)` pair for a response.
A rule's `MessageClass` ranges over a **set** of those, and two rules contest on every elementary
class their sets share. Stated as tuple equality instead, `Request { methods: [INVITE, REGISTER] }`
and `Request { methods: [INVITE] }` would be different targets, the G10 disjointness check would be
defeated by widening a method set, and an override could not name what it was conceding.

**A quirk asserts media policy; it never assigns it.** There is no op for `m=` proto, `a=crypto`,
ICE candidates or transport addresses — they are not catalogue rows and cannot become rule targets.
SRTP mode, codecs and transcoding are the trunk's `TrunkMediaPolicy`
([media-relay](media-relay.md) §13.1, MP11). `MediaAssertion::Srtp` is satisfied by any
`SrtpPolicy` but `Disabled`: the assertion is "this leg runs some SRTP mode", not "this leg runs
`Sdes`", because asserting a mechanism would let a profile pick the keying method by which profile
matched — the per-call pattern MP6 forbids.

**Where rules run.** One egress point per direction: `BeforeForward` (H9) for a request, per
branch, and `BeforeResponseForward` (H11) for a response. H9 is per branch deliberately — a fork
can have two branches on two trunks wanting two different header sets.

**The shipped catalogue, v1.** Two profiles, and the worked example is the two composed on one
trunk, so the shipped set demonstrates composition rather than asserting it. Adding a profile is a
configuration change plus a vector.

| Profile | Rules | Vectors |
|---|---|---|
| `sec-agree-headers` | `Set(Security-Client)` and `Set(Security-Verify)` from trunk config, on a declared method set; `requires_media: [Srtp]` | QP-A-1 … QP-A-4, QP-G-4, QP-G-6, QP-G-7, QP-G-11, QP-G-15, QP-G-16 |
| `sdp-direction-explicit` | `EnsureExplicit(Direction)` at `Session` and `Media(Audio)`, on requests and responses carrying a body | QP-A-5 … QP-A-9, QP-C-1, QP-C-3 |

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
against `Store("geo")` at `BeforeForward`, effects `Annotate`), `carrier-quirks` (§8.1's executor:
owns the six `CatalogHeader` rows as `headers_owned`; declares
`body: &[BodyClaim::Field("application/sdp", Direction), BodyClaim::Field("application/sdp",
SessionName)]`; hooks `BeforeForward` and `BeforeResponseForward` with effects `PatchHeaders`,
`RewriteBody`, `Annotate`, `Record`; no `QueryDecl`).

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
| HF-7 | `TokenFact` declarations summing to 72 bytes against the 64-byte sub-budget ([affinity-token](affinity-token.md) §3 fixes the 64) | Startup error (G5) naming the contributing modules and the 72 > 64 sum |
| HF-8 | `a-mod` declares `Before("b-cap")`, `b-mod` (provides `b-cap`) declares `Before("a-cap")` (provided by `a-mod`), same phase | Startup error (G3): ordering cycle `a-mod → b-mod → a-mod` listed |
| HF-9 | `external-route`'s `on` omits the `ServerError` arm; the other six are declared | Startup error (G7): `module "external-route": query "route" declares no disposition for outcome ServerError; the outcome map must be total`. The omission is **not** read as "proceed", "reject" or any other default — a profile that cannot say what happens does not boot |
| HF-10 | `external-route` and `geo-tag` declare queries at `BeforeTargetResolution` and `BeforeForward` with `timeout` 1500 ms each, both manifests otherwise valid, `hook_budget` 2000 ms | Startup error (G7): `invalid profile "EdgeProxy": queries on the proxy path sum to 3000 ms over hook_budget 2000 ms (external-route/"route" at BeforeTargetResolution 1500 ms, geo-tag/"lookup" at BeforeForward 1500 ms)`. Both modules, both phases and both terms named — the sum is not attributable to either alone |
| HF-11 | `external-route` declares `(ServerError, Proceed("fallback_pool"))` and an empty `defaults` | Startup error (G8): `module "external-route": query "route" disposition Proceed("fallback_pool") names no entry in defaults`. Same error when the entry exists but does not type-check against the fact it feeds |
| HF-12 | `external-route` declares `(Declined, Reject(603))` | Startup error (G8): `module "external-route": query "route" disposition Reject(603) is outside the permitted status set {403, 404, 480, 500, 503}; 6xx is forbidden`. `Reject(486)` fails the same way, on the set membership rather than on the 6xx clause |
| HF-13 | `external-route` declares `(ServerError, Proceed("fallback_pool"))` with `defaults: &[("fallback_pool", Pool("carrier-z"))]`, and no trunk `carrier-z` is configured | Startup error (G8): `module "external-route": query "route" default "fallback_pool" names pool "carrier-z", which is not a configured trunk`. The closed world is checked at the boundary the answer would cross, so an oracle — or a fallback — cannot name an egress into existence |

### 9.1 Carrier quirk profiles — `QP`

The `QP` rows run against §8.1: `QP-A` applies one profile to one message, `QP-C` composes several,
and `QP-G` is startup validation against G9–G14 with profiles and bindings as given. Every "message
unchanged" expectation is a **byte** expectation, which the kernel's lossless model makes meaningful:
an untouched byte re-serializes verbatim (§1, PX-3).

These rows lived in [extension-framework](../designs/extension-framework.md) until `EX-12` moved
them here. That was not tidying: `scripts/check-vectors.py` reads rows only out of the spec that
*owns* a prefix, so while they sat in a design record no gate could read them, and a fabricated row
inserted among them passed `--check` untouched. A vector table in a design record is prose that
looks like a measurement.

(A row ID is deliberately not spelled out in that sentence: the gate reads row IDs from anywhere in
an owning spec, so naming a fabricated one here would conjure the very row it describes.)

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
| QP-C-2 | One INVITE transaction from a registrant of `example.net` out over `trunk-a`, with a profile bound to each — enumeration row 4 | Both applied, to **two different messages**: the trunk-bound profile writes the forwarded request at H9, the domain-bound one the forwarded response at H11. The trace names both, each against the leg it ran on, and no composed set contains rules from both bindings. This row asserted "both apply to one message" until `EX-11` derived that no leg carries both attachments |
| QP-C-3 | `media-anchor` selected alongside `sdp-direction-explicit` | The relay's replacement body is produced first, the field write lands on **it**, and the forwarded body carries both the relay's `c=`/`m=` and the materialized direction. The G9 ordering assertion |
| QP-C-4 | A domain-bound and a trunk-bound profile writing the **same** elementary target — `(P-Charging-Vector, request:INVITE)` — with no override anywhere | **Boots**, and each applies on its own leg (QP-C-2's shape, same target). The row that fails if the derived condition is violated: under the union reading this is a G10 startup error, and G13 cannot repair it, because an override is declared at one binding and its winner must be contesting *there* |

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
| QP-G-16 | `sec-agree-headers`, whose `Set(Security-Client)` reads `ValueLeaf::TrunkConfig`, bound to a **domain**; and the mirror — a profile carrying `ValueLeaf::DomainConfig` bound to a trunk | Startup error (G14) naming the binding, the profile, the rule and the key: a domain binding has no trunk configuration to read, at any evaluation. Two independent reasons reject this profile at a domain — G12 for its `requires_media` and G14 for its leaves — and G14 is the one that survives if a future profile drops the assertion |
| QP-G-17 | QP-C-4's configuration, plus a `quirkOverrides` entry at `trunk-a` naming that target with the **domain**-bound profile as `winner` — the operator reading the composition as a union and trying to resolve it | Startup error (G13) naming the binding, the target and the winner: at `trunk-a` the target is written by one profile, and the winner is bound elsewhere. A specialization of QP-G-13 rather than a new rule, and the row exists because the union reading would have made this entry the *repair* for QP-C-4 instead of an error — and G13 would have rejected it anyway, which is what made the union reading unrepairable |
| QP-G-18 | Three profiles bound to `trunk-b` all writing `(P-Charging-Vector, request:INVITE)`, with one override naming one `winner` | Boots. The winner's rule is in the composed set, **both** losers' rules for that target are not, and the composition record names the override once. A `winner` is a single profile id, so a three-way contest needs no second entry |