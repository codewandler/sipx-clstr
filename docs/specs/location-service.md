# Spec: Location service

**Status:** normative · **Crate:** _future — created by CX-2_ · **Stories:** RG-1 … RG-6 ·
**Design:** [registrar-location](../designs/registrar-location.md)

## 1. Normative references

- RFC 3261 §10 (registrations): §10.2 constructing/refreshing/removing bindings and
  §10.2.1.1 (expiration selection), §10.3 (registrar processing — the intro's in-order and
  atomic-processing requirements, and steps 1–8).
- RFC 3261 §19.1.1 (SIP URI components), §19.1.4 (URI comparison) — consumed via the sipx
  kernel's `Uri::equivalent`. §19.1.4 equivalence is **not transitive** (the RFC's own
  `;security=on`/`;security=off` example), which is why it can compare contacts but can never
  key a hash or a store; see §3.1.
- RFC 2396 (URI syntax as referenced by RFC 3261): the `unreserved` character set
  (alphanumerics and `- _ . ! ~ * ' ( )`) used by the canonical escaping rule in §3.2.
- RFC 3986 §6.2.2.1 — case normalization conventions (lowercase scheme/host, uppercase
  percent-encoding hex digits) adopted for the canonical byte form.
- **RFC 3327** — the `Path` header: accumulation on the REGISTER path (§5.2), registrar
  procedures and use of the stored vector as the route set toward the contact (§5.3).
- RFC 3581 (`rport`) and RFC 3261 §18.2.1 — the observed source address recorded in the
  binding's `received` field.
- **RFC 5626** (outbound): `+sip.instance` / `reg-id` binding identity. M3 semantics; the
  schema fields exist now (§4).
- **RFC 8599** (push notifications): `pn-provider`, `pn-prid`, `pn-param` Contact URI
  parameters. M3 semantics; schema fields exist now (§4).
- RFC 5952 — canonical IPv6 text representation, used in §3.2.
- sipx kernel contracts consumed: the URI parser (validation, escape handling, duplicate
  URI-parameter rejection), `Uri::equivalent` (§19.1.4 including RFC 3966 tel rules), and the
  lossless message model (verbatim bytes for stored Contact and Path values).
- This repo's specs consumed / consuming: [proxy-behavior](proxy-behavior.md) §2
  (`TargetsResolved(Vec<Target>)`) and §7 F2/F7 consume the lookup contract of §7 here;
  [hook-framework](hook-framework.md) H5/H6 define the registrar-path extension phases this
  spec anchors in §5.7.

**Out of scope:** digest challenge, nonce and credential verification (RG-2; reaches this
spec only as the authenticated `principal` input), `flow_ref` internals (AF-2's spec — opaque
bytes here), the rendezvous hash function and shard handoff (RG-5 — only the key bytes are
fixed here, §8), proxy forwarding of the target set (proxy-behavior §16.5 consumption),
RFC 5626 flow management and `430 Flow Failed` (M3), RFC 8599 push wake-up flows (M3).

**Upstream considerations** (AGENTS.md rule 6):

- `Path`/`Service-Route` typed headers: kernel gap, already ledgered
  ([upstream ledger](../upstream.md)). This spec deliberately treats Path values as verbatim
  URI byte strings so the contract does not wait on that row; typed accessors arrive with it.
- Server-side digest primitives: ledgered, RG-2's scope.
- Considered for upstream: AoR canonicalization — **no**, cluster-specific: the canonical
  byte form is this platform's storage/shard key format (an injective encoding layered on the
  kernel's parser and escape folding), not protocol machinery; the kernel keeps §19.1.4
  comparison.
- Considered for upstream: the §10.3 REGISTER decision function — **no** for v1,
  cluster-specific: it is inseparable from tenancy, quota and durable-store policy; revisit
  only if sipx ever grows a server-side registrar role.

## 2. Roles and the sans-IO contract

The location service has three faces:

1. **Registrar command path** — REGISTER processing as a pure decision function plus a
   compare-and-swap commit (§5).
2. **Lookup path** — resolve a canonical AoR to a forking-ordered routable target set (§7).
3. **Change stream** — best-effort commit notifications for cache invalidation (§6).

Decision logic is sans-IO (AGENTS.md rule 2): time enters as the command's `now`, the current
binding set enters as data, and the store commits under a revision check. No clock, socket or
database handle appears in the decision function.

```rust
struct RegisterCommand {
    tenant: TenantId,            // from the edge's trust context, never from the URI
    aor: CanonicalAor,           // §3 canonical bytes
    call_id: Bytes,              // byte-exact, case-sensitive
    cseq: u32,
    contacts: ContactOps,        // Explicit(Vec<ContactOp>) | Wildcard
    path: Vec<Bytes>,            // verbatim Path URIs, topmost first (RFC 3327)
    received: SourceAddr,        // (transport, ip, port) observed by the edge
    flow_ref: Option<Bytes>,     // opaque; format is AF-2's spec
    principal: Principal,        // authenticated identity (RG-2), already authorized
    now: Instant,                // time is an input
}

enum Outcome {
    Commit { set: BindingSet, response: Response },  // requires a CAS commit
    Noop   { response: Response },                   // idempotent retry or rejection
}

fn process(cmd: &RegisterCommand, current: &BindingSet, policy: &TenantPolicy) -> Outcome;

trait LocationStore {
    fn read(&self, tenant: &TenantId, aor: &CanonicalAor) -> (BindingSet, Revision);
    fn commit(&self, tenant: &TenantId, aor: &CanonicalAor,
              expected: Revision, set: BindingSet) -> Result<Revision, CasConflict>;
    fn lookup(&self, tenant: &TenantId, aor: &CanonicalAor, now: Instant) -> TargetSet;
    fn changes(&self) -> ChangeStream;               // best-effort, §6
}
```

The driver loop is: `read` → `process` → on `Commit`, `commit(expected)` → on `CasConflict`,
re-read and re-process (bounded retries, §5.1 S10). The idempotency rule (§5.3) is what makes
this loop safe to repeat.

## 3. AoR canonicalization

### 3.1 Why a canonical byte form, and its relation to §19.1.4

RFC 3261 §19.1.4 equivalence is a pairwise *comparison* and is not transitive; the kernel's
`Uri::equivalent` implements it and deliberately refuses to be `PartialEq` for that reason. A
storage key and a rendezvous-hash input need the opposite: a total function `C(uri)` whose
byte output is compared by equality — transitive by construction. Two spellings of one AoR
must produce identical bytes or they land on two shards, which the design forbids.

RFC 3261 provides the anchor: §10.3 step 5 instructs the registrar to convert the AoR "to a
canonical form" — all URI parameters removed (including `user`), escaped characters
unescaped — and to use the result as the index into the bindings. The canonical byte form
below is a deterministic, injective, printable encoding of exactly that §10.3 step 5 canonical
URI, plus case and literal normalizations that §19.1.4 already declares meaningless.

