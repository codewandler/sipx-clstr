# Spec: Proxy behavior

**Status:** normative · **Crate:** _future — created by CX-2_ · **Stories:** PX-1 … PX-7 ·
**Design:** [proxy-engine](../designs/proxy-engine.md)

## 1. Normative references

- RFC 3261 §16 (proxy behavior): §16.3 request validation, §16.4 route information
  preprocessing, §16.5 target determination, §16.6 request forwarding, §16.7 response
  processing, §16.8 timer C, §16.9 transport errors, §16.10 CANCEL, §16.11 stateless proxies.
- **RFC 5393** — loop-detection correction and `Max-Breadth` fork limiting. Updates §16.3;
  adopted, see §4 and §6.
- **RFC 6026** — 2xx INVITE response handling. Adopted via the kernel's transaction layer
  (sipx `docs/specs/sip-transaction.md` §5); this spec inherits it, see §8.
- RFC 3263 §4.4 — an element becomes transaction-stateful after a failed first attempt so
  retransmissions follow the selected destination. Interacts with §3; the failover rules
  themselves live with the routing-trunks epic (RT-4).
- sipx kernel contracts consumed: the lossless message model (verbatim re-serialization of
  untouched bytes) and the four transaction FSMs.

**Out of scope:** REGISTER semantics ([location-service](location-service.md), RG-1), token
internals ([affinity-token](affinity-token.md), AF-1), route-plan construction and trunk
failover (RT-1/RT-4), authentication policy (RG-2 and the hook framework).

**Upstream considerations** (AGENTS.md rule 6): the engine itself is orchestration and stays
here. Considered for upstream: the `Headers` surgery primitives (already ledgered, PX-3) and
the RFC 5393 branch-cookie computation (§6) — protocol-generic, flagged for CX-1 to raise with
the other ledger rows; until decided it is implemented here behind a seam that can move.

## 2. Sans-IO contract

The proxy engine is a state machine per proxied request (the *response context*). It reads no
clock, owns no socket, and queries no store; facts arrive as inputs, decisions leave as effects.

```rust
enum Input {
    Upstream(Request, ServerKey),         // new or matched server transaction, from the kernel
    BranchResponse(Response, BranchId),
    BranchTransportError(BranchId),       // §16.9: treated as 503 from that branch
    TargetsResolved(Vec<Target>),         // location service / routing policy answered
    TokenFact(TokenVerdict),              // affinity token verified or rejected (AF-4)
    TimerFired(ProxyTimer, Option<BranchId>),
    UpstreamCancelled,                    // CANCEL matched our server transaction
}

enum Effect {
    Respond(Response),                    // to the server transaction (upstream)
    Forward { branch: BranchId, request: Request, target: Target },
    CancelBranch(BranchId),
    ResolveTargets(TargetQuery),
    SetTimer { timer: ProxyTimer, branch: Option<BranchId>, after: Duration },
    ClearTimer { timer: ProxyTimer, branch: Option<BranchId> },
    Terminate,
}
```

Effects are ordered; the driver performs them in order. `Forward` before the `SetTimer` that
guards it, always. The driver (PX-2) owns the mapping onto kernel client/server transactions.

## 3. Modes

Transaction-stateful (§16.2) is the primary mode. Stateless (§16.11) is a strict subset — every
rule in §§4–8 applies unless §10 explicitly relaxes it.

**[sipx-clstr] Applicability rule** — a request MAY be handled statelessly only when *all* hold:

| # | Condition |
|---|---|
| A1 | It carries a Route with a valid affinity token (mid-dialog; token verified) |
| A2 | Target determination is deterministic and yields exactly one target |
| A3 | No feature on the request needs transaction state (no fork, no local challenge, no local 100, no recursion) |
| A4 | No prior attempt for this request failed over (RFC 3263 §4.4 forces stateful) |

Everything else — dialog-forming requests above all — is stateful. Stateless mode is
implemented in M2 (PX-4); the rules are normative now so PX-5 builds the superset.

