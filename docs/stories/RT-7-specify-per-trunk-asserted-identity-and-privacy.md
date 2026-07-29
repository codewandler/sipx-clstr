---
id: RT-7
title: Specify per-trunk asserted identity and privacy policy
pillar: Signalling
status: in-progress
priority: 2
design: docs/designs/routing-trunks.md
epic: routing-trunks
areas: [routing, trunks, privacy]
note: 
---

# Specify per-trunk asserted identity and privacy policy

## Goal
Make P-Asserted-Identity synthesis and privacy handling a per-trunk policy, since what a carrier requires differs per carrier and is a compliance obligation.

## Acceptance
- [x] A trunk declares whether PAI is asserted, what identity is used, and what happens for an
      anonymous caller. → [asserted-identity](../specs/asserted-identity.md) §4's
      `TrunkIdentityPolicy { trust, assert, privacy }`. *Whether*: `Assert::{Never, Identity}`,
      with `A19` keeping it a statement about synthesis alone and §8's gate deciding separately
      whether an identity in hand travels — RFC 3325 splits creation (§5) from forwarding (§5, §7)
      and so does this. *Which*: `A14`'s ordered `IdentitySource` list —
      `Principal` (registrar-auth §5), `PreferredHint` (RFC 3325 §6, checked against `A5`'s
      assertable set), `ReceivedPai`, `ReceivedFrom`, `Literal` — with `A15`'s `form` filtering
      and never fabricating. *Anonymous caller*: §9, and `A25` in particular. Vectors `AI-S-1…12`,
      `AI-T-13`, `AI-T-14`.
- [x] `Privacy` handling for anonymous callers is declared, including a fallback identity when none
      is available. → `PrivacyEgress { when_unspecified, user_privacy, announce_id,
      on_unperformable }` (§4), with the gate `A20`–`A24` and `A34`, `AI-T-1…11` exhaustive over
      `PeerTrust` × `PaiRequest` and `AI-D-1…7` proving that derivation total over the raw header.
      The fallback identity is `IdentitySource::Literal`, and `A17` names what it actually is — the
      platform's own identity, not the caller's. RFC 3324 §2.2's requirement on it is an **operator
      obligation**, not a startup check: `G-A6` checks only RFC 3325 §9.1's shape, because nothing
      at load can distinguish a deployment's own presentation number from a guess (§12). That is
      why every `Literal` is reproduced verbatim in §12's effective-policy record and flagged per
      branch — a regulatory trace built on it traces here rather than to a subscriber. `A18`'s
      `on_none` covers a deployment that declared no literal. `G-A2` refuses a policy that omits
      `when_unspecified`, which is RFC 3325 §1 `Spec(T)` item 5 — read wider than §7 words it, and
      `A23` says why.
- [x] Interaction with the anonymous-From rule (RFC 5379 §5.1.4) is specified, not left to
      ordering. → §9.1. RFC 5379 §4.1 Table 1 is the argument: `From` is `anonymize` under `user`
      only, `P-Asserted-Identity` is `delete` under `header` and `id` only, so the two rows share
      no column. `A25` makes the composition a **product** — the spec fixes outputs, and `AI-A-6`
      asserts the byte-identical result under either order. The one real coupling is closed by
      construction in `A10` (every source reads ingress facts, so a `From` about to be anonymised
      can never feed the identity asserted on that branch). Separately, `A9` fixes the order
      against *normalisation*, where order genuinely is observable: `AI-N-6`.
- [x] Test vectors per policy combination. → §13, 97 rows in eight families: `AI-D` (7, the
      `PaiRequest` derivation), `AI-T` (17, the emission gate), `AI-S` (12, selection), `AI-A`
      (24, anonymous callers, the `From`, and the response direction), `AI-N` (7, the
      normalisation seam), `AI-C` (12, `critical`), `AI-P` (11, pipeline and binding),
      `AI-X` (7, byte-exact). §13's preamble
      states what is proved instead of a raw cross product: the privacy axis is enumerated as
      `PaiRequest` (3 values, total derivation) rather than as header text, so `AI-T` walking all
      `2 × 3` cells plus `AI-D` proving the derivation total *is* exhaustiveness over every
      `Privacy` header a caller can write. `A19`, `A25` and `A10` establish that the source list
      and the `From` question are independent. Registration in the vector gate is deferred — see
      *Progress*.

## Progress
- **Spec written: [asserted-identity](../specs/asserted-identity.md)**, new and normative — 35
  rules `A1`–`A35`, 8 startup checks `G-A1`–`G-A8`, 97 vectors. AGENTS.md rule 4's test: normative
  references, types, state rules and a test-vector table, not prose.