The division of labor is:

- **AoR keying** (this section): the canonical byte form. Deliberately *coarser* than
  §19.1.4 in the ways §10.3 step 5 mandates (parameters gone, escapes folded).
- **Contact-binding identity inside one AoR** (§5.3): kernel `Uri::equivalent` per §10.3
  step 7, which searches by "URI comparison rules". Comparison is sufficient there because it
  is a linear scan, never a hash key.

### 3.2 The canonical byte form

`C(aor)` is defined only for `sip:` and `sips:` URIs (RFC 3261 §10.3: the address-of-record
MUST be a SIP or SIPS URI). The output grammar:

```
canonical-aor = scheme ":" [ canon-user "@" ] canon-host [ ":" port ]
scheme        = "sip" / "sips"                         ; lowercase; never folded into each other
canon-user    = *( unreserved / "%" HEXDIG HEXDIG )    ; HEXDIG uppercase; unreserved per RFC 2396
canon-host    = lowercase hostname, single trailing "." removed
              / IPv4 dotted-decimal
              / "[" RFC 5952 IPv6 text form, lowercase "]"
port          = decimal, no leading zeros              ; present iff the input had a port
```

Rules, each with its rationale:

| # | Rule | Rationale |
|---|---|---|
| N1 | Scheme lowercased; `sip` and `sips` remain distinct keys | §19.1.4: schemes compare case-insensitively, and "a SIP and SIPS URI are never equivalent" — a sips AoR is a different resource, not a spelling |
| N2 | User part: decode **all** percent-escapes, then re-encode: RFC 2396 `unreserved` bytes raw, every other byte `%HH` with uppercase hex | §10.3 step 5 mandates full unescaping for the AoR index; the deterministic re-encoding is an injective printable image of the decoded bytes, so the induced key relation equals §10.3's while the key stays parseable and delimiter-safe (a raw decoded `@` or NUL could otherwise forge key structure) |
| N3 | User case preserved | §19.1.4: userinfo comparison is case-sensitive |
| N4 | No tel-number folding inside sip user parts (`+1-555-0101` ≠ `+15550101`) | RFC 3966 §4.1 visual-separator rules apply to `tel:` URIs only; a sip user part is opaque bytes |
| N5 | Host: hostnames lowercased; one trailing `.` stripped | §19.1.4: host comparison is case-insensitive. The RFC 3261 §25 `hostname` grammar admits an optional trailing dot for the identical label sequence; two spellings of one FQDN must not shard differently. **[sipx-clstr]** — this is coarser than the kernel's byte comparison and applies to the key only |
| N6 | IP literals: IPv4 canonical dotted-decimal; IPv6 per RFC 5952, lowercase, bracketed. A hostname never canonicalizes to an IP or vice versa | §19.1.4: a hostname never matches an IP address, and a comparison that consulted DNS would be neither pure nor stable (kernel rule) |
| N7 | Port: kept iff present, as the decimal value (leading zeros dropped). Absent port and explicit `:5060` are **distinct** keys | §19.1.4: "a URI omitting any component with a default value will not match a URI explicitly containing that component with its default value"; resolution differs too (RFC 3263: no port → NAPTR/SRV) |
| N8 | All URI parameters removed — including `user`, `transport`, `maddr` | §10.3 step 5, verbatim ("all URI parameters MUST be removed (including the user-param)") |
| N9 | URI headers (`?…`): reject | An AoR is an index, not a message template; §19.1.1 already bars headers from a Request-URI, and §19.1.4 says headers are never ignored — silently stripping would conflate URIs the RFC calls different |
| N10 | Password in userinfo: reject | §19.1.1 recommends against it; a credential must not become key material that flows into logs and hashes. **[sipx-clstr]** |
| N11 | Empty user (`sip:@h`): reject; absent user (`sip:h`) is a valid, distinct key | The `user` ABNF is `1*(...)` — an empty user is not a spelling of "no user" |
| N12 | Malformed input (bad escapes, illegal characters, duplicate URI parameters) rejects — canonicalization is a partial function over kernel-parsed URIs | Kernel parse errors are protocol errors; guessing at a key from a broken URI can only mis-shard |
| N13 | `C(aor)` longer than 512 bytes: reject | **[sipx-clstr]** — keys bound storage rows, change-stream payloads (§6.2) and hash inputs; an unbounded key is a resource-exhaustion vector |

Callers map a rejection to their own failure: REGISTER processing answers `400` (§5.1 S5);
lookup returns the empty target set.

The canonical form never appears on the wire. It exists to key the store, the cache and the
rendezvous hash — which is why it may spell a phone user `%2B15550101` without anyone caring.

## 4. Binding schema

A binding is one row of an AoR's set. Contact and Path values are stored as verbatim bytes
(kernel lossless model): what the UA registered is what the proxy later forwards
(proxy-behavior §7 F2).

| Field | Form | Source | Notes |
|---|---|---|---|
| `tenant` | opaque UTF-8 id, no NUL byte | edge trust context | key component; the NUL-free constraint is load-bearing for §8 |
| `aor` | canonical bytes (§3) | `To` header | key component |
| `contact` | verbatim URI bytes | `Contact` | binding identity per §19.1.4 `equivalent()` (§5.3) |
| `q` | integer thousandths 0–1000 | `Contact ;q` | absent → 1000 (§7); malformed → 400 |
| `call_id` | bytes, case-sensitive | `Call-ID` | ordering/idempotency (§5.3) |
| `cseq` | u32 | `CSeq` | ordering/idempotency (§5.3) |
| `expires_at` | absolute instant | §5.2 | `now` + granted interval |
| `registered_at` / `refreshed_at` | absolute instants | command `now` | creation / last successful update; `refreshed_at` is the §7 tie-break |
| `path` | ordered verbatim URI vector | `Path` (RFC 3327) | stored topmost-first exactly as received (§5.6) |
| `received` | (transport, ip, port) | transport driver | observed source (RFC 3261 §18.2.1, RFC 3581); diagnostics and NAT evidence, not a routing input in M1 |
| `instance_id` | bytes, optional | `Contact +sip.instance` | RFC 5626; stored in M1, becomes binding identity with `reg_id` in M3 |
| `reg_id` | u32, optional | `Contact reg-id` | RFC 5626, M3 |
| `flow_ref` | opaque bytes, optional | accepting edge | format is AF-2's spec; never a socket |
| `push` | `{provider, prid, param}`, optional | `Contact pn-*` | RFC 8599 projections of the verbatim contact; stored in M1, used in M3 |
| `principal` | opaque authenticated id | RG-2 | who created/refreshed the binding; audit and authorization |
| `revision` | u64, per **AoR** | store | one revision fences the whole set (§6); bindings inherit it |