## 4. Request validation (§16.3, amended by RFC 5393)

Checks run in order; the first failure responds and terminates. A response is only possible
when the request is well-formed enough to answer (kernel `ResponseBuilder::to_request`);
otherwise the request is dropped (never guessed at).

| # | Check | On failure |
|---|---|---|
| V1 | Reasonable syntax (kernel parse + validation findings; unknown headers/methods are NOT failures — lossless model) | `400 Bad Request` if respondable, else drop |
| V2 | URI scheme understood (`sip`, `sips`; others per profile) | `416 Unsupported URI Scheme` |
| V3 | `Max-Forwards` present → value > 0 | `483 Too Many Hops` |
| V4 | Loop detection (see below) | `482 Loop Detected` |
| V5 | `Max-Breadth` (RFC 5393): value ≥ 1 available for forwarding | `440 Max-Breadth Exceeded` |
| V6 | `Proxy-Require`: every option tag supported by the active profile | `420 Bad Extension` + `Unsupported` listing the offenders |
| V7 | Authentication (hook phases `BeforeAuth`/`AfterAuth`, [hook-framework](hook-framework.md); policy per RG-2 / tenant) | `407 Proxy Authentication Required` |

**[sipx-clstr] rules:**
- `Max-Forwards` absent → insert with value 70 before decrementing (§16.6 allows MAY; we do,
  so V3 is always meaningful downstream).
- V3 fires `483` for every method including OPTIONS — this platform never answers a proxied
  request on the target's behalf. Rationale: a proxy that impersonates targets on
  `Max-Forwards: 0` leaks topology and surprises diagnostics.
- Loop detection per RFC 5393: a `482` is returned only when one of our own Via entries is
  present **and** its branch cookie (§6) matches a cookie recomputed over the current
  routing-relevant state. Same Via, different cookie = spiral = legitimate; forward.

## 5. Route information preprocessing (§16.4)

Applied before target determination, in order:

| # | Rule |
|---|---|
| P1 | Request-URI equals a value this platform placed in a Record-Route (strict-routing predecessor): replace the Request-URI with the last Route value and remove that Route value |
| P2 | First Route value resolves to this platform: pop it; if it carries an affinity token, verify it — the token's verdict (tenant, direction, shard, media node) becomes a `TokenFact` input |
| P3 | Token verification failure on a mid-dialog request: hard reject per AF-1 — `[sipx-clstr]` the response is `403 Forbidden`; there is no fallback routing |

Recognizing "this platform" covers every configured edge identity, not only the receiving
node — any edge pops any edge's Route (that is the point of the token).

## 6. Via branch and the loop-detection cookie (§16.6 step 8, RFC 5393)

Every forwarded request gets one new topmost Via with a branch of the form
`z9hG4bK` `·` *transaction-unique part* `·` *loop-detection cookie*.

- The transaction-unique part makes the branch unique per client transaction (fork branches
  differ here).
- The cookie is a hash over the fields that determine how the request is **routed**: Request-URI,
  To tag, From tag, Call-ID, CSeq number, and the sequence of Route values. Identical cookie on a
  revisit = loop (V4); different = spiral.
- **The topmost incoming Via is deliberately NOT in the cookie** — corrected by PX-5, which found
  that including it makes V4 structurally unable to fire. A looping request arrives carrying *our*
  Via on top, so the recomputed cookie can never equal the one we minted, and every loop is
  misjudged a spiral and forwarded until Max-Forwards expires at every node on the cycle. The
  topmost Via decides where the *response* goes, not where the *request* is routed, so it is not
  part of "all information affecting processing of a request" (§16.3 step 4). §16.6 step 8's
  recommendation of it is about **entropy** for transaction uniqueness — which is where it belongs,
  and where the implementation puts it: the branch's transaction-unique part.