- **Where creation sits, and why it is a rule rather than an implementation detail.** `A8` fixes
  three egress steps inside the window `N23` already reserves between F2 and F5 — **E1** identity,
  **E2** normalisation, **E3** presentation — and `A9` is the criterion both placements follow: a
  step runs *before* the trunk's normalisation profile iff its output is a number whose shape is
  the trunk's business, and *after* iff its output is a constant an RFC fixes byte for byte. So a
  created PAI is shaped by the trunk's own profile (`AI-N-3`) rather than the E.164 rule being
  restated inside the identity policy, and RFC 5379 §5.1.4's `From` form is written last and
  unshaped (`AI-N-5`). `AI-N-6` is what makes it a rule: under the reverse order, an anonymised
  `From` meeting a `{ e164: global, fallback: to }` guard picks up the **callee's** number via
  `N17`. The ordering is a privacy control.
- **The `N32` seam closes at a point, not an overlap.** Normalisation never creates the field;
  `A8` guarantees E1 has already settled whether it exists and whose identity it names before E2
  runs. So `NN-G-10`/`NN-G-11`'s `Skipped { GuardedFieldAbsent }` becomes the trunk's view of a
  decision already taken (`assert: never`, or the gate withheld) rather than an open question —
  `AI-N-1`, `AI-N-2` pin it from this side. `AI-N-4` covers the case that motivated the ordering:
  a withheld PAI never reaches E2, so a guarded profile cannot fail a branch over a header that
  was never going to leave. **`number-normalisation.md` needs no amendment**; it was read and not
  written.
- **`Privacy: critical`: supported, bounded, declared** (`A29`–`A32`). This platform is not a
  privacy service — RFC 3323 §5.1 puts that in a B2BUA and `PX-8` declined it for v1 — so
  RFC 3323 §5's `MUST` does not literally bind it. Declining on that technicality was rejected: a
  caller who wrote `critical` asked to be rejected rather than exposed, and forwarding is the one
  outcome they ruled out. The performable set is `{ id }` — unconditionally, and `A29` gives the
  RFC 3325 §9.3 scoping reason rather than asserting it — plus `{ user }` on a trunk declaring
  `user_privacy: perform`, which `A33` and `A35` define over **all eleven** of RFC 5379 §4.1
  Table 1's `user` entries: nine performed across both directions, two declined with a reason.
  `header` and `session` are never claimed, and `session` in particular
  because a privacy claim conditioned on whether a call was anchored is a claim that is sometimes
  false. Anything outside the set with `critical` present is `500` per RFC 3323 §5 with the
  enumerating reason phrase §5 asks for. RFC 5379 §4.3's stronger no-`critical` recommendation is
  **declined by default** and made a per-trunk field (`on_unperformable`) instead, with the reason
  recorded. `Privacy: none;critical` falls out of `A30` as a non-rejection rather than needing a
  special case.
- **The safety rule the review should look at first (`A6`).** A `P-Asserted-Identity` arriving from
  a peer this platform does not trust is removed at ingress, and `Privacy: none` does **not** save
  it. RFC 3325 §7's `MUST NOT remove` governs an identity the platform legitimately holds — §5
  scopes it to fields "that it generated itself, or that it received from a trusted source" — and a
  PAI from an untrusted sender is neither. Without this, `none` is a two-word identity spoof and
  every other rule is decoration. `AI-P-1`, `AI-P-2`.
- **State rides the message (invariant 5).** The two priv-values with egress consequences (`user`,
  `id`) are not re-derivable mid-dialog, because a UA need not repeat `Privacy`. `A12` carries them
  as a **one-octet module fact** in the affinity token — [affinity-token](../specs/affinity-token.md)
  `M8`'s `ContributeTokenFact` seam and its 64-byte sub-budget, gated at startup by `G5` — read
  back through P2's `TokenFact`. `AI-A-11`/`AI-A-12` pin that a mid-dialog BYE at a *different*
  edge decides identically. The alternative, a per-dialog anonymity record, is the store `PX-8`
  rejected. The residual limit (a dialog never Record-Routed has no fact) is stated in §9 rather
  than discovered.
- **Sans-IO (rule 2).** `A4` makes the egress decision
  `fn egress_identity(policy, facts) -> Outcome` — no store, no clock, no socket, no second look at
  the message. Identity is resolved **once** at ingress (H4) and only *selected* at egress, so the
  lookup-at-F5 smell is unrepresentable rather than merely discouraged; a deployment needing more
  gets `EX-6`'s async hook, which suspends at a declared phase.
- **Upstream answer (AGENTS.md rule 6):** considered for upstream — **no for the policy, yes for
  two syntax primitives.** A parsed `Privacy` header over RFC 3323 §4.2's closed `priv-value` set,
  and a typed `P-Asserted-Identity`/`P-Preferred-Identity` value list enforcing RFC 3325 §9.1's
  one-or-two rule and reading the one-line and two-line forms identically (RFC 3261 §7.3.1), are
  header syntax and belong to the kernel on the argument that moved the `Headers` surgery API
  (`S-15`/PX-3). Policy has a *trunk* as its subject and the kernel has no trunks. Recorded in
  spec §2 and the design; both are candidate `CX-1` rows. No [upstream ledger](../upstream.md) row
  was added — that file is fenced this wave.
