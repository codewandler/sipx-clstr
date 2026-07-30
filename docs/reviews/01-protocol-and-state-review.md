# Protocol and state-machine review

**Reviewed:** 2026-07-30, repository `HEAD` `86e6b10`

**Focus:** SIP protocol correctness, sans-IO/driver boundaries, proxy transactions, registrar and
authentication semantics, durable-state failure behavior, and the tests and public claims that span
those layers.

## Scope and method

This was an independent adversarial review. I did not read another review. I read `AGENTS.md`,
`docs/vision.md`, the story board, the proxy/location/auth/probe specifications and their designs,
then traced the relevant code through `sipx-clstr-proxy`, `sipx-clstr-registrar`,
`sipx-clstr-node`, `sipx-clstr-probe`, and `sipx-clstr-sim`. I also checked the public README/site
claims, listener/deployment behavior, and the vector and socket tests that are supposed to join the
layers.

The sans-IO split itself is generally clean: the proxy and registrar cores take messages, time, and
state as data. Most severe defects are where a capable core meets an incomplete driver, or where a
driver erases facts the core deliberately preserved.

## Findings

| ID | Severity | Finding |
|---|---|---|
| P1 | Critical | The real node does not implement matched `CANCEL`, branch cancellation, or Timer C |
| P2 | Critical | ACK and in-dialog routing are not viable on the real node |
| P3 | High | A terminal fork result can launch lower-priority contacts afterwards |
| P4 | High | Multi-contact REGISTER reconciliation mutates through stale indices |
| P5 | High | PostgreSQL read failures can be returned as successful empty/no-op state |
| P6 | High | The registrar's S1 and S4 security gates are absent or applied to the wrong URI |
| P7 | High | The node drops normative REGISTER response facts |
| P8 | High | REGISTER bypasses the only concurrency bound |
| P9 | High | Advertised TCP support is inbound-only, and unroutable targets strand transactions |
| P10 | Medium | Malformed expiry and Path values are silently reinterpreted or discarded |

### P1 — Critical — The real node does not implement matched `CANCEL`, branch cancellation, or Timer C

**Evidence.** The proxy core has the right explicit input and effects: `UpstreamCancelled` exists at
`crates/sipx-clstr-proxy/src/types.rs:86-89`, and the engine immediately emits `AnswerCancel` and
propagates cancellation at `crates/sipx-clstr-proxy/src/context.rs:379-411`. The real node never
feeds that input. `serve` special-cases only REGISTER and ACK; every CANCEL creates a new independent
`ResponseContext` (`crates/sipx-clstr-node/src/driver.rs:795-808`, `1043-1053`). It also records but
does not perform `CancelBranch`, and drops `AnswerCancel`, `SetTimer`, and `ClearTimer`
(`driver.rs:1213-1231`). The project story acknowledges this exact caveat at
`docs/stories/PX-10-arm-the-timer-c-the-document-asks-for.md:73-88`.

This is not equivalent to CANCEL handling. A separately proxied CANCEL can happen to recompute the
same downstream branch when location state and fork position are unchanged, because the branch
input intentionally excludes the method. It still cannot immediately answer the CANCEL, mark the
owning INVITE context cancelled, cancel every still-live branch, or handle a changed/sequential
target set. Timer C never fires on real sockets. The normative rules require immediate `200`,
propagation to the owning branches, and branch settlement (`docs/specs/proxy-behavior.md:230-237`).

**Impact.** Losing devices can keep ringing after the caller cancels; a CANCEL can wait for a
downstream result rather than receive its immediate response; sequential forks can be cancelled at
the wrong position; and an INVITE branch that goes quiet after a provisional has no proxy-layer
deadline. The task and its admission permit can remain resident until some unrelated lower-layer
event. Yet `README.md:173` and `website/docs/intro.md:45` call CANCEL and Timer C “Working/today.”