- **[sipx-clstr]:** the cookie is keyed (cluster token key family) so outsiders cannot forge
  "not a loop"; the hash and field-canonicalization are specified byte-exactly in the vectors.

`Max-Breadth` (RFC 5393): the incoming value (default 60 if absent) is divided across the
branches forwarded in parallel, each branch receiving `max(1, ⌊incoming / branches⌋)` — and a
request that cannot receive ≥ 1 is not forwarded (V5).

## 7. Request forwarding (§16.6)

**[sipx-clstr]** An empty resolved target set concludes the context with `480 Temporarily
Unavailable` (§16.5's terminal case; the location service returns empty sets rather than
deciding the response — [location-service](location-service.md) §7).

Per target, in order — the untouched remainder of the message re-serializes byte-exact (kernel
guarantee; the passthrough vectors assert it):

| # | Step | [sipx-clstr] specifics |
|---|---|---|
| F1 | Copy the request | Lossless copy; unknown headers/bodies untouched |
| F2 | Update the Request-URI to the target | Contact from location service keeps its parameters |
| F3 | Decrement `Max-Forwards` | After the V3/insert rule |
| F4 | Record-Route (dialog-forming requests when the platform must stay in the path) | One `Record-Route` per side carrying the affinity token as a URI parameter; direction distinguishes the sides. Byte budget: the token URI **parameter** MUST stay ≤ 200 bytes — the budget authority is [affinity-token](affinity-token.md) §3 (worst case 157 B including the 64 B module-facts ceiling); the bound is per parameter, not per header line |
| F5 | Add headers per policy/hooks | Hook phase `BeforeForward`; modules declare what they touch |
| F6 | Route postprocessing | Strict-routing next hop: move Request-URI to last Route, first Route to Request-URI |
| F7 | Determine next hop | First Route (lr) or Request-URI, handed to the route plan (RT-1); this spec consumes an ordered target, nothing more |
| F8 | Push Via | §6 branch; `rport` per kernel behavior |
| F9 | `Content-Length` | Present on stream transports (kernel framing rule) |
| F10 | Forward | `Effect::Forward` — one kernel client transaction per branch |
| F11 | Timer C for INVITE branches | Default **240 s**, configurable **> 180 s** — RFC 3261 **§16.6 step 11**, "the timer MUST be larger than 3 minutes". The bound is strict and the default does not sit on it, which is the same rule [cluster-config](cluster-config.md) §8 V7 states for `timers.timerC`, the key an operator actually sets; the two must not be read separately. A configured value at or below the floor is **not** silently raised to the floor — the floor is the one value the RFC forbids — so the default stands instead. Reset on every 101–199 |

## 8. Response processing (§16.7, with RFC 6026)

| # | Rule |
|---|---|
| R1 | Match the branch (kernel does; unmatched responses on a stateful proxy are dropped) |
| R2 | Pop the topmost Via (ours); hook `ResponseReceived` fires here. A response with no Via left after the pop was addressed to us — not forwarded |
| R3 | `100` from a branch: absorbed, never forwarded |
| R4 | Other 1xx: forward immediately; reset that branch's Timer C |
| R5 | 2xx to INVITE: forward immediately, **always** — including after a final response was already chosen (RFC 6026); then cancel remaining pending branches |
| R6 | 6xx: forward immediately, cancel all pending branches |
| R7 | Final-response selection when all branches concluded: best response by class order 6xx first (already handled by R6), then the lowest class among received finals; within 4xx, `401`/`407` aggregate all challenge headers from all challenging branches into one response |
| R8 | `503` from a branch is treated as branch failure and MUST NOT be forwarded as `503`: it becomes `500 Server Internal Error` if it ends up the best response (§16.7 ¶ on 503) |
| R9 | Branch timeout (kernel timer or Timer C without provisional): behaves as `408 Request Timeout` from that branch |
| R10 | Branch transport error (§16.9): behaves as `503` from that branch (and therefore R8) |
| R11 | Forwarding the chosen final response cancels every still-pending branch; hook `BeforeResponseForward` runs before the `Respond` effect |