- **Every RFC citation was checked against the RFC text, not from memory.** Two things that check
  caught are worth knowing. RFC 3325's *Introduction* is **§3**, not §1 — §1 is the
  *Applicability Statement*, and that is where the Trust Domain restriction and the eight-item
  `Spec(T)` list actually live, so §3's trust-domain material is cited as §1 throughout and the
  obvious "§1 is the intro" assumption would have cited the wrong clause. And RFC 3325 §10.1's own examples emit two values
  as **two header lines** with the `tel` value as a bare `addr-spec`, so §7.1 states the emitted
  form explicitly rather than assuming the comma-separated ABNF form is what the wire shows.
- **Reworked after independent review, which found four blocking defects. All four are closed.**
  - **`user` was advertised as performable while only a third of it was performed.** RFC 5379 §4.1
    Table 1's `user` column is not only `From: anonymize` — it also deletes `Call-Info`,
    `In-Reply-To`, `Organization`, `Reply-To`, `Subject` and `User-Agent`, and RFC 3323 §5.3 makes
    that a `MUST`. A trunk could therefore claim `user`, anonymise the `From`, and forward six
    headers naming the caller's software, employer and subject line — and under `A30` that means
    **forwarding a `user;critical` request the caller asked to have rejected**. New `A33` makes
    `UserPrivacy::Perform` all seven things and `Decline` none; the middle value is gone because a
    partial performance is a false claim. `Call-ID` and `Referred-By` are declined *with reasons*
    (a `MAY` behind a B2BUA, and Table 1's circumstance asterisk) — declining a `MAY` is not a
    failure to perform the level. `AI-A-13…16`, `AI-C-4`.
  - **`id`'s unconditional performability looked inconsistent with `A20`.** It is not, and `A29`
    now carries the argument rather than the assertion: RFC 3325 §9.3 scopes `id` to entities
    *outside* the trust domain, so forwarding to a peer inside it honours the token too. Both cells
    of §8.1's `Withhold` column are performances. `user` differs in kind — a trunk may simply not
    do it. `AI-C-11`, `AI-C-12`.
  - **The gate was not total.** Six of sixteen trust × privacy cells were undetermined, so
    `when_absent: remove` as a fail-closed posture was **bypassable by one unrelated priv-value**.
    Introduced `PaiRequest { Withhold, Preserve, Unspecified }` as a total derivation over
    `RequestedPrivacy`, renamed `when_absent` → `when_unspecified` to match what it decides, and
    added §8.1 enumerating all six cells. `A24` is now checkable instead of asserted, and the
    exhaustiveness claim in §13 is a claim about a 3-valued derived type rather than about an open
    set of header text. New `A34` settles the contradictory `none` + `id` header toward not
    disclosing. `AI-D-1…7`, `AI-T-11`.
  - **`A19` told an operator something false.** It said `trust: untrusted` + `remove` meant no PAI
    reaches a peer under any circumstance; `AI-T-8` and `A22` say `none` still emits. `A19` now
    states plainly that **no configuration here gives that guarantee** and names the one that does
    — RT-5's unconditional egress header allowlist — with the reason it is deliberately a separate
    control.
  - Smaller, same pass: the §10.1 citation claimed a byte-for-byte match that `§7.1` itself
    contradicts (it fixes the two-header-line *carriage*, not the bracket form); `A6` re-grounded
    on RFC 3325 §5's **receive-side** `MUST` so §7's unqualified `MUST NOT remove` is no longer
    reinterpreted; `A12`'s `G5` repointed at hook-framework, which is where it lives;
    `A11`'s method set narrowed to the reachable intersection `{INVITE, OPTIONS, SUBSCRIBE,
    REFER}`, with BYE and NOTIFY named as permitted-but-unreachable rather than left as an empty
    branch; `A27` inserts `id` before `critical` per §4.2's construction rule (`AI-X-7`); `G-A6`
    dropped an unimplementable "declared assertion domain" check and §12 now records RFC 3324
    §2.2 as an operator obligation, with §14 naming DP-1 for the `sip` half and **nobody** for the
    `tel` half, because E.164 ownership is not checkable from configuration at all.
  - **Both open questions settled in the text.** `A13`: the response gate reads the **response's
    own** `Privacy` header — a PAI on a response is the *answering* party's identity and the
    caller's `Privacy: id` is not a licence to suppress the callee's, which also means the response
    path needs no state to travel (`AI-P-8`, `AI-P-11`). `A11`: BYE is **not** reachable, and now
    says so.
- **Round-2 review found the first rework incomplete in three places and self-contradictory in a
  fourth. All four closed, and the two open questions are now settled in the spec text.**
  - **`user` was still under-performed — `A33` enumerated nine of Table 1's eleven entries.** I
    fetched RFC 5379 §4.1 rather than trust recall: the `user` column also carries `Server`
    (`r`, delete) and `Warning` (`r`, anonymize), and marks `Call-Info`, `Organization` and
    `Reply-To` `Rr` rather than `R`. Five entries are therefore reachable on a **response** and the
    spec named none, while `A29` declared `user` performable — the same defect the first round was
    raised on. Closed by performing them, not scoping them away: new `A35` deletes `Server`
    (§5.1.12), strips the hostname from `Warning` (§5.1.16), and deletes the three `Rr` headers on
    a response carrying `Privacy: user`. `A33` now enumerates the column explicitly — **nine
    performed, two declined with reasons** — so a reader can count it against the RFC.
  - **`A19` named a mechanism that cannot fire.** RT-5's allowlist is default-deny over the
    platform's *application-header prefix*; `P-Asserted-Identity` carries no such prefix, so an
    allowlist omitting it removes nothing. The first half of `A19` was right and is kept; the
    second half was one false statement traded for another. It now says plainly that **nothing in
    the platform provides that guarantee today** and §14 files the gap. Two further instances of
    the same wrong claim, not in the brief: `A33`'s "seam with RT-5" (the six §5.3 headers are
    standard SIP headers — the field sets are disjoint, there is no seam) and `AI-A-16`, which
    tested RT-5 dropping `User-Agent`, which it also cannot do.
  - **`A5` and `A17` still asserted a `G-A6` check the first rework deleted.** Both told the reader
    that config load enforces RFC 3324 §2.2 on a literal. It does not. `A5` now gives the reason
    the assertable set satisfies §2.2 anyway — RG-1's store holds identities the deployment issued,
    so membership comes from the source, which is exactly why `Literal` is the exception — and
    `A17` says nothing at load can tell a presentation number from a guess.
  - **The acceptance record above described the design it replaced**, citing `when_absent` and
    `anonymous_from`, neither of which appears in the spec. Corrected to the shipped types.
  - **Open question 1 — how `A12`'s token fact reaches the gate.** Once, at `A4`'s ingress step,
    which reconciles the fact and the message's own header into `IdentityFacts.privacy`; §8 and §9
    read that plain value and neither knows the token exists. Stated on the field itself. `AI-T-16`
    pins the mid-dialog re-INVITE (`Withhold`, `when_unspecified` never consulted); `AI-T-17` pins
    the un-Record-Routed dialog that genuinely has no fact.
  - **Open question 2 — whether user-level privacy reaches responses.** Yes, and `A35` says how:
    gated by the **response's own** `Privacy` header, which is `A13`'s rule with the subject
    changed. §5.1.12's own first sentence is "information about the software used by the UAS", so
    these five name the *responding* party; reading the caller's request here would let one party's
    privacy request strip another party's headers. Same payoff as `A13` — no dialog state on the
    response path, so invariant 5 holds without a store. `critical` is untouched: RFC 3323 §4.2
    says criticality cannot be managed for responses, so a response is never rejected (`AI-A-24`).
- **Deferred, and tracked — do not re-file:** the `AI` rows are **not** registered in
  `scripts/check-vectors.py`, and `docs/reference/vector-scope.toml` /
  `docs/reference/conformance.md` carry no `AI` rows. Those three files were fenced to another
  story during this implementation wave — the same wall `ME-1` and `AF-2` hit.
  [CF-8](https://github.com/codewandler/sipx-clstr/blob/main/docs/stories/CF-8-bring-every-spec-under-the-vector-gate.md)
  tracks the gap repo-wide and covers this spec by its goal, but its inventory table predates this
  spec and **needs an `AI` row** alongside `LS`, `MR`, `AT`, `FR` and `HF`. The rows use the
  three-part `XX-Y-n` shape already registered for `PB`/`EP`/`RA`, so they need registration only,
  not renumbering; `RT-2` is the story their deferrals should name, since §4's types are a spec
  contract and no Rust exists yet.
- **Not added to `website/sidebars.js`** — outside this story's write set. `registrar-auth.md` is
  already a normative spec absent from that file, so the gate is green either way; the coordinator
  should decide whether this spec joins the published Specifications list.

## Notes
- One deployment synthesises PAI on egress, adds `Privacy: id` for anonymous callers, and falls back to a fixed number when no identity is available.
- Filed from a downstream deployment of this platform, whose capability inventory records this as `upstream` (its ledger entry **U-7**). The evidence sits in that deployment's own reference material.