**Required correction.** Correlate the kernel's matched-CANCEL event with the existing INVITE
context, perform `AnswerCancel` and `CancelBranch`, and schedule/cancel per-branch Timer C inputs.
Until the socket driver does that, the public capability tables must not claim these features.

### P2 — Critical — ACK and in-dialog routing are not viable on the real node

**Evidence.** Every ACK, not only a 2xx ACK, is sent through `forward_statelessly`
(`driver.rs:800-807`). That function treats the ACK Request-URI as a registered AoR, looks up the
first binding, ignores the Route set, and sends the request unchanged (`driver.rs:1237-1257`). A 2xx
ACK's Request-URI is the dialog remote target, normally the Contact from the 2xx, not the registered
AoR. Non-2xx ACK is transaction-scoped and must not be handled by this blanket 2xx path.

BYE, re-INVITE, and other in-dialog methods fare no better. The core correctly asks to resolve the
first Route or else the Request-URI (`crates/sipx-clstr-proxy/src/context.rs:118-126`), but the node
forces every such next-hop URI through `CanonicalAor` and the location store
(`driver.rs:1189-1196`). After the node's own loose Route is popped, the next hop is the remote
Contact. Looking it up under the original AoR store cannot route the dialog and normally produces
`480`. The Record-Route currently carries no routing token (`context.rs:254-263`), as the public
roadmap correctly says, but same-node RFC route processing does not require a token.

The end-to-end probe masks this. It does not consume Contact, To-tag, or Record-Route from the INVITE
2xx; it constructs fresh ACK and BYE requests to the configured echo AoR
(`crates/sipx-clstr-probe/src/engine.rs:446-455`, `635-658`). The echo accepts any BYE without dialog
state (`crates/sipx-clstr-probe/src/echo.rs:185-196`), while the simulation resolves every request
through the registrar (`crates/sipx-clstr-sim/tests/probe_end_to_end.rs:101-139`). Consequently the
probe can pass without ever sending an in-dialog request, despite claiming to consume the dialog
layer (`docs/specs/e2e-probe.md:17-20`) and to acknowledge and clean up a dialog
(`e2e-probe.md:61-75`).

**Impact.** A proxied call may receive its 2xx while the ACK is silently dropped. BYE and session
refreshes can fail, leaving calls and endpoint transaction state alive. This directly contradicts
the user guide's statement that BYE/re-INVITE route via the learned Route set
(`website/docs/guides/registrations-and-calls.md:60-70`) and weakens the README's “with audio” proof:
media after a 2xx does not prove ACK or dialog teardown.

**Required correction.** Drive in-dialog next hops directly through transport resolution after Route
preprocessing; implement the distinct ACK cases; and make the probe establish and retain an actual
dialog (remote target, tags, route set, and dialog CSeq) before it can claim P4 cleanup success.

### P3 — High — A terminal fork result can launch lower-priority contacts afterwards

**Evidence.** Target resolution retains every q-group in `self.queued` and drains only the leading
group (`context.rs:204-242`). On an INVITE 2xx or any 6xx the engine cancels only already-launched
pending branches; it neither clears the queued targets nor sets a terminal guard
(`context.rs:342-361`, `631-647`). `on_upstream_cancelled` likewise touches only launched branches
(`context.rs:379-411`). When the cancelled branch later settles, the generic final-response path sees
the retained queue and calls `fork_next_group` (`context.rs:364-374`). A single launched branch is
also left unfinished after 2xx while lower-q targets remain, because there may be no event that
reaches that generic path.

I reproduced the 2xx case against the built library with three targets: A and B at q=1.0, C at
q=0.5. A's 200 produced `[Respond, CancelBranch]`; feeding B's 487 then produced
`[Forward, SetTimer]` for C. All existing proxy vectors still passed. The current tests cover q-group
sequencing and terminal results separately, not their composition.

