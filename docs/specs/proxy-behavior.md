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
the RFC 5393 branch-cookie computation (§6). The second is now a **decided** ledger row in
[upstream.md](../upstream.md), and the decision is **declined**: the cookie is keyed with the
cluster's own key family so an outsider cannot forge "not a loop" (§6), its inputs are this
engine's routing state, and the kernel has no proxy to consume it. It stays here, in
`sipx-clstr-proxy`'s `cookie` module — the seam that could have moved, recorded as one that does
not.

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
    Forward { branch: BranchId, request: Request, target: Target, next_hop: Uri },
    CancelBranch(BranchId),
    ResolveTargets(TargetQuery),
    SetTimer { timer: ProxyTimer, branch: Option<BranchId>, after: Duration },
    ClearTimer { timer: ProxyTimer, branch: Option<BranchId> },
    Terminate,
}
```

Effects are ordered; the driver performs them in order. `Forward` before the `SetTimer` that
guards it, always. The driver (PX-2) owns the mapping onto kernel client/server transactions.

**`Forward` carries the target *and* the next hop, because they are two different things.** The
target is what goes into the Request-URI (F2); the next hop is the URI whose address the copy is
actually sent to (F7). They coincide for a bare contact and diverge whenever a `Route` survives
preprocessing or the target carries a `Path` — and a driver that derived the next hop from the
target instead would send a mid-dialog request to the far end's contact rather than to the route
set's first hop. **[sipx-clstr]** The choice is the engine's, so the harness and the socket driver
cannot make it differently.

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

**[sipx-clstr] P1 recognizes a Record-Route *value*, not merely our host.** An edge identity is
host-scoped and port-agnostic (§5 above, and deliberately: a client that resolved a different port
is still talking to this node), so "the Request-URI names an edge" is far weaker than §16.4's
condition, which is that the Request-URI *is a value this platform placed in a Record-Route*. The
values this platform places have a fixed shape — no user part, and `lr` — so P1 fires only for a
Request-URI that has both. Without that narrowing, every mid-dialog request whose remote target
happens to sit on the same host as the edge is mistaken for a strict-routing recovery: its
Request-URI is replaced by our own `Record-Route` and its `Route` is consumed, so the request is
addressed to us and the dialog's remote target is lost. A loopback deployment — the edge on
`127.0.0.1` and its phones on `127.0.0.1` — is exactly that case, and so is any deployment that
puts an edge on the same address as a gateway.

## 5.1. Target determination (§16.5)

Applied after preprocessing and before forwarding. §16.5 has two cases, and which one applies is a
decision, so it belongs in the engine rather than in whatever the driver happens to ask a store:

| # | Rule |
|---|---|
| T1 | The request is **within a dialog** (its `To` carries a tag): the target set is *predetermined*. The Request-URI is the only target and no location lookup happens — a mid-dialog Request-URI is the dialog's remote target (§12.2.1.1), which is a contact and not an address of record |
| T2 | Otherwise the Request-URI is an address this platform is responsible for: the targets come from the location service — `ResolveTargets`, answered by `TargetsResolved`, with §7's empty-set rule |

**[sipx-clstr]** T1 is what V-03 was missing, and the failure it caused was total rather than
partial: an ordinary remote `Contact` is not a registered AoR, so treating a mid-dialog Request-URI
as one resolves to an empty set for every normal call — a `BYE` answered `480` and, since an `ACK`
has no response at all (§7.2), an acknowledgement silently discarded. A registrar that happens to
hold a binding under that contact's canonical key is worse, not better: the request is then
delivered to whatever that binding names.

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
| F7 | Determine next hop | Read off **the copy, after F6**: the first `Route` if it carries `lr`, else the Request-URI. The `lr` test is what makes the rule total across F6 — a first `Route` without it means the swap has already put the strict router in the Request-URI, so the Request-URI *is* the next hop. The engine computes it and carries it on the `Forward` effect (§2); the address it resolves to is the driver's (RT-1's route plan), the URI is not |
| F8 | Push Via | §6 branch; `rport` per kernel behavior |
| F9 | `Content-Length` | Present on stream transports (kernel framing rule) |
| F10 | Forward | `Effect::Forward` — one kernel client transaction per branch |
| F11 | Timer C for INVITE branches | Default **240 s**, configurable **> 180 s** — RFC 3261 **§16.6 step 11**, "the timer MUST be larger than 3 minutes". The bound is strict and the default does not sit on it, which is the same rule [cluster-config](cluster-config.md) §8 V7 states for `timers.timerC`, the key an operator actually sets; the two must not be read separately. A configured value at or below the floor is **not** silently raised to the floor — the floor is the one value the RFC forbids — so the default stands instead. Reset on every 101–199 |

### 7.1. The queue: which targets go out, and when

**[sipx-clstr]** Targets fork in `q` order, most preferred first. Equal-`q` targets are one **group**
and fork in parallel; distinct `q` values fork in **sequence**. Only the leading group is forwarded
— the remainder stays **queued**, and the next group is forwarded when the current one concludes
without concluding the context. A group wider than `Max-Breadth` is serialized into the same queue
rather than truncated (PB-V-8): the surplus is a device the user registered, and dropping it is not
the proxy's call to make.

The queue is therefore a set of requests *not yet sent*, and its lifetime is bounded from the other
end by **R12** and **C7**: it does not survive a result that concludes the context. Cancelling the
launched branches and leaving the queue intact is not a partial stop — the next branch to settle
finds a queue and forks it, so the request is re-originated after it was answered, globally rejected
or withdrawn.

## 7.2. ACK: one method, three messages (§17.1.1.3, §17.2.1)

`ACK` is the one method a proxy must split by semantics, because the same method name covers three
messages with three different owners. Conflating them is V-03: every `ACK` went down one path that
was right for none of them.

| # | Rule |
|---|---|
| K1 | The `ACK` for a non-2xx sent **downstream** belongs to the client transaction that received the final response, which generates it (§17.1.1.3). The proxy neither builds nor forwards it — the kernel does, on the branch's own transaction, and the copy the proxy would add would be a second one |
| K2 | The `ACK` for a non-2xx arriving from **upstream** belongs to the server transaction that sent that response. It is absorbed there — Completed → Confirmed, which stops the response being retransmitted (§17.2.1) — and it is **not** forwarded. There is no forwarding to do: the transaction it concludes is ours |
| K3 | The `ACK` for a 2xx is a **separately routed request** (§17.1.1.3) and takes §§4–5.1 and §7 like any other: validation, preprocessing, the predetermined target set (T1), F1–F9 and F7's next hop. It is forwarded with **no transaction of its own**, and it is **never answered** — there is no response to an `ACK`, so a request that cannot be forwarded is recorded and dropped, and no status is invented for it |

**[sipx-clstr]** K3's "never answered" is not a policy choice, and it is the same rule
[cluster-config](cluster-config.md) §8 V11 states for a node whose roles wire no forwarding path: an
`ACK` that cannot be delivered has no refusal available, so the outcome is a record. Every other
unroutable request settles as a state-machine input with a status attached (§7's `480`, §4's V-table);
an `ACK` settles as an explicit outcome with a reason and no message. **Neither is a silent drop.**

## 8. Response processing (§16.7, with RFC 6026)

| # | Rule |
|---|---|
| R1 | Match the branch (kernel does; unmatched responses on a stateful proxy are dropped) |
| R2 | Pop the topmost Via (ours); hook `ResponseReceived` fires here. A response with no Via left after the pop was addressed to us — not forwarded |
| R3 | `100` from a branch: absorbed, never forwarded |
| R4 | Other 1xx: forward immediately; reset that branch's Timer C |
| R5 | 2xx to INVITE: forward immediately, **always** — including after a final response was already chosen (RFC 6026); then cancel remaining pending branches |
| R6 | 6xx: forward immediately, cancel all pending branches |
| R7 | Final-response selection when all branches concluded: 6xx first (a MUST, discharged on arrival by R6), then the lowest class among received finals, then §8.1's within-class rank. `401`/`407` aggregate all challenge headers from all challenging branches into one response |
| R8 | `503` from a branch is treated as branch failure and MUST NOT be forwarded as `503`: it becomes `500 Server Internal Error` if it ends up the best response (§16.7 ¶ on 503) |
| R9 | Branch timeout (kernel timer or Timer C without provisional): behaves as `408 Request Timeout` from that branch |
| R10 | Branch transport error (§16.9): behaves as `503` from that branch (and therefore R8) |
| R11 | Forwarding the chosen final response cancels every still-pending branch; hook `BeforeResponseForward` runs before the `Respond` effect |
| R12 | A result that concludes the context concludes the whole **target set**, not only the branches in flight: a 2xx forwarded by R5 and a 6xx forwarded by R6 discard §7.1's queued groups as well as cancelling the launched branches (R11). A branch final arriving afterwards — typically the `487` settling one of those cancellations — completes selection and MUST NOT resume sequential forking |

### 8.1. Choosing within the class (§16.7 step 6's `MAY`)

§16.7 step 6 fixes the class and, deliberately, not the response:

> It MUST choose from the 6xx class responses if any exist in the context. If no 6xx class
> responses are present, the proxy SHOULD choose from the lowest response class stored in the
> response context. The proxy MAY select any response within that chosen class. The proxy SHOULD
> give preference to responses that provide information affecting resubmission of this request,
> such as 401, 407, 415, 420, and 484 if the 4xx class is chosen.

So the RFC settles the class — 6xx a MUST, the lowest class otherwise a SHOULD — and leaves the
pick inside it open. Its one steer is an *interest* rather than a closed list: prefer the response
the caller can act on. A fork ending in one `486` and one `404` names neither of the enumerated
codes, so the RFC will not adjudicate it. That is the premise, not the answer: a proxy that leaves
the choice open forwards whichever code the branches' arrival order happened to favour, and the
caller cannot tell which question was answered.

**The rule.** Within the chosen class the best response is the one of lowest **rank**; equal ranks
break to the lowest numeric code, so selection is total and independent of the order branches
concluded in.

| rank | codes | why |
|---|---|---|
| 1 | `401`, `407`, `415`, `420`, `484` | §16.7 step 6's own SHOULD, verbatim: these tell the caller what to change and retry *successfully*. `401`/`407` additionally aggregate every challenging branch's headers (R7) |
| 2 | `480`, `486` | a branch reached the user and the user's own side answered — present, not available now |
| 3 | every other code in the class | absence (`404`, `410`), silence (`408`, R9), our own cancellation (`487`), and rejections of the message rather than of the user (`400`) — lowest code first |

Ranks 1 and 2 name 4xx codes only, which is the class the RFC scopes its preference to. Every other
class falls entirely to rank 3 and is chosen by code alone: the plain reading of the MAY, and
nothing more is claimed for it. R8 removes `503` from the candidates before any of this runs.

**Why rank, and not the lowest code.** A forking proxy answers one question with one response. Each
branch's final is a statement about one *contact*; what goes upstream is a statement about the
*address of record*. `404 Not Found` says that address does not exist — a claim any other branch's
answer falsifies. Forwarding it over a `486` tells the caller something we know to be untrue about
the thing they asked for, and tells them to stop rather than to try again; `486 Busy Here` is true
at the AoR level and is the one they can act on. Forking to a busy contact and a stale one is an
ordinary registration rather than a corner case, so the difference is user-visible and not a
tie-break nobody sees.

Numeric order reads like a preference because the codes sort plausibly in places, and then stops:
`408` — a branch that never answered — outranks `486`, a branch that did, so silence beats an
answer; and a downstream `400 Bad Request` outranks both, reporting a fault in the message we sent
as the callee's status. Lowest code is a good tie-break, which is the job it keeps here. It cannot
carry a preference, and it was being read as one.

The rule is written down because the alternative was tried: with §8 silent, `PB-R-5` and the test
that proved it stated opposite outcomes for the life of the project, and neither was wrong about
anything this specification said (`PX-11`).

## 9. CANCEL and Timer C (§16.10, §16.8)

| # | Rule |
|---|---|
| C1 | CANCEL matching our server transaction: respond `200 OK` to the CANCEL immediately |
| C2 | Propagate CANCEL to every branch that has received no final response (a branch that never got a provisional queues its CANCEL until one arrives, per §9.1) |
| C3 | Branches cancelled before a final response conclude with `487 Request Terminated`; when all conclude, `487` is the best response upstream (unless a 2xx raced — then R5 wins) |
| C4 | CANCEL matching nothing: forward it statelessly (§16.10 ¶6) — see also §11 |
| C5 | Timer C fires, branch has seen a provisional: `CancelBranch` (then C3) |
| C6 | Timer C fires, no provisional: conclude the branch as timeout (R9) |
| C7 | A CANCEL matching our server transaction discards §7.1's queued groups as well as propagating to the launched branches (C2). The caller has withdrawn the request, so a target that was never tried is never tried — R12's rule, reached by the one route that concludes the target set without any response having been forwarded |

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
| ACK for a non-2xx | No match → K3's path: routed as a request if anything names a next hop, else an unroutable outcome that is **recorded** (an ACK never gets a response) | Upstream absorbs response retransmissions until its timer expires — bounded, harmless |
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
| PB-P-6 | In-dialog request (`To` tag), R-URI a remote contact | T1 — no `ResolveTargets` at all; forwarded to the Request-URI as the only target |
| PB-P-7 | R-URI at an edge's host but with a user part, `Route` present | **Not** P1 — the Request-URI is a contact, not a `Record-Route` value: it survives, and the `Route` is popped by P2 rather than consumed |

**Forwarding (PB-F):**

| # | Given | Expect |
|---|---|---|
| PB-F-1 | Dialog-forming INVITE, 1 target | Forward: Via pushed (cookie present), `Max-Forwards` decremented, Record-Route with token parameter ≤ 200 B, Timer C armed at F11's default, **240 s**, and armed *after* the `Forward` |
| PB-F-2 | 3 targets (q-ordered) | 3 branches, unique branch ids, same cookie field rules, `Max-Breadth` divided |
| PB-F-3 | Unknown header `X-Vendor: a, b` in request | Byte-identical in every forwarded branch |
| PB-F-4 | Next hop is a strict router | F6 swap: R-URI ↔ Route ends; F7's next hop is then the Request-URI, because the first `Route` no longer carries `lr` |
| PB-F-5 | Resolved target set is empty | → Respond `480` |
| PB-F-6 | In-dialog request, a `Route` for another element survives preprocessing | F7 — the next hop is that `Route`, and the Request-URI is left as the dialog's remote target |
| PB-F-7 | `ACK` for a 2xx carrying our `Route` | K3 — the `Route` is popped, the remote target is the next hop, `Max-Forwards` is decremented, a `Via` is pushed, no `Record-Route` is added, and nothing is answered |
| PB-F-8 | `ACK` for a 2xx that cannot be forwarded (`Max-Forwards: 0`) | K3 — an explicit unroutable outcome carrying the reason; no response of any status |

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
| PB-R-11 | A `404`, B `484`, all concluded | best = `484` forwarded — the resubmission preference beats a lower code |
| PB-R-12 | A `486`, B concludes on timeout (R9), all concluded | best = `486` forwarded — an answer outranks silence |
| PB-R-13 | A `500`, B `502`, all concluded | best = `500` forwarded — no rank applies, lowest code |
| PB-R-14 | A concluded `404`, then B answers `600` | `600` forwarded — R6, and the class order is a MUST |

**CANCEL / Timer C (PB-C):**

| # | Given | Expect |
|---|---|---|
| PB-C-1 | Upstream CANCEL, branches: A provisional, B nothing yet | `200` to CANCEL; CancelBranch(A); B's CANCEL queued until its first provisional |
| PB-C-2 | Cancelled branch answers `487` (all done) | `487` upstream |
| PB-C-3 | CANCEL races a `200` from A | `200` wins (R5); CANCEL still `200`-answered |
| PB-C-4 | CANCEL, no matching transaction | statelessly forwarded |
| PB-C-5 | Timer C, provisional seen | CancelBranch |
| PB-C-6 | Timer C, silence | branch concludes as `408` |

**A terminal result and the queued target set (PB-T):**

These are the cross-product the tables above do not reach. PB-F-2 forks sequentially with nothing
terminal happening; PB-R-3, PB-R-7 and PB-C-2 end contexts that have no queue behind them. Composing
the two is what §7.1, R12 and C7 are about, so the composition gets its own rows rather than being
assumed from the halves.

| # | Given | Expect |
|---|---|---|
| PB-T-1 | A, B at `q=1.0`, C at `q=0.5`; A answers `200`, B then answers `487` | `200` forwarded and the launched branches cancelled; B's later final produces **no** `Forward` — R5 concluded the target set, so C is never tried |
| PB-T-2 | A, B at `q=1.0`, C at `q=0.5`; A answers `600`, B then answers `487` | `600` forwarded and everything cancelled; B's later final produces **no** `Forward` — R6 discards the queue too |
| PB-T-3 | A, B at `q=1.0`, C at `q=0.5`; upstream CANCEL, then both branches answer `487` | `487` upstream and **no** `Forward` anywhere — C7: a withdrawn request never originates a new INVITE |

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
| PB-A-2 | ACK (non-2xx) at foreign edge, no route | not forwarded, and the outcome recorded — never answered |
| PB-A-3 | ACK (2xx) at foreign edge with token | routed normally (mid-dialog path) |