**[sipx-clstr]** M3 fields (`instance_id`, `reg_id`, `push`) are present in the schema from
the first migration so M3 activates semantics, not a schema change; until then they are
stored and ignored by processing.

## 5. REGISTER processing and the CAS contract

RFC 3261 §10.3's intro is the consistency requirement in prose: REGISTER requests for an AoR
are processed **in order** and **atomically** — "a particular REGISTER request is either
processed completely or not at all" — and independently of other AoRs. Hence:

**[sipx-clstr] Serialization domain** — commands serialize per `(tenant, canonical AoR)` and
never across domains. Per-AoR order is enforced by the CAS revision, not by any backend's
global lock, so the guarantee is backend-agnostic (§6).

### 5.1 Processing order

Steps run in order; the first failure responds and terminates with nothing committed.

| # | Step (RFC 3261 §10.3) | On failure |
|---|---|---|
| S1 | Request-URI domain is served by this registrar's tenant config (step 1) | `404 Not Found` — **[sipx-clstr]** the registrar role never silently proxies an unserved domain |
| S2 | `Require` option tags all supported (step 2) | `420 Bad Extension` + `Unsupported` listing the offenders |
| S3 | Authentication (step 3) — RG-2's scope; arrives here as the `principal` fact | challenge upstream of this spec |
| S4 | `principal` authorized for the AoR (step 4) — policy input | `403 Forbidden` |
| S5 | AoR extraction from `To`; canonicalization (§3); AoR valid for the Request-URI domain (step 5) | `400` (malformed / §3 rejection), `404` (AoR not in domain) |
| S6 | Wildcard validation (step 6, §5.4) | `400 Bad Request` |
| S7 | Per-contact expiry selection and min/max policy (step 7, §5.2) | `423 Interval Too Brief` + `Min-Expires` |
| S8 | Per-tenant quota (§5.5) **[sipx-clstr]** | `403 Forbidden` |
| S9 | Per-binding Call-ID/CSeq application (steps 6–7, §5.3) | `500` on stale-CSeq abort **[sipx-clstr]** |
| S10 | Atomic commit via CAS; on conflict re-read and re-process, bounded retries (default 3, configurable) | `503 Service Unavailable` when the store is unreachable or retries exhaust — **[sipx-clstr]**; this 503 is originated by the registrar UAS, not a branch response, so proxy-behavior R8 does not apply to it |
| S11 | `200 OK` with the complete active set (step 8, §5.6) | — |

### 5.2 Expiry selection

Per contact, in order (RFC 3261 §10.2.1.1, §10.3 step 7):

| # | Rule |
|---|---|
| E1 | `;expires` parameter on the Contact, if present |
| E2 | else the `Expires` header, if present |
| E3 | else the tenant's configured default — **[sipx-clstr]** default 3600 s |
| E4 | Requested value 0 = removal of that binding; never subject to E5/E6 |
| E5 | Requested value > tenant max (**[sipx-clstr]** default 86400 s): granted value lowered to the max, silently (§10.3 step 7 MAY shorten); the response's `expires` states what was granted |
| E6 | 0 < requested < tenant min (**[sipx-clstr]** default 60 s): the whole request fails `423` and the response MUST carry `Min-Expires` with the tenant minimum (§10.3 step 7). Atomicity (§5) means one too-brief contact fails the entire REGISTER — nothing partial commits |

### 5.3 Binding identity, Call-ID/CSeq ordering and idempotency

Within an AoR, an incoming contact is matched against stored bindings with the kernel's
`Uri::equivalent` (§10.3 step 7's "URI comparison rules"). Because §19.1.4 equivalence is
non-transitive, an incoming contact can be equivalent to more than one stored binding;
**[sipx-clstr]** the first match in creation order (`registered_at`, then stored contact byte
order) is updated — a deterministic choice the harness can assert. In M3, when a contact
carries both `+sip.instance` and `reg-id`, identity becomes that pair (RFC 5626 §6); noted
here so the schema and vectors don't move later.

Per matched binding (RFC 3261 §10.3 steps 6–7):

| # | Stored vs request | Action |
|---|---|---|
| B1 | No stored binding matches | add (or ignore, for a removal of an absent contact) |
| B2 | `Call-ID` differs | apply — update or remove (the UA restarted; step 7 says update/remove regardless of CSeq) |
| B3 | Same `Call-ID`, request `CSeq` > stored | apply |
| B4 | Same `Call-ID`, request `CSeq` = stored, requested state already holds (§5.3.1) | **idempotent retry**: no mutation, `200` with the current set, revision unchanged — **[sipx-clstr]** |
| B5 | Same `Call-ID`, request `CSeq` ≤ stored otherwise | abort the **entire** request (step 7 "the update MUST be aborted"); **[sipx-clstr]** the failure code is `500 Server Internal Error` — the RFC names no code; the request is not malformed (400 would mislead diagnostics) and a retry with a fresh CSeq succeeds |

**The idempotency rule, precisely:** a `RegisterCommand` is a retry of an applied command iff
for every binding it touches, the stored `(call_id, cseq)` equal the command's and the stored
state already equals the command's requested outcome (same granted expiry base, same contact
set effect). A retry returns success with the current set and commits nothing. This is what
lets the CAS driver loop (§2) and cluster-level retries re-present a command safely: RFC 3261
§10.3 makes `(Call-ID, CSeq)` the registrar's ordering token, and this spec makes replaying
the same token a no-op rather than a second write **to the bindings that token wrote**. It
does not make the token a lock on the address-of-record: a request that is not a retry by the
definition above — one that also carries a contact matching no stored binding — is still B1's
business for that contact (B4.3).

#### 5.3.1 "Same granted expiry base", and how far B4 reaches **[sipx-clstr]**

| # | Rule |
|---|---|
| B4.1 | The **granted duration** is the base. A stored binding already holds a command's requested expiry iff the lifetime it was granted — `refreshed_at` to `expires_at` — equals the lifetime this command grants that contact under §5.2. The absolute deadline is deliberately **not** compared |
| B4.2 | B4's remedy is *no mutation*, never an extension. A retry leaves `expires_at`, `refreshed_at`, `q`, the Path vector and the revision exactly as they are, and answers `200` with the set as it stands — remaining lifetimes computed against the retry's own `now` (§5.6), which is a statement about the response, not a write |
| B4.3 | B4's guarantee is about the **matched binding**, not about the request. B1–B5 are decided per contact against the binding it matches; a contact matching no stored binding is B1's, and B1 has no stored `(Call-ID, CSeq)` to compare against — §10.3 step 7's abort is written against a binding, and there is none. So one request that replays a spent token for a bound contact *and* carries a contact that is not bound refreshes nothing and commits the addition, at a bumped revision. B5 remains asymmetric with it: a matched binding that fails B5 aborts the **whole** request, additions included |