**Impact.** The proxy can originate a new INVITE after the call was accepted, globally rejected, or
cancelled. That causes unexpected ringing and privacy exposure and can create a second 2xx after the
caller believes cancellation completed. In the one-branch form it can instead strand the response
context.

**Required correction.** A terminal 2xx/6xx/upstream-CANCEL must discard never-launched targets and
prevent any future q-group launch while still accepting late 2xx from branches that were already
sent. Add cross-product vectors for multiple q-groups with 2xx, 6xx, and CANCEL.

### P4 — High — Multi-contact REGISTER reconciliation mutates through stale indices

**Evidence.** `explicit` clones the current `BindingSet` and builds one parallel `parsed` vector
(`crates/sipx-clstr-registrar/src/process.rs:132-150`). Every incoming Contact then finds an index in
that original vector (`process.rs:184-197`) but reads/removes/replaces that index in the already
mutated set (`process.rs:207-235`). A removal shifts later indices, while newly inserted contacts
never enter the match view. Missing indices are silently skipped at lines 208-210.

I reproduced this with current bindings `{A, B}` and one REGISTER containing
`A;expires=0, B;expires=0`. The returned `Outcome::Commit` still contained B. The first removal shifts
B from index 1 to index 0; the second operation finds original index 1 and silently continues. The
vector table covers a single-contact removal and a multi-contact early rejection, but not two
mutations of an existing set (`docs/specs/location-service.md:500-521`).

**Impact.** A registrar can return 200 while retaining a binding the client explicitly removed, or
refresh/remove the wrong later binding depending on operation order. The whole REGISTER still
commits atomically, but it commits the wrong state.

**Required correction.** Keep the parsed view and set mutation in one stable identity structure, or
update both together after every operation. Add order-sensitive vectors for remove/remove,
remove/refresh, refresh/remove, and insert followed by an existing-binding mutation.

### P5 — High — PostgreSQL read failures can be returned as successful empty/no-op state

**Evidence.** Any database or JSON decode failure in `PostgresStore::read` is converted to
`(BindingSet::new(), Revision::INITIAL)` (`crates/sipx-clstr-node/src/postgres_store.rs:227-240`). The
comment argues that the following revision-0 commit fences correctness. There is no following commit
for a no-op or rejection: `apply` returns those directly (`crates/sipx-clstr-registrar/src/store.rs:101-113`).
REGISTER queries, empty wildcards, and absent-contact removals can all take that path
(`crates/sipx-clstr-registrar/src/process.rs:56-89`, `99-105`, `199-203`). Lookup uses the same
error-to-empty read and therefore converts an outage into no targets (`postgres_store.rs:265-270`).

**Impact.** During a store outage or schema/decode fault, the registrar can return `200` with an
empty binding set for a query or claim a deregistration already holds while the durable binding is
still present. Calls become `480` “user unavailable” rather than an observable store failure. This
contradicts S10's `503` contract (`docs/specs/location-service.md:205-220`).

**Required correction.** Make authoritative reads fallible through the store contract, map read
failure to `Rejection::Unavailable`, and preserve a distinct lookup failure so proxy policy can
choose an honest response. Fault-inject read and decode failures; normal PostgreSQL conformance tests
do not exercise them.

### P6 — High — The registrar's S1 and S4 security gates are absent or applied to the wrong URI

**Evidence.** The normative processing order requires the Request-URI domain gate S1 and principal
authorization for the AoR at S4 (`docs/specs/location-service.md:205-220`). The auth spec explicitly
relies on S4 to prevent an authenticated credential from substituting another user's `To`
(`docs/specs/registrar-auth.md:48-52`, `271-280`, `318-320`). But `admit_audited` proceeds from a valid
digest directly to command parsing (`crates/sipx-clstr-registrar/src/parse.rs:100-134`), `process`
has no authorization policy input (`process.rs:29-52`), and `TenantPolicy` has no principal-to-AoR
mapping. `RegisterCommand.principal` is merely documented as “already authorized” and later stored
(`crates/sipx-clstr-registrar/src/command.rs:42-71`).