## 9. CANCEL and Timer C (§16.10, §16.8)

| # | Rule |
|---|---|
| C1 | CANCEL matching our server transaction: respond `200 OK` to the CANCEL immediately |
| C2 | Propagate CANCEL to every branch that has received no final response (a branch that never got a provisional queues its CANCEL until one arrives, per §9.1) |
| C3 | Branches cancelled before a final response conclude with `487 Request Terminated`; when all conclude, `487` is the best response upstream (unless a 2xx raced — then R5 wins) |
| C4 | CANCEL matching nothing: forward it statelessly (§16.10 ¶6) — see also §11 |
| C5 | Timer C fires, branch has seen a provisional: `CancelBranch` (then C3) |
| C6 | Timer C fires, no provisional: conclude the branch as timeout (R9) |

## 10. Stateless mode (§16.11)

Relaxations and obligations relative to §§4–9 — everything not listed is unchanged:

| # | Rule |
|---|---|
| S1 | Applicability: §3 table only |
| S2 | No fork: exactly one target (A2) |
| S3 | Never send `100`; never originate a response except V-table rejections |
| S4 | The Via branch is a *deterministic function of the incoming top Via branch* — a retransmission produces the identical forwarded message |
| S5 | No retransmission on its own; retransmissions in = retransmissions out |
| S6 | Responses: pop our Via, forward per the next Via, no state consulted |
| S7 | Any condition that breaks A1–A4 mid-processing promotes the request to stateful handling before anything is sent |

## 11. Transaction affinity **[sipx-clstr]**

Dialog routing needs no node affinity (the token); *transaction-scoped* messages carry no Route
and must reach the edge holding the transaction. The dataplane MUST provide same-flow affinity
(same 5-tuple → same edge; connections inherently pinned) — a deployment requirement (DP-2),
not tuning. Degraded behavior when it fails anyway:

| Message at an edge without the transaction | Behavior | Consequence |
|---|---|---|
| INVITE/non-INVITE retransmission | Processed as a new request (indistinguishable) | Duplicate fork risk — why dataplane affinity is REQUIRED; residual rate measured under CF harness fault schedules |
| CANCEL | C4: stateless forward | Correct per §16.10; the owning edge's transaction still answers |
| ACK for a non-2xx | No match → routed as a request if it has a route, else dropped silently (ACK never gets a response) | Upstream absorbs response retransmissions until its timer expires — bounded, harmless |
| ACK for a 2xx | Not transaction-scoped: routes by token like any mid-dialog request | The normal path; any edge handles it |

## 12. Test vectors

Vectors are message-in / effects-out; the harness (CF-5) executes them verbatim. Notation:
`→` produced effects in order. Full byte-level fixtures accompany the implementation stories;
the rows here are the normative behavior matrix.

**Validation (PB-V):**

| # | Given | Expect |
|---|---|---|
| PB-V-1 | INVITE, `Max-Forwards: 0` | → Respond `483` |
| PB-V-2 | OPTIONS, `Max-Forwards: 0` | → Respond `483` (never answered on behalf) |
| PB-V-3 | INVITE, no `Max-Forwards` | insert 70 → decrement → forward with 69 |
| PB-V-4 | `Proxy-Require: nothing-we-know` | → Respond `420` + `Unsupported: nothing-we-know` |
| PB-V-5 | Unknown scheme `tel:` (profile: off) | → Respond `416` |
| PB-V-6 | Own Via present, cookie matches | → Respond `482` |
| PB-V-7 | Own Via present, cookie differs (spiral) | → forward normally |
| PB-V-8 | `Max-Breadth: 1`, target set of 2 | one branch forwarded with `Max-Breadth: 1`; second target serialized behind it, not dropped |
| PB-V-9 | Unparseable start line | → drop, nothing sent |