Why the duration and not the deadline: `now` is stamped when a request is *admitted*, so two
deliveries of one REGISTER never share one. A UDP retransmission after a lost `200`, a CAS
re-read (§2), and a re-presentation at a second node all arrive with a later `now` than the
delivery that wrote the binding. A rule that compared deadlines would therefore classify
every retry that actually happens as a second write under a spent token and refuse it `500`
(B5) — which makes B4 unreachable in practice and contradicts this section's own claim that
the rule is what lets a command be re-presented safely. Comparing durations makes the test
independent of when the copy arrived, and two nodes computing the same granted duration from
the same policy agree without exchanging anything.

The carve-out stays narrow, which is the point of comparing anything at all. RFC 3261 §10.3
step 7 aborts on a CSeq that is not higher; B4 is this platform's exception for
retransmissions, and it applies only when the stored state *is* the requested state. Same
token, matching a stored binding, with a different requested duration, a different
contact-set effect (a removal of a contact that is still bound), or a Path vector that is not
the stored one is not a retry — it is a second write to that binding, and B5 refuses it.

"Narrow" is about the **matched binding**, and B4.3 is where the boundary actually falls: a
spent token is not authority over the address-of-record, so a request that also carries an
unbound contact still adds it (LS-R-23). That is not a widening of anything. A UA that can
send that request can send one carrying only the new contact, which B1 accepts identically —
the ordering token never gated an addition, because there is nothing for it to be stale
against. What bounds additions is the authenticated principal (§5.1 S3/S4) and the per-tenant
quota (§5.5), and both apply here unchanged. Stated because a reader who took "replaying the
same token is a no-op" as an invariant of the *request* would be surprised, and the
CAS-and-cluster claim that rule supports (§2, K1) is about the bindings the token wrote.

Those three are the whole comparison, deliberately: the §4 projections a Contact can also
carry — `q`, `+sip.instance`, `reg-id`, `pn-*` — are **not** compared. Recorded rather than
left silent, because it is the one place B4 is broader than "the stored state equals the
requested outcome": a command that reuses a spent token but asks for a different `q` is
answered as a retry. It is still not a write — B4 mutates nothing, so no `q` is lost and no
token is spent twice — and the `200` enumerates the stored `q` (§5.6), so the UA is told what
holds rather than what it asked for. Narrowing the comparison to the full projection set is a
change to this table, and to the vectors under it, not an implementation detail.

**Rejected: comparing an originating `now` carried with the command.** The alternative was to
add the instant of the first delivery to `RegisterCommand` and have every re-presenter carry
it forward, which is a more literal reading of "base" and would survive a policy whose granted
lifetime changed between attempts. It is rejected because it cannot answer the case this rule
exists for: a UA's retransmission arrives as fresh bytes over the wire and carries no field of
ours, so the edge stamps its own `now` and the two deliveries differ again — the defect would
survive the fix. It also makes the ordering decision depend on a value one node accepts from
another, where the duration is recomputed from local policy and stored state. The policy-drift
case it would have covered is handled correctly by B4.1 without it: a changed granted lifetime
makes the durations differ, so the command is not a retry and B5 refuses it, which is the
conservative answer.

An expired binding is **absent** for every purpose — lookup, matching, and Call-ID/CSeq
comparison. A late REGISTER carrying an older CSeq after the binding expired therefore adds a
fresh binding (B1): once a binding is gone, the RFC's model has nothing to compare against.

#### 5.3.2 More than one contact in one REGISTER **[sipx-clstr]**

| # | Rule |
|---|---|
| B6 | A request's contact operations are applied **in the order the request states them**, and each is matched (§5.3) against the binding set **as the preceding operations left it**. A binding an earlier operation removed is absent for every later operation, exactly as an expired one is; a binding an earlier operation added or replaced is present for them, with the value it was given |
| B7 | B2–B5 compare this request's ordering token against the token of the request that **last wrote** the matched binding. A binding that an earlier operation of *this same request* added or replaced carries this request's own `(Call-ID, CSeq)`, so that comparison decides nothing — B6 has already fixed the order. Such a binding is matched and the operation **applies**: a later removal removes it, a later refresh replaces it with the value the later operation grants. It is never B4's idempotent retry and never B5's abort |
| B8 | B4 and B5 are decided against the **net** outcome this request asks of the matched binding — the effect of the *last* operation of this request that resolves to it — not against the operation being applied. §5.3's idempotency rule is stated per binding against "the command's requested outcome", and where several operations resolve to one binding, B6 makes the last one that outcome; the earlier ones are writes this same request overwrites before anything commits |
| B9 | A request whose reconciled set **is** the set it read commits nothing: the revision does not move and no change event is published (B4.2), even where individual operations mutated the set on the way there. `changed` is not the test; the durable set is |

Why this needs saying. §10.3's atomicity requirement is about what a *reader* observes — the whole
request commits or none of it does (K2) — and it is silent on what each operation is decided
against, because the RFC describes one contact at a time. A registrar may therefore be tempted to
resolve every operation against one view of the set captured before the first mutation, which is
cheaper and, for a single-contact REGISTER, indistinguishable. It is wrong from the second contact
onward: a removal shortens the set, so every later operation resolving through the captured view
names an entry that has moved, and the request commits a binding set it never described. B6 fixes
the resolution point so that the committed set is a function of the request, not of the order in
which reconciliation happened to invalidate its own view.

B6 costs neither of the guarantees around it. Atomicity is unaffected — the whole sequence is still
one CAS commit or nothing (§6 K1/K2), and no reader sees an intermediate set. Nor is the CAS loop:
a writer that loses the race re-reads the committed set and re-runs the **entire** sequence against
it (§2, §5.1 S10), so a losing writer never commits a set reconciled against state it read before
the winner wrote.

**B7 is the half of B6 that is easy to get wrong in the safe-looking direction.** Once operations
really are resolved against the set as it stands, a later operation can match a binding an earlier
operation of the same request just wrote — two §19.1.4-equivalent contacts in one REGISTER is all it
takes, and `CC;expires=3600, CC;line=7;expires=0` is an ordinary thing for a UA to send when it
believes the line-tagged spelling is a separate binding. That freshly written binding carries this
request's `Call-ID` and `CSeq`, because this request wrote it. A registrar that runs B2–B5 against it
unchanged reads "same token, stored state is not what is asked for" and aborts the **whole** request
`500` (B5) — so the UA is left unregistered, its retry with a fresh `CSeq` fails identically, and the
failure is permanent rather than transient. B6's own text already forbids that reading: the binding
"is present for them, with the value it was given" — present and applied to, not fatal.

