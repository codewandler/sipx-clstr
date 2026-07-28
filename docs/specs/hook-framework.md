# Spec: Hook framework

**Status:** normative · **Crate:** _future — created by CX-2_ · **Stories:** EX-1, EX-3 ·
**Design:** [extension-framework](../designs/extension-framework.md)

## 1. Normative references

- RFC 3261 §16 (proxy behavior — the pipeline this framework attaches to, via
  [proxy-behavior](proxy-behavior.md)), §10.3 (registrar processing), §19.2 (option tags),
  §20.5 (`Allow`), §20.29 (`Proxy-Require`), §20.37 (`Supported`), §20.40 (`Unsupported`),
  §22 (authentication challenges: `401` from the registrar role, `407` from the proxy role).
- RFC 3327 (`Path`, option tag `path`) and RFC 4028 (`Session-Expires`, option tag `timer`,
  method `UPDATE`) — used as example modules in the vectors; their behavior is specified by
  their own future stories, not here.
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

† `query` is a suspension (§4); its stateless-mode interaction is rule E5.

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
  own `ResolveTargets`/`TargetsResolved` (proxy spec §2).
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

## 5. Module state — the three classes

The design's central risk: "declared module state" must not become a euphemism for a dialog
database. **[sipx-clstr] rule — a module may declare state in exactly three classes; nothing
else exists:**

| Class | Declaration | Lifetime & access | Bound |
|---|---|---|---|
| **(a) Request-scoped scratch** | `Scratch { key, ty, published }` | Lives in the request's response context; dies with the transaction. `published: false` = module-private; `true` = readable by later modules as a fact. Never serialized, never crosses a node | Type-checked at compile time |
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

Rationale: G3's tiebreak makes the order a function of the module set alone — the same
profile computes the same order on every node and every boot. Two edges running different
module orders are two different proxies, which the north star (a cluster indistinguishable
from one correct proxy) forbids.

## 8. Profiles

A **profile** is a named set of module ids plus role flags (proxy, registrar). This spec
governs the *graph* validation of that set: G1–G6 run per profile at startup, and a node
boots only if every profile it is configured to serve validates.

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
`path`; owns `Path`).

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