**Preprocessing (PB-P):**

| # | Given | Expect |
|---|---|---|
| PB-P-1 | R-URI = our Record-Route value; Routes present | R-URI ← last Route; that Route removed |
| PB-P-2 | First Route = our edge (valid token) | Route popped; TokenFact(verdict) consumed |
| PB-P-3 | First Route = *another* edge of ours (valid token) | Same as PB-P-2 — any edge pops any edge |
| PB-P-4 | Mid-dialog, token tampered | → Respond `403`; no forward, no fallback |
| PB-P-5 | Mid-dialog, token expired | → Respond `403` |

**Forwarding (PB-F):**

| # | Given | Expect |
|---|---|---|
| PB-F-1 | Dialog-forming INVITE, 1 target | Forward: Via pushed (cookie present), `Max-Forwards` decremented, Record-Route with token parameter ≤ 200 B, Timer C armed at F11's default, **240 s**, and armed *after* the `Forward` |
| PB-F-2 | 3 targets (q-ordered) | 3 branches, unique branch ids, same cookie field rules, `Max-Breadth` divided |
| PB-F-3 | Unknown header `X-Vendor: a, b` in request | Byte-identical in every forwarded branch |
| PB-F-4 | Next hop is a strict router | F6 swap: R-URI ↔ Route ends |
| PB-F-5 | Resolved target set is empty | → Respond `480` |

**Responses (PB-R):**

| # | Given | Expect |
|---|---|---|
| PB-R-1 | Branch sends `100` | absorbed |
| PB-R-2 | Branch sends `180` | forwarded upstream (Via popped), Timer C reset |
| PB-R-3 | Branch A `200` (INVITE), branch B pending | `200` forwarded; CancelBranch(B) |
| PB-R-4 | Late `200` on B after A's `200` chosen | forwarded too (RFC 6026) |
| PB-R-5 | A `486`, B `404`, all concluded | best = `486` forwarded |
| PB-R-6 | A `407` (nonce1), B `407` (nonce2) | one `407` with both challenges aggregated |
| PB-R-7 | A `600`, B pending | `600` forwarded; CancelBranch(B) |
| PB-R-8 | Sole branch `503` | upstream sees `500` |
| PB-R-9 | Branch transport error | as `503` from branch → R8 |
| PB-R-10 | Branch timeout | as `408` from branch |

**CANCEL / Timer C (PB-C):**

| # | Given | Expect |
|---|---|---|
| PB-C-1 | Upstream CANCEL, branches: A provisional, B nothing yet | `200` to CANCEL; CancelBranch(A); B's CANCEL queued until its first provisional |
| PB-C-2 | Cancelled branch answers `487` (all done) | `487` upstream |
| PB-C-3 | CANCEL races a `200` from A | `200` wins (R5); CANCEL still `200`-answered |
| PB-C-4 | CANCEL, no matching transaction | statelessly forwarded |
| PB-C-5 | Timer C, provisional seen | CancelBranch |
| PB-C-6 | Timer C, silence | branch concludes as `408` |

**Stateless (PB-S):**

| # | Given | Expect |
|---|---|---|
| PB-S-1 | Tokened in-dialog request, 1 target | forwarded; no timer set, no state kept |
| PB-S-2 | The same request retransmitted | byte-identical forwarded message (S4) |
| PB-S-3 | Response arrives | our Via popped, forwarded per next Via |
| PB-S-4 | A2 fails (2 targets) | promoted to stateful before any send (S7) |

**Affinity (PB-A):**

| # | Given | Expect |
|---|---|---|
| PB-A-1 | INVITE retransmission delivered to a second edge (simulated stickiness miss) | duplicate fork observed and counted — the metric exists; rate bounded by the fault schedule |
| PB-A-2 | ACK (non-2xx) at foreign edge, no route | dropped silently |
| PB-A-3 | ACK (2xx) at foreign edge with token | routed normally (mid-dialog path) |