The distinction B7 turns on is **who wrote the binding, not what token it carries**. A retransmission
of one REGISTER also arrives carrying the token stored on the binding, and that case must stay B4's:
the binding was written by an *earlier delivery*, the registrar must not write it a second time, and
§5.3.1 is the whole argument for why. B7 covers only a binding written during *this* reconciliation
pass — a fact the registrar knows directly, having just performed the write, and which it must not
try to infer by comparing tokens, because the two cases are token-identical. A CAS re-read (§2)
starts a fresh pass against the committed set, so nothing carries over: on the retry the binding the
winner committed was written by someone else, and B2–B5 decide it normally.

B7 does not widen what a request may do. Every operation it lets through is one the same UA could
have sent as a single-contact REGISTER against the state its predecessor left, and the bounds on
additions are unchanged — authentication (§5.1 S3/S4) and the per-tenant quota (§5.5), neither of
which B7 touches. What it removes is only the `500`: the last operation naming a contact wins, the
committed set is a function of the request, and the `200` enumerates the set that actually holds
(§5.6), so a UA whose two spellings turn out to be one contact is told so rather than refused.

**B8 is what makes B7's claim about retransmissions true.** B7 says a retransmission "must stay
B4's", and that is exactly the case B7 alone does not deliver, because B4 has to be asked the right
question first. `CC;expires=3600, CC;expires=7200` commits one binding at 7200 (LS-R-29). Deliver it
again: the stored binding carries this command's token, `written_here` is false because an earlier
*delivery* wrote it, and the first operation is compared — grant 3600 — against a stored 7200. Read
per operation, that is "same token, different request", and B5 aborts the whole thing `500`
(LS-R-31). Read per binding as §5.3 states it, the command asks for 7200, the store holds 7200, and
the request is a retry that commits nothing. The per-operation reading is not a narrower version of
the rule; it is a different rule that happens to agree whenever a binding is named once, which is
every case that existed before B6.

B8 does not soften B5. The comparison base moves from one grant to the net grant; what is compared is
unchanged, and a command that asks for something the store does not hold is still a second write
under a spent token and still aborts. `CC;expires=3600` alone against a stored 3600 is B4 exactly as
before; against a stored 7200 it is still B5, because 3600 is then also the net outcome. What B8
changes is only the case where this request itself supersedes the operation being decided — and in
that case the operation's own grant was never what the request asked for.

**B9 is B4.2 applied to the set rather than to a binding.** B4's remedy is no mutation, and the
reason is that a re-presented command must not spend its token twice. A request whose operations
cancel — an addition a later removal takes back (LS-R-28) — reaches the end of reconciliation having
mutated the set repeatedly and arrived back where it started. Committing that costs a revision and
publishes a change event describing no change, and it does so on *every* delivery: the two deliveries
of such a request are indistinguishable from the durable state, because the set is identical before
and after each and no binding survives to carry the ordering token. There is nothing a registrar
could compare to tell them apart, so the only way the retransmission is idempotent is for neither
delivery to write (LS-R-32). Reaping expired bindings is a real change and is not covered by this:
a set that lost an expired binding on the way in is not the set that was read.

### 5.4 Wildcard removal (`Contact: *`)

RFC 3261 §10.3 step 6:

| # | Rule |
|---|---|
| W1 | `*` with any other Contact header value present → `400` |
| W2 | `*` without an explicit `Expires: 0` header → `400` (a wildcard with a non-zero or absent Expires "MUST" be rejected as invalid) |
| W3 | Valid wildcard: for each stored binding — `Call-ID` differs → remove; same `Call-ID` and request `CSeq` > stored → remove; same `Call-ID`, `CSeq` not higher → abort the entire request (`500`, as B5) |
| W4 | The removals commit atomically with revision bump; the `200` enumerates the (now possibly empty) active set |

### 5.5 Per-tenant quotas **[sipx-clstr]**

- `max_bindings_per_aor` (default 10, per-tenant configurable): a REGISTER whose committed
  outcome would exceed it fails `403 Forbidden` — a policy refusal, not a server fault, and a
  retry cannot succeed without removing bindings. Refreshes, replacements and removals never
  grow the set and never trip the quota.
- The quota bounds the active set at write time, so every lookup's target set is bounded by
  it — the fork-breadth interaction proxy-behavior V5/§6 (`Max-Breadth`) relies on.

**"Committed outcome" is the whole rule, and it is decided on the reconciled set.** Nothing computed
before §5.3's operations are applied knows the outcome: deciding whether an operation adds a binding
or lands on one *is* the reconciliation, and under B6/B7 several operations can collapse onto one
binding while a later removal can take back an earlier addition. A conservative pre-check is therefore
not a cheaper spelling of this rule but a different and stricter one, and it **may not refuse a
request the reconciled set permits** — `403` is a policy refusal a UA cannot retry out of, so an
over-refusal is a registration a UA can never obtain. Two attempts at such a pre-check have been wrong
in that direction: one counted every positive-expiry contact as an addition and refused refreshes
(LS-R-15), and one counted a candidate unless it was equivalent to a candidate already counted, which
is an *upper* bound on additions and answered `403` where the outcome was within the quota
(LS-R-30). §5.1 lists S8 before S9 as an ordering of checks, not as a claim that the quota can be
decided before the operations it measures; where a request would both exceed the quota and abort under
S9, S9's failure is the one reported.

### 5.6 Path and the response

- **[sipx-clstr]** A REGISTER carrying `Path` from a UAC that did not advertise
  `Supported: path` is rejected with `421 Extension Required` naming `path`. RFC 3327 leaves
  accepting such registrations to local policy; this platform refuses, because committing the
  binding would make it reachable only through a route set the UA does not know exists.
- A valid Path vector is stored verbatim, topmost value first, exactly as received. RFC 3327
  §5.2 accumulates Path like Record-Route — each proxy prepends itself — so the topmost value
  is the proxy nearest the registrar and is the first hop of the route set toward the contact
  (§7).
