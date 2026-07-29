# Spec: Number normalisation

**Status:** normative · **Crate:** _future — lands with the trunk model (RT-2)_ ·
**Stories:** RT-6 · **Design:** [routing-trunks](../designs/routing-trunks.md)

## 1. Normative references

- RFC 3261 §19.1.1 (SIP URI components — the `userinfo`/`user` token that holds the number),
  §19.1.6 (`user=phone`), §19.1.2 (URIs in header fields).
- RFC 3261 §16.4 (route information preprocessing) and §16.5 (target determination) — the
  ingress binding sits between them (§8). §16.6 step 1 fixes what a proxy does to the copy it
  forwards: fields the section does not describe are not removed, and the body is never touched.
  Rewriting a user part is a modification §16.6 does not describe, which is why §8 fences it to
  exactly two points and to the number position alone.
- RFC 3261 §12.1.1 (a dialog is identified by Call-ID plus the two tags — not by the `To`/`From`
  URIs) and §12.2.1.1 (a mid-dialog Request-URI is the remote target). Together these are why
  normalisation may rewrite a `To` URI on a dialog-forming request and MUST NOT touch a request
  inside a dialog (N23).
- RFC 3261 §9.1 (a CANCEL's Request-URI, `To`, `From` and Call-ID MUST be identical to the
  request it cancels) and §17.1.1.3 (an ACK for a non-2xx repeats the INVITE's Request-URI,
  `To`, `From` and Call-ID) — N24: neither method is ever normalised.
