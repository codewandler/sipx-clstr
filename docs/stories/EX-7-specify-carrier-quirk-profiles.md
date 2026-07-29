---
id: EX-7
title: Specify carrier quirk profiles
pillar: Platform
status: in-progress
priority: 
design: docs/designs/extension-framework.md
epic: extension-framework
areas: [hooks, trunks]
note: 
---

# Specify carrier quirk profiles

## Goal
Give per-peer protocol quirks — header injection and SDP body rewriting — a bounded, declarative vocabulary, so accommodating a carrier is configuration with a test vector rather than a patch.

## Acceptance
- [x] A quirk profile is data: which peers it applies to, which headers it adds on which methods, and which SDP rewrites it performs. — [extension-framework](../designs/extension-framework.md) § *The profile — data, and only data* (`QuirkProfile`, `HeaderRule`, `SdpRule`, `MessageClass`) and § *Binding — which peers, and several at once*.
- [x] The vocabulary is bounded — it must not become general scripting embedded in config. — § *The bound — stated so it can be checked rather than believed* (B1–B4 with the check per property, the anti-catalogue table, and rule P3 in § *The catalogues*).
- [x] Profiles attach to a trunk or a domain, and several may apply. — § *Binding*: two attachment points, disjoint union over the composed set, `overrides` as the only escape (G10; vectors QP-C-1, QP-C-2, QP-G-2, QP-G-3).
- [x] Each shipped profile carries a test vector; adding one is a config change plus a vector. — § *The shipped catalogue, and its vectors*: two shipped profiles, 22 `QP` rows, and the stated asymmetry — a peer is config + a vector, a new *kind* of quirk is a catalogue row + a spec change + a vector.
- [x] Interaction with media policy (e.g. a quirk that also implies SRTP) is specified. — § *Media policy — a quirk asserts, and never configures*: `requires_media`, G11, and the named ME-6 boundary.

## Progress
- 2026-07-29 — **specified** in [extension-framework](../designs/extension-framework.md)
  § *EX-7: carrier quirk profiles*. Zero Rust: this story's deliverable is the record, the same
  posture `EX-6` took in the same design doc.
- **The bound is the deliverable.** A quirk profile is a finite set of assignments, not a program.
  Four properties (B1–B4) make that checkable against the *types* rather than the semantics:
  non-recursive grammar of fixed depth, no environment, no condition inside a rule, no iteration
  and no positional addressing. Consequences asserted by vectors: evaluation is total and O(rules),
  idempotent, and confluent — which is what makes "several profiles may apply" safe.
- **One module, many profiles.** Modules are statically compiled (hook-framework §2), so a quirk
  cannot be a module without making every new peer a code change. `carrier-quirks` carries the
  vocabulary; profiles are data; configuration binds them to a trunk or a domain.
- **Two closed catalogues, small on purpose** — RFC 3329 `Security-*`, RFC 5009 `P-Early-Media`,
  RFC 3455 `P-Charging-*`; and SDP `Direction` (`EnsureExplicit` only) and `SessionName`. Rule P3:
  a quirk may materialize an RFC default or carry an out-of-band value, and may **never** overwrite
  a value an endpoint negotiated — which is why the live "rewrite to `a=sendrecv`" requirement is
  catalogued as `EnsureExplicit` and `Set(Direction)` does not exist in the grammar.
- **No new hook phase and no new effect.** H2/H9/H11 already permit everything the module emits;
  the H-table and the effect enum are untouched.
- **One structural spec change is required, and it is not a phase**:
  `SyntaxDecl.media_types_rewritten` must split into `BodyClaim::{Replace, Field}`, because
  `media-anchor` replaces the whole body (media-relay §3.2 O3) while a quirk writes a named field,
  and G2's media-type exclusivity currently makes anchoring + an SDP quirk a startup conflict.
  Handed to [EX-8](EX-8-make-the-async-query-declaration-normative.md) with G9/G10/G11 rather than
  edited into the closed `hook-framework` spec.
- **Media policy**: a quirk cannot configure media (no `m=` proto, `a=crypto`, ICE or transport
  address is a catalogue row). It may *assert* — `requires_media: [Srtp(..)]`, checked at startup
  against the bound trunk's `ME-6` policy (G11). EX-7 owns the assertion and the check; ME-6 owns
  the value. The one thing EX-7 needs from ME-6 is that per-trunk SRTP mode is a declared enum
  readable at startup; nothing was written into `media-control.md`.
- **Considered for upstream: no, cluster-specific** — quirk profiles are per-peer deployment policy
  over this platform's trunk/domain objects and hook manifest; the kernel has no trunks, domains or
  hooks. The header surgery is the kernel's already (`S-15`, landed 0.4.0, via PX-3). One row *is*
  implied — a field-addressable SDP model — and it is ME-4's to file, on media-control's own stated
  trigger; the requirement is written out in the design so ME-4 inherits it.
- **Not filed by this story, for the coordinator:** an `sec-agree` *negotiation* module (RFC 3329
  §2/§5 is behaviour, not a quirk); registering the `QP` prefix in the vector gate is
  [CF-8](CF-8-bring-every-spec-under-the-vector-gate.md)'s.

## Notes
- One deployment has a live example needing `mediasec` headers on REGISTER and INVITE plus an SDP `a=sendrecv` rewrite. It is currently an inline domain test in the routing script.
- The requirement is the mechanism, not that specific carrier — more will follow.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-5**). The evidence sits in that deployment's own reference material.