- The `200` response (RFC 3261 §10.3 step 8, RFC 3327 §5.3): enumerates **all** current
  bindings as Contact values, each with an `expires` parameter stating the remaining granted
  lifetime (computed against the command's `now`) and its stored `q`; echoes the stored Path
  vector unmodified and in order; includes `path` in `Supported`.
- The complete-set requirement is unconditional: a REGISTER that removed one of three
  bindings answers with the two survivors, and a valid wildcard removal answers `200` with no
  Contact values.

### 5.7 Extension points

The registrar path exposes exactly the two hook phases the
[hook-framework](hook-framework.md) spec defines (its H5/H6 rows and alignment table); this
spec names them, per that spec's contract, and anchors them to §5.1:

| Phase (hook-framework) | Anchor here |
|---|---|
| `BeforeRegistrarUpdate` | After S6 — the `RegisterCommand` is constructed and validated, the principal fixed — and before S7–S10. Modules see the command (contact ops, requested expiries) and may reject (e.g. `423`, `403`) or adjust the registration; the adjusted command is what §5.2–§5.5 then process |
| `AfterRegistrarUpdate` | After S10 — the CAS applied, the final binding set known, the S11 response drafted — and before the response is sent. Modules may patch response headers; the binding set is read-only |

**[sipx-clstr]** Each phase fires once per REGISTER request: `BeforeRegistrarUpdate` on the
command before the first CAS attempt, not per retry — a §6 K1 conflict retry re-runs
`process` on the same (possibly adjusted) command, so module effects stay idempotent under
the driver loop. No other extension point exists on the registrar path; in particular,
nothing hooks between `read` and `commit`.

## 6. Consistency contract

### 6.1 Backend-agnostic rules

| # | Rule |
|---|---|
| K1 | **Linearizable per-AoR CAS.** `commit(tenant, aor, expected, set)` succeeds iff the stored revision equals `expected`, atomically installing `set` at `expected + 1`. Commits for one `(tenant, aor)` form a single total order; commits for different AoRs are unordered relative to each other (§10.3: independent processing) |
| K2 | **Atomic multi-binding replacement.** All of one command's binding mutations become visible together or not at all; no reader ever observes an intermediate set (§10.3's atomicity requirement) |
| K3 | **Revision fencing.** Revisions are monotonic per AoR, never reset — including across periods where the set is empty. Any consumer holding revision *n* discards state or events labeled < *n*. RG-5's shard handoff fences on the same counter |
| K4 | **Change stream.** Every commit emits `Change { tenant, aor, revision }` — no binding payload; consumers re-read. Delivery is **best-effort**: the stream is a latency optimization and correctness never depends on it (K5 is the correctness bound) |
| K5 | **TTL-bounded read staleness.** `lookup` MAY be served from a cache whose age is bounded by the configured staleness TTL — **[sipx-clstr]** default 5 s. A consumer that misses every change event still converges within one TTL. Caches are never the source of correctness: the bound, not the invalidation, is the guarantee |
| K6 | **Reads feeding the CAS loop.** `read` returns the latest committed `(set, revision)`. Staleness there is a liveness concern only — a stale read manifests as a `CasConflict` and a retry, never as a lost update. That is the sense in which per-AoR serialization holds "regardless of backend" |

The staleness consequence is stated, not hidden: within one TTL, a branch may still be built
toward a just-removed binding, and a just-added binding may be missed. Both are within
RFC 3261 semantics — registration propagation was never synchronous with call setup — and the
TTL prices it.

### 6.2 PostgreSQL mapping (backend #1, sketch — RG-4)

- One row per `(tenant, aor)` holding the revision and the binding set; per-AoR
  serializability is enforced by the revision predicate
  (`UPDATE … SET set = $4, revision = revision + 1 WHERE tenant = $1 AND aor = $2 AND
  revision = $3`), so K1 holds at any isolation level that keeps single-statement atomicity —
  the design's "serializable per-AoR transactions" achieved by fencing rather than by a
  global isolation setting.
- `LISTEN/NOTIFY` as the change stream: `NOTIFY` on commit with the K4 payload
  (`tenant`, canonical AoR, revision — bounded by §3.2 N13, well inside the notify payload
  limit). Best-effort by nature, which is exactly the K4/K5 contract.
- Read path: per-process caches invalidated by `NOTIFY`, expired by the K5 TTL.
- Expired bindings are invisible per §5.3 regardless of storage; row garbage collection is a
  background concern. Empty-set AoR rows persist (K3 monotonicity) and are GC'd only after a
  horizon covering the cache TTL and any in-flight shard handoff fence (RG-5).

### 6.3 In-memory backend (RG-3)

The same trait, trivially serialized, running under the deterministic harness — the backend
the contract tests and this spec's vectors are executed against first; the PostgreSQL backend
passes the identical suite (RG-4).

## 7. Lookup contract

`lookup(tenant, aor, now)` returns the target set the proxy forks on — shaped for
proxy-behavior §2 `TargetsResolved(Vec<Target>)` and consumed by its §7 F2/F7 (RG-6 wires
it):

```rust
struct Target {
    contact: Bytes,          // verbatim registered Contact URI — F2's "keeps its parameters"
    route_set: Vec<Bytes>,   // stored Path vector, stored order (topmost first) — the route toward the UA
    flow_ref: Option<Bytes>, // opaque; present when the binding rides a client-initiated connection (AF-2)
    q: u16,                  // thousandths, 0–1000
    expires_in: Duration,    // remaining lifetime at `now`
}
```

| # | Rule |
|---|---|
| L1 | Expired bindings (`expires_at` ≤ `now`) are excluded; `now` is the caller's input, never a read of a clock |
| L2 | Order: descending `q` (RFC 3261 §10.2.1.2 — higher q is more preferred). A Contact without `q` sorts as 1000 — **[sipx-clstr]**: an unstated preference is full preference, so plain registrations are tried first, not last |
| L3 | Tie rules, in order: more recent `refreshed_at` first (**[sipx-clstr]** — recency is the best available reachability signal), then ascending canonical contact bytes as the final total-order tie-break. The order is a pure function of the set and `now`: every node computes the identical order, which the deterministic harness asserts (vision principle 6) |
| L4 | The list is flat with `q` carried per target; equal-`q` targets form the proxy's parallel fork groups and distinct `q` values its sequential groups — grouping is §16.6's business, ordering is this spec's |
| L5 | Unknown AoR, or no active bindings: the empty set. What to answer upstream is the consumer's decision (proxy-behavior §16.5) |
| L6 | The lookup input is canonicalized with §3 before keying — a Request-URI spelling variant resolves to the identical target set |
| L7 | `flow_ref` and `route_set` are carried verbatim; the location service never interprets either |

## 8. Sharding key (scope note for RG-5)

Rendezvous hashing shards ownership over:

```
shard_key = tenant_bytes 0x00 C(aor)
```

Injective because `tenant` contains no NUL (§4) and `C(aor)` is printable ASCII (§3.2 — every
non-printable byte is escaped). RG-5 specifies the hash function, weights and handoff; this
spec fixes only the input bytes, so any change to canonicalization is by definition a change
to this spec — two spellings of one AoR hashing to two shards is the failure §3 exists to
make impossible.

## 9. Test vectors

Vectors are normative; the harness (RG-3 first, RG-4 against the same suite) executes them.
`K =` the canonical key; policy defaults per §5 unless a row states otherwise
(min 300 s in LS-R rows where a minimum matters, quota 2 in quota rows).

**Canonicalization (LS-C).** Input → canonical bytes, or rejection.

