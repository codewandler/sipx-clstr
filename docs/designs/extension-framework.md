# Design: Extension framework & RFC registry

**Status:** proposed · **Pillar:** Platform · **Epic:** `extension-framework` ·
**Stories:** EX-1 … EX-5

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

## Alternatives considered

- **Free-form middleware chain (onion model).** Rejected: unordered, untyped interception makes
  extension interaction emergent behavior; SIP extensions interact through headers and state, and
  the framework must see those interactions to check them.
- **Generate behavior from ABNF/registry.** Rejected as impossible in general — normative
  behavior (state machines, timer rules, trust requirements) is prose; the registry generates
  syntax and *tracks* behavior.
- **Everything enabled, always.** Rejected: some RFCs define alternative or trust-domain-bound
  behavior; "all extensions on" is not a coherent SIP profile.

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

## Acceptance / done

The union of EX-1 … EX-5: `docs/specs/hook-framework.md` and `docs/specs/rfc-registry.md`; the
hook runtime executing a declared module graph in the harness; codegen producing at least the M3
syntax set from registry data; profile validation rejecting a deliberately conflicting set in a
test; and the demonstration that a syntax-only RFC lands as a registry entry with no hand-written
parser code.