The node's only adjacent gate extracts a domain from `cmd.aor`—the `To` URI—not from the REGISTER
Request-URI, and returns 403 rather than S1's 404 (`driver.rs:993-1013`). A request whose Request-URI
names an unserved domain but whose To names a served AoR can therefore pass. Explicit AoR ports also
become part of the naive `rsplit('@')` result, causing configured-domain false refusals.

**Impact.** Once a real credential store is enabled, any valid tenant credential can create,
replace, or wildcard-remove any AoR in that tenant's served domain. Today the public node refuses to
start with a closed tenant and no real credential store, so this is a latent security boundary, not
a claim that the shipped open registrar authenticates users. The S1 error also permits/denies the
wrong authority and emits the wrong protocol response.

**Required correction.** Define an injected authorization policy and enforce it between digest
admission and mutation. Parse and validate the Request-URI authority independently from the To AoR,
with S1/S5's distinct 404/400 results. Add a vector where an authenticated principal targets another
AoR and one where Request-URI and To domains differ.

### P7 — High — The node drops normative REGISTER response facts

**Evidence.** The sans-IO result preserves q and Path for successful responses and structured data
for 420/421/423 (`crates/sipx-clstr-registrar/src/command.rs:97-170`, `223-240`). The node renders only
status plus `<contact>;expires=N` (`driver.rs:1015-1038`). It does not render stored q, echoed Path,
`Supported: path`, `Unsupported` option tags, `Min-Expires`, or the required `path` extension name.
The spec requires those values at `docs/specs/location-service.md:211-220`, `347-363`, and pins them
in LS-R-11/17/18/20 (`location-service.md:509-518`).

**Impact.** A UA cannot learn the minimum interval after 423, identify an unsupported Require tag,
confirm the Path route, or reconstruct the registrar's binding preferences from a successful
response. Core vector tests prove the internal outcome, not the wire response actually emitted by
the node.

**Required correction.** Centralize `Outcome -> Response` rendering and add socket-level assertions
for each normative header, including multi-value ordering and q formatting.

### P8 — High — REGISTER bypasses the only concurrency bound

**Evidence.** `AdmissionBound::gates` explicitly exempts REGISTER and ACK
(`driver.rs:306-319`), while the accept loop spawns one task per exempt arrival
(`driver.rs:524-588`). The rationale bounds the *cost of one* REGISTER, not concurrent REGISTERs
(`driver.rs:211-225`). PostgreSQL calls are synchronous operations wrapped in `block_in_place`
(`crates/sipx-clstr-node/src/blocking_store.rs:52-73`) and the backend serializes all work through one
client mutex (`postgres_store.rs:65-76`). Under a slow/unavailable database, arrivals can therefore
accumulate tasks without a registrar-specific cap.

**Impact.** The unauthenticated public registrar is the easiest method to flood and is outside the
only resident-work bound. Memory, tasks, auth-lock waiters, and blocking-pool demand can grow while
the advertised proxy admission gauge remains healthy. This makes the broad README claim of “a
bounded number of in-flight transactions” inaccurate (`README.md:173`).

**Required correction.** Give REGISTER a separately sized concurrency/queue budget that preserves a
reserved refresh capacity without making the method unbounded. Test sustained slow-store behavior,
not only that REGISTER succeeds while the proxy bound is saturated.

### P9 — High — Advertised TCP support is inbound-only, and unroutable targets strand transactions