| # | Input | Canonical key / result | Note |
|---|---|---|---|
| LS-C-1 | `sip:alice@ATLANTA.com` | `sip:alice@atlanta.com` | host folds (N5) |
| LS-C-2 | `SIP:alice@AtLanTa.CoM;Transport=tcp` | `sip:alice@atlanta.com` | = LS-C-1: scheme folds (N1), params stripped (N8) |
| LS-C-3 | `sip:%61lice@atlanta.com` | `sip:alice@atlanta.com` | = LS-C-1: escape of unreserved decodes (N2) |
| LS-C-4 | `sip:Alice@atlanta.com` | `sip:Alice@atlanta.com` | ≠ LS-C-1: user case-sensitive (N3) |
| LS-C-5 | `sip:alice@atlanta.com:5060` | `sip:alice@atlanta.com:5060` | ≠ LS-C-1: explicit port is a different key (N7) |
| LS-C-6 | `sip:alice@atlanta.com:05060` | `sip:alice@atlanta.com:5060` | = LS-C-5: numeric port (N7) |
| LS-C-7 | `sips:alice@atlanta.com` | `sips:alice@atlanta.com` | ≠ LS-C-1: sips never sip (N1) |
| LS-C-8 | `sip:alice@atlanta.com.` | `sip:alice@atlanta.com` | = LS-C-1: trailing dot (N5) |
| LS-C-9 | `sip:a;b@h.example` and `sip:a%3Bb@h.example` | both `sip:a%3Bb@h.example` | one key per §10.3 full decode, though not §19.1.4-equivalent — deliberately coarser (N2, §3.1) |
| LS-C-10 | `sip:alice%2fx@h.example` and `sip:alice%2Fx@h.example` | both `sip:alice%2Fx@h.example` | uppercase hex (N2) |
| LS-C-11 | `sip:+15550101@gw.example;user=phone` | `sip:%2B15550101@gw.example` | `user`-param stripped — §10.3 names it; `+` is not unreserved, so it re-escapes (N2, N8) |
| LS-C-12 | `sip:+1-555-0101@gw.example` | `sip:%2B1-555-0101@gw.example` | ≠ LS-C-11: no tel folding in sip user parts (N4) |
| LS-C-13 | `sip:bob@[2001:DB8:0:0:0:0:0:1]` | `sip:bob@[2001:db8::1]` | = `sip:bob@[2001:db8::1]` (N6, RFC 5952) |
| LS-C-14 | `sip:bob@phone.example` vs `sip:bob@192.0.2.4` | distinct keys | hostname never matches an IP (N6) |
| LS-C-15 | `sip:h.example` | `sip:h.example` | userless AoR is valid and distinct from every user-bearing key (N11) |
| LS-C-16 | `sip:null-%00-null@h.example` | `sip:null-%00-null@h.example` | NUL stays escaped; the key stays printable (N2) |
| LS-C-17 | `sip:@h.example` | reject | empty user (N11) |
| LS-C-18 | `sip:alice:secret@h.example` | reject | password (N10) |
| LS-C-19 | `sip:alice@h.example?subject=x` | reject | URI headers (N9) |
| LS-C-20 | `tel:+15550101` | reject | not sip/sips (§3.2) |
| LS-C-21 | `sip:alice%zz@h.example` | reject | malformed escape (N12, kernel) |
| LS-C-22 | user part whose canonical form exceeds 512 bytes | reject | N13 |

**REGISTER / CAS commands (LS-R).** Fixture: tenant `t1`, AoR key K; contacts CA, CB, CC.