- RFC 3261 §21.4.5 (`404 Not Found`) — the single rejection status of N20.
- **RFC 3966** §3 (the `tel` URI ABNF: `telephone-subscriber`, global numbers carrying a leading
  `+`, `visual-separator`, and the requirement that a local number carry `phone-context`) and §4
  (comparison, consumed via the kernel's `Uri::equivalent`).
- **RFC 3325** §4 (`P-Asserted-Identity`: a `sip`, `sips` or `tel` URI; one or two values, and
  when there are two, one of each scheme) — N4.
- **ITU-T Recommendation E.164** (11/2010) §6.2.1 — an international public telecommunication
  number carries at most 15 digits and its country code does not begin with `0`. This is the
  whole content of the `e164: global` guard condition (N15).
- **RFC 8174** — MUST/SHOULD/MAY in this document carry RFC 2119 meanings.
- sipx kernel contracts consumed: the URI parser and its escape handling (`Uri::user`,
  `Uri::decoded_user`), `Uri::equivalent`, and the lossless message model — every byte this spec
  does not name is re-serialised verbatim.
- This repo's specs consumed: [proxy-behavior](proxy-behavior.md) §5 (route preprocessing), §7
  F2/F5 (the forwarded copy) and §10 S4 (retransmission determinism);
  [location-service](location-service.md) §3 (the AoR canonical form, which this spec never
  touches — N25); [hook-framework](hook-framework.md) §3 (the phases this runs between, H7/H9).

**Out of scope:** which egress a call takes and the prefix/scope tables that decide it (RT-9);
trunk state, breakers and CPS (RT-2); `P-Asserted-Identity` *policy* — whether an identity is
asserted at all, what it is, and privacy handling (RT-7, which owns the field's content while
this spec owns only the shape of the number inside it); the egress header allowlist (RT-5);
number *portability* or carrier lookups of any kind (an external routing decision, EX-6); the
configuration file's syntax and reload semantics (DP-1, KO-8).

**Upstream considerations** (AGENTS.md rule 6): considered for upstream — **no for the policy,
yes for two syntax primitives.** Which fields a deployment rewrites, with which transforms, in
which order, per trunk, is platform orchestration and stays here. The primitives the
implementation needs are protocol-generic and belong to the kernel, exactly as the `Headers`
surgery API did (upstream ledger, `S-15`/PX-3):

1. **User-part surgery on `Uri`.** `Uri` exposes `user()`/`decoded_user()` and `push_param()`
   but no way to replace the user part; today N26's "every other byte identical" could only be
   met by re-serialising a URI this repo re-assembled itself, which is shadow parsing.
2. **Structured `tel:` access.** The kernel models `Scheme::Tel` but keeps the body opaque
   (`Parts::Opaque`); `tel_equivalent` splits `telephone-subscriber` from the parameter tail
   *privately*. N5 and N7 need that split in public API. Writing a second RFC 3966 splitter here
   is precisely what rule 6 forbids.

Both are candidate ledger rows for `CX-1` to file against sipx; the implementing story (RT-2's
trunk model) is blocked on them for `tel:` fields and for lossless rewriting, not on this spec.

## 2. What this is, and the one property that matters

Normalisation is a **total, pure function from numbers to numbers**, declared as data:

```rust
fn normalise(profile: &Profile, subject: &Subject) -> Outcome
```

No clock, no store, no socket, no message. Message surgery — pulling the numbers out (§4) and
putting them back (§8 N26) — is the caller's; the decision core sees four strings and a profile, so
the whole of §5–§7 is a fixture in the deterministic harness and every row of §11's transform
tables is a string-in/string-out assertion (AGENTS.md rule 2).

The property that makes this worth specifying at all: **the rule vocabulary is closed and
finite.** Four fields, four transforms, one guard with two condition forms. There is no regex,
no capture group, no substring-by-offset, no arithmetic, no conditional other than the guard, no
loop, no reference from one rule to another, and no rule that reads anything outside its own
field. §12 states the bound and what a deployment does when it needs more. A rule language that
grows past this is the routing DSL the vision rules out as a non-goal, and reaching that state
would fail this spec's purpose even if every rule below still passed.

## 3. Types

```rust
/// The closed field set (§4). Nothing else in a message is ever normalised.
enum Field { RequestUri, To, From, PAssertedIdentity }

/// What §4's extraction found at a field's number position.
enum Extracted {
    Absent,               // the field is not present in this request
    NotANumber,           // present, but not a number candidate (N6)
    Number(DigitForm),    // present and a candidate; visual separators already gone (N5)
}

/// `["+"] 1*DIGIT` — the only value shape this spec produces (N8).
struct DigitForm(String);

struct Subject {
    request_uri: Extracted,
    to:          Extracted,
    from:        Extracted,
    pai:         Vec<Extracted>,   // 0..2 values, RFC 3325 §4; each handled independently (N4)
}

enum Outcome {
    Rewrite { subject: Subject, trace: Vec<Applied> },   // per-field results
    Reject  { trace: Vec<Applied> },                     // 404 (N20)
}

/// Which declared rule fired where, or why a field was left alone. A rewrite nobody can explain
/// at 03:00 is a rewrite nobody will keep.
enum Applied {
    Rule { field: Field, index: usize, rule: RuleKind, before: DigitForm, after: DigitForm },
    Skipped { field: Field, reason: SkipReason },   // NotANumber (N6) | TelLocalWithoutContext (N7)
}

struct Profile {
    fields: Vec<FieldRules>,     // at most one entry per Field (N22)
}

struct FieldRules {
    field:      Field,
    transforms: Vec<Transform>,  // §5; declared order is the applied order (N13)
    guard:      Option<Guard>,   // §6; at most one
}

enum Transform {
    ReplacePrefix(BTreeMap<Literal, Literal>),   // T1
    StripLeadingZeros { max: u8 },               // T2
    AddPrefix(Literal),                          // T3
    EnsurePrefix(Literal),                       // T4
}

struct Guard { condition: Condition, fallback: Fallback }
enum Condition { Digits { min: u8, max: u8 }, E164Global }
enum Fallback  { Field(Field), Reject }
```

`Literal` is 1–8 bytes drawn from `+` and DIGIT, with `+` permitted only in first position
(N22). `Profile` is the entire language: a value with no free-form string in it anywhere.

## 4. Fields and extraction

| # | Rule |
|---|---|
| N1 | The field set is exactly `RequestUri`, `To`, `From`, `PAssertedIdentity`. No other header, and no other part of any of these four, is reachable from a profile. |
| N2 | The **number position** of a field is: for a `sip:`/`sips:` URI, the `user` token (RFC 3261 §19.1.1) with its percent escapes decoded; for a `tel:` URI, the `telephone-subscriber` (RFC 3966 §3), i.e. the body up to the first `;`. |
| N3 | Every byte of the field outside the number position is out of reach: scheme, host, port, URI parameters, URI headers, display name, and header parameters including `tag` (N26). |
| N4 | `P-Asserted-Identity` may carry two values (RFC 3325 §4). Each value is extracted, transformed and guarded **independently**; the count and the order of the values are preserved. A guard naming `PAssertedIdentity` as a *fallback* source uses the first value; if there is none, N18 applies. |
| N5 | Extraction deletes RFC 3966 §3 visual separators (`-`, `.`, `(`, `)`) from the number position. They carry no meaning; the digit form is what every rule below operates on. |
| N6 | A number position is a **number candidate** iff, after N5, it matches `["+"] 1*DIGIT`. Anything else — `alice`, `+`, an empty user part — extracts as `NotANumber`, is never transformed, and is re-emitted byte for byte. Rationale: `ensure_prefix` on `sip:alice@example.com` would produce `+alice`, an AoR nobody registered; a normaliser that mangles non-numbers is worse than one that does nothing. |
| N7 | A `tel:` field whose result would be a **local number** (no leading `+`) while the URI carries no `phone-context` parameter is left byte-identical and traced as `TelLocalWithoutContext`; RFC 3966 §3 requires the parameter, and this spec never emits a URI it knows to be invalid. A guard on that field then sees the extracted value, unchanged. |
| N8 | The output alphabet is `+` and DIGIT. Both are legal unescaped in a SIP `user` token (RFC 3261 §19.1.1: `+` is `user-unreserved`) and in `global-number-digits` (RFC 3966 §3), so normalisation never introduces a percent escape and never preserves one. |
| N9 | A field with no `FieldRules` entry in the bound profile is untouched, including N5: separators only disappear from a field the profile actually names. |

## 5. Transforms — the closed set

Four kinds. Each operates on one field's digit form, reads nothing else, and applies **at most
once**; a kind may appear at most once in a field's list (N22).

| # | Transform | Semantics |
|---|---|---|
| T1 | `replace_prefix: { <literal>: <literal>, … }` | Longest matching key wins; that one occurrence is replaced by its value; a value may be empty (a strip). No key may be empty (N22) — a "matches everything" arm is written as `add_prefix`, on its own line, where a reviewer sees it. At most 32 entries. |
| T2 | `strip_leading_zeros: { max: <1..8> }` | Deletes up to `max` leading `0`s, stopping early rather than emptying the number: at least one digit always remains. The only bounded repetition in the language, and its bound is declared. |
| T3 | `add_prefix: <literal>` | Prepends the literal unconditionally. Deliberately **not** idempotent — which is why N24 admits exactly one application point per direction, and why applying a profile twice is observable rather than silent. |
| T4 | `ensure_prefix: <literal>` | Prepends the literal iff the digit form does not already start with it. Idempotent. `ensure_prefix: "+"` is the E.164 forcing rule of §9. |

| # | Rule |
|---|---|
| N10 | A transform whose input is `Absent` or `NotANumber` is a no-op that produces the same `Extracted` value. |
| N11 | A transform never changes a field other than its own, never reads another field, and never reads the request. |
| N12 | Every transform is total: for every digit form there is exactly one output, and it is again a digit form (`["+"] 1*DIGIT`) or the input unchanged. |
| N13 | Within a field, transforms apply in **declared order**. Order is observable — `[strip_leading_zeros, replace_prefix]` and its reverse differ on `0049…` (NN-T-7) — so the list is the order, and there is no second ordering knob anywhere. |

## 6. The guard, and the fallback field

A field declares at most one guard. The guard is the only conditional in the language.

```yaml
guard: { digits: { min: 3, max: 20 }, fallback: to }
```

| # | Rule |
|---|---|
| N14 | `digits: { min, max }` holds iff the DIGIT count of the guarded field's provisional value (§7 phase 1), excluding a leading `+`, is within `[min, max]` inclusive. `0 ≤ min ≤ max ≤ 32`. |
| N15 | `e164: global` holds iff the provisional value is `+` followed by 1–15 DIGIT whose first digit is `1`–`9` (ITU-T E.164 §6.2.1). |
| N16 | A guard that holds changes nothing. A guard that fails substitutes the **provisional value of the fallback field**, or rejects (N17, N18). |
| N17 | Substitution replaces the guarded field's **number only**. The URI keeps its own scheme, host, port and parameters — a fallback that carried the `To` header's host into the Request-URI would send the request somewhere the route plan never chose (NN-G-2). |
| N18 | The outcome is `Reject` (N20) when the fallback is `reject`; when the fallback field is `Absent` or `NotANumber`; or when the fallback field's provisional value fails the **same** condition. |
| N19 | Guards do not chain and are not ordered. A guard reads the fallback field's phase-1 value, never that field's own guard result, so evaluating all guards from one snapshot gives the same answer in any order (NN-G-6). Two guards on one field, or a guard whose fallback names the field it guards, are configuration errors (N22). |
| N20 | The rejection status is `404 Not Found` (RFC 3261 §21.4.5) and is not configurable. A number the profile cannot make acceptable is a number this platform has no target for; statuses that describe *routing* outcomes belong to RT-2/RT-9, not to a normaliser. |

## 7. Evaluation — three phases, one pass

Given a bound profile and a request:

```text
phase 0  extract    per field: Absent | NotANumber | Number(digit form)      (§4)
phase 1  transform  per field: fold its declared transforms over its phase-0 value, in order
phase 2  guard      per field: evaluate its guard against phase-1 values only (§6)
phase 3  apply      write each changed number back into its field           (§8 N26)
```

| # | Rule |
|---|---|
| N21 | **Termination and determinism.** Every phase reads the snapshot the previous phase produced; nothing reads its own output. A profile therefore terminates in at most `4 fields × (4 transforms + 1 guard)` steps, and the result is a function of `(profile, subject)` alone — independent of field order, of guard declaration order, and of anything outside the request. |
| N22 | **Configuration errors, all detected at load.** Unknown field/transform/guard/condition name · a transform kind twice in one field's list · more than one guard on a field · a guard whose fallback names its own field · an empty `replace_prefix` key · more than 32 `replace_prefix` entries · a literal outside `+`/DIGIT, longer than 8 bytes, or with `+` anywhere but first · `min > max`, `max > 32`, `strip_leading_zeros.max` outside `1..8` · a profile with no fields · a binding naming a profile that does not exist. Each fails the configuration load, naming the profile, the field and the rule. An invalid profile fails a deployment, never a call. |

## 8. Binding — where a profile attaches, and where it runs

A profile is named and referenced; it is never written inline at a binding site, so the same
words cannot say two things in two places.

| # | Rule |
|---|---|
| N23 | Two binding directions exist and no others. **Ingress:** one profile per ingress scope (the scope vocabulary is RT-8/RT-9's), applied to the request as received, after route preprocessing ([proxy-behavior](proxy-behavior.md) §5 / RFC 3261 §16.4) and before target determination (§16.5) — the hook framework's `BeforeTargetResolution` boundary (H7). The ingress result is therefore the routing key. **Egress:** one profile per trunk (RT-2), applied per branch to the forwarded copy after [proxy-behavior](proxy-behavior.md) §7 F2 (Request-URI ← target) and before F5 (`BeforeForward`, H9), so a fork sends each branch its own trunk's numbers while the upstream copy stays untouched. |
| N24 | **At most one profile per direction per request, and profiles never chain.** Two applications of one profile are observable (T3 is not idempotent), so the pipeline offers exactly two application points and no way to compose a third. |
| N25 | **Never applied** to: a request carrying a `To` tag (in-dialog — RFC 3261 §12.2.1.1 makes the Request-URI the remote target); CANCEL (RFC 3261 §9.1 requires a byte-identical Request-URI, `To`, `From` and Call-ID); an ACK for a non-2xx (RFC 3261 §17.1.1.3); and REGISTER, whose address-of-record key is [location-service](location-service.md) §3's canonical form and no business of a trunk profile. Rewriting a `To`/`From` **URI** on a dialog-forming request is admissible because a dialog is identified by Call-ID and the two tags (RFC 3261 §12.1.1), and N3 leaves every tag byte-identical. |
| N26 | **Lossless everywhere else.** In the emitted message, every byte outside the number positions the profile changed is identical to the byte it replaced, including unknown headers, bodies, parameter order and spelling. Normalisation changes numbers; it never becomes a message rewriter, and it never touches a body — the line between this platform and a B2BUA ([services-b2bua](../designs/services-b2bua.md)) is not negotiable. |
| N27 | **No default.** With no profile bound, no message is altered; there is no implicit `+`, no implicit zero-stripping, and no built-in profile anywhere in the platform. |

## 9. The E.164 egress policy

"Force `+` on egress" is a declared rule in the vocabulary above, not a code path and not a
boolean somewhere else:

```yaml
normalisation:
  carrier-e164:
    fields:
      - name: request_uri
        transforms:
          - replace_prefix: { "00": "", "0": "49" }   # international / national trunk prefixes
          - ensure_prefix: "+"
        guard: { e164: global, fallback: reject }
      - name: from
        transforms: [ { ensure_prefix: "+" } ]
        guard: { e164: global, fallback: reject }
```

| # | Rule |
|---|---|
| N28 | A leading `+` appears on egress **only** because some `ensure_prefix`, `add_prefix` or `replace_prefix` in a bound profile put it there. No other component of this platform adds, removes or assumes one. |
| N29 | `e164: global` is validity, not repair: it accepts or it triggers the fallback (N16–N18). Nothing in the language guesses a country code, and `replace_prefix` is the only way one is ever supplied — from a literal table a reviewer can read. |

The example is illustrative, not a shipped default (N27). A country code is a property of a
deployment, and this spec has no opinion about which one.

## 10. Configuration surface

The data model is §3; DP-1 owns the file syntax and KO-8 the reload semantics. Rendered in the
platform's configuration language:

```yaml
normalisation:
  trim:                                   # an ingress profile
    fields:
      - name: request_uri
        transforms:
          - replace_prefix: { "+": "" }
          - strip_leading_zeros: { max: 2 }
        guard: { digits: { min: 3, max: 20 }, fallback: to }
      - name: to
        transforms:
          - replace_prefix: { "+": "" }
          - strip_leading_zeros: { max: 2 }
      - name: from
        transforms:
          - replace_prefix: { "+": "" }
          - strip_leading_zeros: { max: 2 }
      - name: p_asserted_identity
        transforms:
          - replace_prefix: { "+": "" }
          - strip_leading_zeros: { max: 2 }

ingress:
  - scope: { … }                          # RT-8/RT-9
    normalisation: trim
trunks:
  - name: carrier-a
    normalisation: carrier-e164           # §9
```

That is the whole surface: a map of named profiles, plus one name at each binding site.

## 11. Test vectors

Two grains, told apart by how the row is written. A row whose values are bare digit forms is
**`Subject`-level**: string in, string out, executed against `normalise` directly with no message
in sight — that is most of NN-T, NN-G and NN-E. A row whose values are URIs or header lines is
**message-level**: bytes in, bytes out, executed through the extraction and application phases in
the harness — all of NN-X, NN-C and NN-B, plus the rows below that turn on a host or on a
non-number. Every row is normative.

Registration of these rows in the vector registry (`scripts/check-vectors.py`,
`docs/reference/vector-scope.toml`) is deferred to `CF-8`, which tracks the same gap for every
spec written after `PX-1`.

**Extraction and URI forms (NN-X)** — profile `strip` is `[{ replace_prefix: { "+": "" } }]` on
the named field unless stated otherwise.

| # | Given | Expect |
|---|---|---|
| NN-X-1 | R-URI `sip:+493012345@carrier.example;user=phone`, profile `strip` | `sip:493012345@carrier.example;user=phone` — `;user=phone` byte-identical (N3) |
| NN-X-2 | From `"A" <sip:alice@example.com>;tag=1928301774`, profile `strip` on `from` | Byte-identical; traced `NotANumber` (N6) |
| NN-X-3 | To `<tel:+1-201-555-0123>`, profile `[{ ensure_prefix: "+" }]` on `to` | `<tel:+12015550123>` — separators do not survive a field the profile names (N5) |
| NN-X-4 | To `<tel:+1-201-555-0123>`, **no** rules for `to` | Byte-identical, separators included (N9) |
| NN-X-5 | PAI `<tel:7042;phone-context=example.com>`, profile `[{ add_prefix: "1" }]` | `<tel:17042;phone-context=example.com>` — a local number stays local, its context preserved |
| NN-X-6 | PAI `<tel:+4930123>`, profile `strip` | Byte-identical, traced `TelLocalWithoutContext` — stripping `+` would leave a local number with no `phone-context` (N7) |
| NN-X-7 | R-URI `sip:%2B4930123@edge.example`, profile `strip` | `sip:4930123@edge.example` — escapes decode on the way in and are never re-introduced (N8) |
| NN-X-8 | To `"Anna Nummer" <sip:+49301@edge.example>;tag=xyz`, profile `strip` | `"Anna Nummer" <sip:49301@edge.example>;tag=xyz` — display name and tag untouched (N3, N25) |
| NN-X-9 | PAI `"A" <sip:+4930999@edge.example>, <tel:+4930999>`, profile `strip` on `p_asserted_identity` | Both values normalised independently; two values, same order (N4). The `tel` value hits N7 and stays byte-identical |
| NN-X-10 | R-URI `sip:+@edge.example`, profile `strip` | Byte-identical — `+` alone is not `["+"] 1*DIGIT` (N6) |

**Transforms (NN-T).**

| # | Given | Expect |
|---|---|---|
| NN-T-1 | `replace_prefix: { "00": "", "0": "49" }` on `0049301` / `0301` / `49301` | `49301` / `49301` / `49301` — longest key wins; no key matches the third (T1) |
| NN-T-2 | `replace_prefix: { "0": "" }` on `000123` | `00123` — one occurrence, never a loop (T1) |
| NN-T-3 | `strip_leading_zeros: { max: 2 }` on `000123` / `0123` | `0123` / `123` (T2) |
| NN-T-4 | `strip_leading_zeros: { max: 8 }` on `000` | `0` — one digit always remains (T2) |
| NN-T-5 | `add_prefix: "0"` on `4930` | `04930`; applied twice would give `004930`, which is why N24 exists (T3) |
| NN-T-6 | `ensure_prefix: "+"` on `4930` / `+4930` | `+4930` / `+4930` — idempotent (T4) |
| NN-T-7 | `[strip_leading_zeros{max:2}, replace_prefix{"49":"+49"}]` vs the reverse order, on `0049301` | `+49301` vs `49301` — reversed, nothing starts with `49` when the prefix map runs, so it matches nothing and the `+` is never added: declared order is the applied order (N13) |
| NN-T-8 | Any transform on an `Absent` or `NotANumber` field | Unchanged, no trace entry beyond the extraction verdict (N10) |

**Guards and fallback (NN-G)** — profile: `request_uri` guarded `digits {min:3,max:20}`,
`fallback: to`; both fields transformed with `[{ replace_prefix: { "+": "" } }]`.

| # | Given | Expect |
|---|---|---|
| NN-G-1 | R-URI `+49301234`, To `+4930999` | R-URI `49301234` — guard holds, nothing substituted (N16) |
| NN-G-2 | R-URI `sip:12@edge.example`, To `<sip:+4930999@other.example>` | R-URI `sip:4930999@edge.example` — the **number** comes from `To`, the host does not (N17) |
| NN-G-3 | R-URI `12`, To `99` | `Reject` → `404` — the fallback fails the same condition (N18, N20) |
| NN-G-4 | R-URI `12`, To `<sip:alice@example.com>` | `Reject` → `404` — fallback is `NotANumber` (N18) |
| NN-G-5 | R-URI `12`, To `+4930999` | R-URI `4930999` — the fallback is `To`'s **phase-1** value, `+` already stripped (N19) |
| NN-G-6 | As NN-G-5, plus a guard on `to` that fails with `fallback: reject` | R-URI still `4930999`: guards read phase-1 values, so `To`'s guard does not feed the Request-URI's (N19) |
| NN-G-7 | R-URI with 21 digits, `max: 20`, To `4930999` | R-URI `4930999` (N14) |
| NN-G-8 | R-URI `12`, guard `fallback: reject` | `Reject` → `404` (N18) |

**E.164 egress (NN-E)** — `[{ ensure_prefix: "+" }]` with `guard: { e164: global, fallback: reject }`.

| # | Given | Expect |
|---|---|---|
| NN-E-1 | `4930123456` | `+4930123456`, accepted (N15, N28) |
| NN-E-2 | `+4930123456` | `+4930123456`, byte-identical — idempotent (T4) |
| NN-E-3 | `049301` → `+049301` | `Reject` → `404`: a country code does not begin with `0` (N15) |
| NN-E-4 | 16 digits | `Reject` → `404`: E.164 §6.2.1 caps at 15 (N15) |
| NN-E-5 | `sip:alice@example.com` | `Reject` → `404`: `NotANumber` with `fallback: reject` (N6, N18) |

**Composition (NN-C)** — ingress profile `trim` and egress profile `carrier-e164` exactly as
written in §10 and §9; the request is a dialog-forming INVITE.

| # | Given | Expect |
|---|---|---|
| NN-C-1 | R-URI `sip:+0049301234567@edge.example`, To `<sip:+4930999@edge.example>`, From `"A" <sip:+00491701234@edge.example>;tag=a1`, PAI `<sip:+4930999@edge.example>`; ingress `trim` | R-URI `sip:49301234567@edge.example` (11 digits, guard holds), To `<sip:4930999@edge.example>`, From `"A" <sip:491701234@edge.example>;tag=a1`, PAI `<sip:4930999@edge.example>`. Four fields, two transforms each, one guard — one pass (N21) |
| NN-C-2 | As NN-C-1 but R-URI `sip:12@edge.example` | R-URI `sip:4930999@edge.example`, taken from `To`'s phase-1 value; the other three fields as in NN-C-1 (N17, N19) |
| NN-C-3 | NN-C-1's request, ingress `trim`, then egress `carrier-e164` toward trunk `carrier-a` at `carrier.example` | Forwarded R-URI `sip:+49301234567@carrier.example` — ingress stripped, F2 set the target, egress forced `+`; the two profiles compose without either knowing the other exists (N23) |
| NN-C-4 | NN-C-1's request, no profile bound in either direction | Every byte of the forwarded request identical to the received one, modulo the F1–F11 steps [proxy-behavior](proxy-behavior.md) §7 already specifies (N27) |
| NN-C-5 | Profile naming only `request_uri` and `to` | `From` and `P-Asserted-Identity` byte-identical, separators included (N9) |
| NN-C-6 | Ingress `trim` where the Request-URI is `sip:+0049301234567@edge.example` and the ingress result feeds the location lookup | The lookup key is the **normalised** number — normalisation runs before target determination (N23). A deployment that normalises ingress and registers unnormalised AoRs looks up a number nobody registered; the binding is per scope so that this is a decision, not a surprise |

**Binding and the pipeline (NN-B).**

| # | Given | Expect |
|---|---|---|
| NN-B-1 | Mid-dialog re-INVITE (`To` tag present), both profiles bound | Byte-identical numbers in every field (N25) |
| NN-B-2 | CANCEL for a normalised INVITE | CANCEL untouched; the CANCEL the proxy sends downstream carries the forwarded INVITE's Request-URI, so it still matches (RFC 3261 §9.1, N25) |
| NN-B-3 | ACK for a non-2xx | Untouched (RFC 3261 §17.1.1.3, N25) |
| NN-B-4 | REGISTER arriving on a scope with an ingress profile | Untouched; the AoR key stays [location-service](location-service.md) §3's (N25) |
| NN-B-5 | The INVITE of NN-C-1 retransmitted | Byte-identical forwarded message — `normalise` is a pure function of `(profile, subject)`, which is what lets stateless mode keep its S4 guarantee (N21, proxy-behavior §10 S4) |
| NN-B-6 | Fork to two trunks with different egress profiles | Each branch carries its own trunk's numbers; the upstream copy is unaffected (N23) |
| NN-B-7 | Ingress `[{ add_prefix: "0" }]` and egress `[{ add_prefix: "0" }]`, one INVITE | Exactly one `0` from each — two application points, no third, and no accidental re-run (N24) |
| NN-B-8 | A profile referenced by a binding but not declared | Configuration load fails, naming the binding and the profile; no node starts on it (N22) |

## 12. What is deliberately not expressible

The vocabulary is closed at: **4 fields × (4 transforms + 1 guard)**, with literals drawn from
`+` and DIGIT. Everything below was considered and left out on purpose. Each has a home, and the
home is never "another keyword here".

| Not expressible | Why, and where it belongs |
|---|---|
| Regular expressions, capture groups, substring by offset | The regexes in route blocks are the problem this story exists to remove: they are unreviewable, untestable in isolation, and each one is a new dialect. A prefix table and four transforms cover the same ground as data. |
| Conditionals on anything but the guard — on another header, on the source, on the time of day | Selection is routing: scope × trigger, RT-9. A normaliser that branches on the caller is a router with a misleading name. |
| Number *ranges* and lookup tables that pick a destination | RT-9 (route selection), RT-2 (trunks). `replace_prefix` maps a number to a number; it never maps a number to an egress. |
| Chained or nested profiles, per-rule enable flags, ordering keys across fields | N21's determinism argument is the whole value of the model. Every one of these turns a total function into a program with a control-flow graph. |
| Anything requiring state, a lookup, or an external service — portability dips, LNP, per-caller history | An external routing hook (EX-6), which is asynchronous by construction and specified as such. Nothing in this spec ever waits. |
| A transform that is not one of T1–T4 | A typed module under the [hook framework](hook-framework.md) (EX-1), with a manifest declaring what it touches and startup validation over the set. That is the platform's answer to "we need one more transformation", and it is deliberately more expensive than editing a YAML file. |

The escape hatch is a compiled module with a declared manifest, never a longer config grammar.
If a deployment's requirement cannot be written in the table above, the answer is EX-1 or EX-6 —
and if the answer keeps being "add a keyword", the vocabulary has started becoming the routing
DSL the vision names as a non-goal, and this spec has failed on its own terms.