**Evidence.** Every forwarded Via is hard-coded `SIP/2.0/UDP`
(`crates/sipx-clstr-proxy/src/forward.rs:132-141`). `destination_of` ignores URI transport parameters,
refuses hostname Contacts, and always constructs a UDP destination (`driver.rs:1261-1273`). The
listener test explicitly records that every branch leaves as UDP regardless of target
(`crates/sipx-clstr-node/tests/advertised_listeners.rs:167-173`). When target materialization fails,
`perform` logs and continues without feeding the engine's `BranchTransportError`
(`driver.rs:1198-1208`), even though that input exists specifically for this case
(`crates/sipx-clstr-proxy/src/types.rs:79-83`). With no branch stream, `proxy_request` can exhaust its
`JoinSet` and return while the context remains unfinished (`driver.rs:1079-1153`).

**Impact.** A TCP-only registered UA is contacted over UDP; a future actual TCP send would carry a
wrong Via transport token; a syntactically valid hostname Contact can cause no upstream final at
all. Public tables nevertheless say UDP and TCP are available today (`README.md:173`,
`website/docs/intro.md:47`).

**Required correction.** Resolve the next hop per the protocol kernel's resolver/transport
capability, build Via from the chosen transport, and always turn target construction/send failure
into a branch failure input. Scope public TCP claims to inbound listening until egress is proved.

### P10 — Medium — Malformed expiry and Path values are silently reinterpreted or discarded

**Evidence.** A present but non-numeric Expires header is converted to `None`
(`crates/sipx-clstr-registrar/src/parse.rs:179-184`), so processing silently applies a Contact value or
tenant default. A malformed Contact `;expires` is likewise converted to absence
(`parse.rs:241-250`), unlike malformed q, which correctly yields 400. Malformed Path values are
discarded one header at a time by `filter_map` (`parse.rs:306-314`).

**Impact.** Syntax errors change requested registration lifetime rather than reject the request, and
a registration that depended on Path can be committed with a partial/empty route set. The latter can
make a successful registration unreachable while evading the `Supported: path` policy.

**Required correction.** Preserve presence separately from successful parsing and reject malformed
Expires, Contact parameters, and every malformed Path vector atomically. Add byte-level negative
vectors for these cases.

## Validation performed

- `cargo test -p sipx-clstr-proxy -p sipx-clstr-registrar -p sipx-clstr-node --all-features` —
  passed (including 50 proxy vectors, 32 REGISTER vectors, 30 auth vectors, and node integration
  tests). The green result is important: P1/P2/P7/P9 are cross-layer gaps, and P3/P4 are missing
  vector compositions rather than existing-test failures.
- Built isolated review executables from the current crates in `/tmp` to exercise P3 and P4. P3
  printed `after_cancelled_branch_487=[Forward, SetTimer]` and forwarded the q=0.5 target after a q=1
  200. P4 printed `remaining=sip:b@127.0.0.1:5061` after one command removed both A and B.
- PostgreSQL tests were compiled, but live backend tests are conditional on
  `SIPX_CLSTR_TEST_DATABASE_URL` (`crates/sipx-clstr-node/tests/postgres_store.rs:8-19`, `39-58`); no
  live database or injected read/decode failure was part of this review run.

## Residual risks and coverage gaps

- The upstream ledger already records nonce uniqueness and the O(n) replay-window check; I did not
  duplicate those known open items as new findings. They remain material before authenticated
  registration is exposed.
- Affinity tokens, flow routing, stateless proxying, and cross-node dialog return are explicitly not
  shipped. The more immediate P2 issue is that the documented same-node route path does not work
  before those future features are involved.
- There is no real-socket conformance scenario for CANCEL, a compliant 2xx ACK, dialog-route BYE,
  TCP-only Contact, hostname Contact, or each REGISTER rejection header. The existing simulation and
  probe simplify precisely the protocol state that those scenarios need to prove.
- Store interfaces erase read errors, so fault-injection coverage cannot become complete until the
  error is represented in the trait. A live happy-path PostgreSQL suite alone cannot establish S10.
- I did not run a long-duration resource-exhaustion test. P8 follows directly from unbounded task
  creation plus a serialized blocking backend, but the exact failure curve depends on runtime and
  deployment limits.