| # | State / request | Expect |
|---|---|---|
| LS-R-1 | Empty set; REGISTER Call-ID `i1`, CSeq 1, `CA;expires=3600` | `200`; set `{CA/3600}`; revision 1; response lists `CA;expires=3600` |
| LS-R-2 | Refresh: `i1`, CSeq 2, CA | applied (B3); revision 2 |
| LS-R-3 | Retransmit/retry **500 ms after** LS-R-2, i.e. `now` + 0.5 s: `i1`, CSeq 2, CA, same granted duration | Noop (B4/B4.1): no mutation, `200` with current set, revision still 2, `expires_at` unchanged — the delay is stated because a rule that compared deadlines would pass this row only at zero latency (B4.2) |
| LS-R-4 | Stale: `i1`, CSeq 1 | abort, `500` (B5); store untouched |
| LS-R-5 | New Call-ID `i2`, CSeq 1, CA | applied (B2 — the UA restarted) |
| LS-R-6 | Set `{CA, CB}`; REGISTER `CA;expires=0` | CA removed; `200` lists only CB — the complete-set rule (§5.6) |
| LS-R-7 | Set `{CA, CB}`, distinct Call-IDs; `Contact: *`, `Expires: 0`, fresh Call-ID | all removed (W3); `200` with no Contact values; revision bumped |
| LS-R-8 | `Contact: *` plus `Contact: CA` | `400` (W1) |
| LS-R-9 | `Contact: *`, `Expires: 3600` — or no Expires header | `400` (W2) |
| LS-R-10 | `Contact: *`, `Expires: 0`, same Call-ID as a stored binding, CSeq not higher | abort, `500` (W3/B5); nothing removed |
| LS-R-11 | min 300: `CA;expires=60` | `423` + `Min-Expires: 300`; nothing committed (E6) |
| LS-R-12 | max 7200: CA, `Expires: 86400` | granted 7200; response `CA;expires=7200` (E5) |
| LS-R-13 | `Expires: 600` header + `CA;expires=1800` | 1800 — the parameter wins (E1 > E2) |
| LS-R-14 | CA, no expires anywhere | default 3600 (E3) |
| LS-R-15 | quota 2, set `{CA, CB}`; REGISTER new CC | `403`; refresh of CB instead → `200` (§5.5) |
| LS-R-16 | min 300: one REGISTER with `CB;expires=3600` and `CC;expires=60` | `423`; **neither** commits — atomicity (E6, K2) |
| LS-R-17 | REGISTER with `Path: P2, P1` and `Supported: path` | stored topmost-first [P2, P1]; `200` echoes the Path unmodified in order, `Supported: path` present (§5.6) |
| LS-R-18 | Path present, no `Supported: path` | `421 Extension Required` naming `path`; nothing committed (§5.6) |
| LS-R-19 | Stored `sip:c@h.example;x=1` (via `i1`/1); REGISTER `sip:c@h.example`, `i1`/2 | refreshes that binding — §19.1.4 match, first-match-in-creation-order rule (§5.3); no second binding |
| LS-R-20 | `Require: nothing-we-know` | `420` + `Unsupported: nothing-we-know` (S2) |
| LS-R-21 | CA expired at `now`, stored CSeq 9; REGISTER `i1`, CSeq 3, CA | added fresh (B1) — an expired binding is absent for every purpose (§5.3) |
| LS-R-22 | LS-R-3's retry 500 ms later, but `CA;expires=7200` | abort, `500` (B5): the carve-out is B4.1's *duration* match, not the token alone; a same-token command asking for something else is a second write. Nothing commits and the revision does not move |
| LS-R-23 | Set `{CA}` written by `i1`/1; REGISTER `i1`, CSeq 1, `CA` (same granted duration) **and** CB, 500 ms later | `200`, commits: CA untouched (B4 — same deadline, same `refreshed_at`), CB added (B1), revision bumped. B4.3 — the no-mutation guarantee is about the matched binding, not the request |
| LS-R-24 | Set `{CA, CB}` written by `i1`/1; **one** REGISTER `i2`/1 carrying `CA;expires=0`, `CB;expires=0` and `CC;expires=3600` | `200`; the committed set is exactly `{CC}` and the response lists CC alone. B6 — CA's removal shortens the set, and CB's removal is still resolved against CB. A registrar resolving every operation against a view captured before the first mutation leaves CB bound |
| LS-R-25 | Set `{CA, CB, CC}` written by `i1`/1; **one** REGISTER `i2`/1 carrying `CA;expires=0` and `CB;expires=7200` | `200`; the committed set is `{CB, CC}`, CB granted 7200 and CC untouched. B6 — the refresh lands on CB, not on whatever the removal shifted into CB's former place |
| LS-R-26 | Set `{CA, CB}` written by `i1`/1; **one** REGISTER `i2`/1 carrying `CA;expires=0` and `CB;expires=0` | `200`; the committed set is **empty** and the response lists no contacts; revision bumped. B6 — CA's removal shortens the set, and CB's removal must still resolve to CB rather than past the end |
| LS-R-27 | Set `{CA, CB, CC}` written by `i1`/1; **one** REGISTER `i2`/1 carrying `CB;expires=7200` and `CA;expires=0` | `200`; the committed set is `{CB, CC}`, CB granted 7200 and CC untouched — LS-R-25's operations in the opposite order. B6 — the refresh does not move CA, so the removal that follows it still resolves to CA |
| LS-R-28 | Empty set; **one** REGISTER `i2`/1 carrying `CC;expires=3600` and `CC;line=7;expires=0`, the two §19.1.4-equivalent | `200`; the committed set is **empty**, and the revision does not move (B9 — the operations cancel, so the reconciled set is the set that was read). B7 — the removal applies to the binding the first operation just added, which carries this request's own token; a registrar running B4/B5 against that token aborts the whole request with B5's failure code and leaves the UA unregistered |
| LS-R-29 | Empty set; **one** REGISTER `i2`/1 carrying `CC;expires=3600` and `CC;expires=7200` | `200`; the committed set is `{CC}` granted 7200, one binding not two; revision bumped. B7/B6 — the later operation replaces the binding the earlier one added; it is neither a retry (B4) nor a second write under a spent token (B5) |
| LS-R-30 | Quota at its default; nine bindings held; **one** REGISTER `i2`/1 carrying `CC;line=1`, `CC` and `CC;line=2`, all `expires=3600` — a §19.1.4 chain where the bare spelling is equivalent to both tagged ones while the tagged ones are not equivalent to each other. Then the same fixture, and a REGISTER carrying the two tagged spellings alone | `200`, and the committed set holds 10 bindings — B6/B7 collapse the three operations onto one, so the committed outcome is within the quota (§5.5) and a check bounding additions from above refuses a registration the quota permits. The two tagged spellings alone do commit two bindings, so that request is `403`, the set stays at 9, and the revision does not move |
| LS-R-31 | LS-R-29's request delivered a second time, 500 ms later | `200`, nothing committed, the revision unchanged, and the set still one binding granted 7200 (B8) — the command's net outcome for that binding is the later operation's grant, which is what the store already holds, so §5.3's per-binding idempotency rule makes the re-presentation a retry rather than a second write under a spent token |
| LS-R-32 | LS-R-28's request delivered twice, the second 500 ms later | `200` both times, no contacts, and the revision does not move on **either** delivery (B9). The two deliveries are indistinguishable from the durable state — the set is empty before and after each, and no binding survives to carry the ordering token — so a bump on one is a bump on every retransmission |

**Consistency / CAS (LS-K).**

| # | Given | Expect |
|---|---|---|
| LS-K-1 | Two commands read revision 5 and both compute commits | first `commit(5)` → revision 6; second → `CasConflict` carrying current state; its driver re-reads and commits at 7 — no lost update (K1, K6) |
| LS-K-2 | The conflicting command is a retry of the first | after re-read, `process` yields Noop (B4): `200` with the current set, no commit — CAS and idempotency compose |
| LS-K-3 | Commit at revision 6; its change event is dropped | consumers serve stale for at most the TTL, then converge (K4, K5) — the bound is asserted, not the delivery |
| LS-K-4 | Consumer holds revision 6; event/state labeled 5 arrives | discarded (K3 fencing) |
| LS-K-5 | One REGISTER replaces CA and CB together | every reader observes the revision-5 set or the revision-6 set, never a mix (K2) |
| LS-K-6 | Authoritative `read` immediately after the revision-6 commit | returns revision 6 (K6); only `lookup` may lag, bounded by K5 |

**Lookup (LS-L).** Fixture set at `now`: A (q 1.0, refreshed t=100, Path [P2, P1]), B (q 1.0,
refreshed t=200, flow_ref present), C (q 0.5), D (expired), E (no q, refreshed t=50).

| # | Given | Expect |
|---|---|---|
| LS-L-1 | `lookup(t1, K, now)` | [B, A, E, C]; D absent (L1) — q 1000 group ordered by refresh recency (B t=200, A t=100, E t=50), then C at 500 (L2, L3) |
| LS-L-2 | E carries no `q` | sorts in the 1000 group (L2) |
| LS-L-3 | Two nodes, same store state, same `now` | byte-identical target order (L3 determinism) |
| LS-L-4 | All bindings expired | empty set; answering upstream is the proxy's decision (L5) |
| LS-L-5 | Target A | carries verbatim Contact bytes, `route_set` = [P2, P1] in stored order, remaining `expires_in`, q (L7, §5.6) |
| LS-L-6 | Target B | carries `flow_ref` opaquely (L7) |
| LS-L-7 | Unknown AoR | empty set (L5) |
| LS-L-8 | Lookup keyed by `SIP:alice@Atlanta.COM.` | identical target set to `sip:alice@atlanta.com` (L6) |

**Shard key (LS-H).**

| # | Given | Key bytes |
|---|---|---|
| LS-H-1 | tenant `t1`, AoR `sip:alice@ATLANTA.com` | `"t1" 0x00 "sip:alice@atlanta.com"` |
| LS-H-2 | tenant `t1`, AoR `SIP:%61lice@Atlanta.com` | identical bytes to LS-H-1 — spelling variants shard identically (§8) |
| LS-H-3 | tenant `t2`, same AoR | different key bytes — tenants never share a shard domain (§8) |
