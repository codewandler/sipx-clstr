# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1] — 2026-07-29

### Fixed

- **The documentation site could not deploy**, and had not since `v0.4.0`. `docs/designs/cluster-viz.md`
  relative-linked into five story files; `docs/roadmap.md` picked up a sixth in `0.5.0`. Those paths
  resolve perfectly on disk — which is why the gate passed them — but `docusaurus.config.js`
  excludes `stories/**` from the published site, so each one is a broken link there, and Docusaurus
  fails the build rather than shipping one. Rewritten as absolute GitHub URLs, which is what the
  rest of `docs/` already did.
- **The gate had the matching hole**, which is the actual defect: a check that says "every relative
  link resolves" is not the same as "every published page resolves", and the difference is exactly
  the set of files the site excludes. `scripts/check-docs.py` gained that third rule, and it reads
  the exclude globs **from the site config** rather than restating them — a check that keeps its own
  copy of the list is one that eventually disagrees with the thing it checks. Per AGENTS.md, green
  locally and red in CI is a bug in the gate, so it was fixed there rather than worked around.

## [0.5.0] — 2026-07-29

### Added

- **M1 is complete** (`RG-2`, the fourteenth story) — **server-side digest, wired**. The decision
  core and its 22 `RA` vectors were already in; what landed here is the seam and the proof.
  - **`registrar::parse::admit` is where authentication happens**, and
    [registrar-auth](docs/specs/registrar-auth.md) §2's ordering is now structural rather than
    conventional: it runs `TenantAuth::decide` and builds a `RegisterCommand` **only** on
    `Proceed`, so a challenged request never exists as something `process` could store. It returns
    `Admission::{Command, Challenge, Reject}` rather than a `Result` — a challenge is not a
    failure, it is the first half of a round trip the client is expected to finish, and typing it
    as an error would put it on the same footing as a malformed message.
  - **`EdgeContext::principal` is removed.** An identity is not something an edge *knows*, it is
    what a decision *produced*; a settable field would have let a driver assert an identity nobody
    proved, which is the one mistake in this area no downstream test could catch. `admit` is now
    the only way a principal is ever attached, and `register_command` is explicitly the open-tenant
    path — it yields `principal: None`, which §3 A1 requires to be a *recorded* fact so the audit
    trail can say "unauthenticated" rather than fail to say anything.
  - **The command's tenant comes from the authenticator, not the edge.** §5 shapes the principal as
    `<tenant>:<username>` because a username is unique only within a tenant; sourcing the two from
    different places would let a misconfigured listener write tenant A's bindings under a principal
    naming tenant B. One source, so they cannot disagree.
  - **`Timestamp::as_secs`** is the clock seam — the location service counts nanoseconds and digest
    counts seconds. Truncating, deliberately: a nonce then expires a moment late rather than a
    moment early, so nobody is told `stale` for a nonce that was still good.
  - **The node driver can authenticate**, via `NodeConfig::auth` (realm, nonce secret, credentials),
    open by default — a default that quietly required credentials would make a node that answers
    nothing look like a node that is up. One `Mutex<TenantAuth>` per process, because the replay
    window is the thing it holds and a per-request authenticator is a window that never says no.
  - **`sipx-clstr-sim/tests/register_auth.rs`** — the harness scenario M1's fifth exit criterion
    asks for, in eight cases: the challenge/answer round trip, the principal reaching the stored
    binding, `RA-R-1`'s retransmission, `RA-R-2`'s forged twin, `RA-D-4`'s foreign realm refused
    `403`, a wrong password binding nothing, an open tenant recording `None`, and a byte-for-byte
    replay sweep under jitter. The phone's half is `sipx_ua::auth::respond` — the kernel's own
    client — so what it proves is that the two halves of digest agree, not that this file agrees
    with itself.

### Known issues

- **A retransmitted REGISTER authenticates and is then refused `500`** (`RG-8`, `ready`, priority
  1). The authentication half is correct — same nonce, same nonce-count, same digest, admitted
  twice, exactly as `RA-R-1` requires. What refuses it is
  [location-service](docs/specs/location-service.md) §5.3 B5, because B4's idempotency test reads
  "same granted expiry base" as the same **absolute deadline**, which is true only for a retry
  arriving in the very nanosecond of the original. Over UDP a lost `200` produces this every day.
  - `RG-3` had recorded the question and deferred it to the cluster stories, framed as a
    *re-presentation at a second node* stamping its own `now`. The harness reached it with one
    node, one phone and no cluster at all, which is what moved it from M2's problem to M1's.
  - Left unfixed here on purpose: §5.3 is normative, the reading changes there before the code
    does, and reversing a decision that is on the record belongs to the story that owns it.
    `a_retransmission_that_authenticates_is_still_refused_by_the_ordering_rule` pins the current
    behaviour meanwhile, so the defect is something a build fails on rather than a paragraph.

## [0.4.0] — 2026-07-29

### Added

- **M1's own end-to-end proof** (`CX-3`) — `scripts/e2e-call.sh` brings up one node and two real
  `sipx` CLI phones, registers both, places a call from one to the other over UDP, answers it and
  hangs up. Every step asserts on the CLI's `--json` output and its exit code. **Media is proved,
  not assumed**: alice plays a three-second 440 Hz tone and bob records 24 000 samples — exactly
  three seconds at 8 kHz — while the script asserts the node holds exactly *one* UDP socket, so the
  RTP bob heard came from alice and nowhere else. The CLI is deliberately not vendored: the value
  of the test is that the client side is somebody else's implementation of RFC 3261.
  - Four defects only a real network found: the node announced its address *before* binding, so a
    failed bind looked like a successful start; the exit code was swallowed by a pipe; the
    transaction gauge logged per completed request, so it could never observe the store draining
    after the last one; and `tracing`'s ANSI codes sat between a field name and its value, defeating
    the `grep` that read the gauge.
  - Two things that look like bugs and are not: the store takes ~32 s to drain, because RFC 3261
    keeps a concluded transaction alive for 64·T1 to absorb retransmissions — asserting "empty
    immediately" would be asserting a bug — and a contact naming a *host* rather than an address is
    not routable until `RT-1` brings RFC 3263 resolution.
- **The constellation's first feed** (`VZ-1`, [design](docs/designs/cluster-viz.md)) — the
  cluster, watchable: a dev adapter (`cargo run -p sipx-clstr-sim --example viz`) paces a seeded
  scenario against the wall clock and streams it live over SSE, and a canvas page renders the
  stage — messages as particles colored by method, drops/duplicates/breaks as visible faults,
  timers as sweeping rings, and the DP-3 invariant counters as a HUD that must not move. The
  stream is a serialization of the harness trace and nothing else: the frame vocabulary lives in
  the library (`sipx_clstr_sim::viz`) behind an exhaustive match, so a new trace variant breaks
  the build rather than rendering as nothing, and uninstrumented counters render as
  *uninstrumented* rather than as a pretend zero. std-only HTTP, localhost-bound, `serde_json`
  as a dev-dependency — no new runtime surface, and nothing a deployment could ship. The
  end-to-end smoke test (`cargo test -p sipx-clstr-sim viz_smoke`) spawns the real server and
  proves `/healthz`, the page, and the live frame stream — including backlog resync for a late
  client — and the runbook lives at `crates/sipx-clstr-sim/examples/viz/README.md`.
- **The Cargo workspace** (`CX-2`) — five crates drawn along the sans-IO boundary rather than by
  subject matter, because that boundary is what the deterministic harness depends on:
  `sipx-clstr-proxy` (RFC 3261 §16 forwarding), `-registrar` (bindings and REGISTER), `-sim` (the
  harness), `-probe` (the e2e-tester) and `-node` (drivers, roles, the `sipx-clstr` binary).
  **`tokio` is a dependency of `-node` and of nothing else** — the rule made mechanical, so a
  violation shows up as a dependency-graph change rather than as a code review someone has to do.
- **The kernel pinned to a released tag.** sipx is not on crates.io, so a git dependency is the
  only honest way to depend on it; `tag = "v0.2.1"` rather than a branch is what makes "which
  kernel version is this claim true of?" a question with an answer. `sipx-clstr --version` reports
  it, and a test asserts the constant it reports matches the tag the workspace actually pins —
  otherwise a bump with a forgotten constant leaves the binary confidently wrong about the one
  thing it is asked during an incident.
- **The gate** (`scripts/gate.sh`), which CI now runs step for step: fmt, clippy `-D warnings`,
  tests, the feature matrix, provenance, and documentation consistency.
  - `check-provenance.sh` carries **the integration carve-out**: `scripts/provenance-allow.txt`
    names interop targets that may appear anywhere, in the repository with a reason per line —
    not a contradiction, since a term we are willing to write down is by definition not one we
    refuse. Matching is exact rather than prefix, or one allowed name would silently permit a
    family of denied ones, and the script refuses to run if the allowlist swallows the whole
    denylist.
  - `check-features.sh` builds each crate with its optional features **off**. `--all-features`
    hides a crate that does not compile without one, and that stays invisible until it is in a
    release.
  - `check-docs.py` moves the documentation checks out of the CI workflow into a script both the
    workflow and `gate.sh` call, so there is one implementation instead of two that drift.

- **The echo endpoint** (`ET-3`), and with it **the first scenario in which every component is
  real** — the probe engine, the registrar, the forwarding core and the echo, with only a driver and
  a network supplied by the test. The separation §9 demands is structural rather than promised: a
  test reads the manifest and fails if this crate ever depends on the proxy, the registrar, the node
  crate or `tokio`. An unmarked call is refused `403` rather than `404`, because the
  address-of-record exists and the refusal is policy; `OPTIONS` is answered and unknown methods get
  `405`, because silence would make the echo look like the dead listener the probe exists to detect.
- **The probe engine and scheduler** (`ET-2`) — `sipx-clstr-probe`, 40 tests plus 5 harness
  scenarios, and **all 19 `EP-*` vectors proved**. No clock, socket or sleep in the crate: `now` is a
  parameter and jitter is an injected closure, so a failure scenario is a seed rather than a flake.
  One `ProbeRun` record, which `ET-4`'s API and `ET-5`'s metrics read rather than each inventing a
  view of it. The rate bound *defers and counts* rather than dropping — a skipped target is a blind
  spot, a delayed one is only late — and the matrix's first runs are spread across one interval,
  because a rollout starts many nodes at once.
- **The probe contract** (`ET-1`, [spec](docs/specs/e2e-probe.md)) — eleven normative sections and 19
  vectors, so the engine, the echo endpoint and the trigger API are derived from one contract rather
  than three opinions. `inconclusive` is why the verdict taxonomy is three-valued: a probe that could
  not run is not an outage, and conflating them trains operators to ignore the alert. A shed probe is
  its own cause, because silence means the listener is gone and shedding means the platform is
  protecting itself. The marker rides in `Subject`, which no intermediary has a reason to alter, so a
  marker that does not come back is evidence about the *path* — and it must not be derivable from the
  `Call-ID`, or something that merely saw the request go past could reflect it. Retries are explicitly
  not the probe's job: a probe that retried would mask the marginal loss it exists to report.
- **Decided: the echo is the same binary in `echo` mode**, a role rather than a service — one artifact
  to build, version and secure for something whose whole job is to answer `200` and copy a header. The
  constraint that made it a real question is absolute and is enforced at *load*: no proxy role ever
  links a UAS, so a configuration asking for both is refused where a human is still watching.
- **The `PostgreSQL` location store** (`RG-4`), verified against a real PostgreSQL 16. "Passes the
  identical suite" is now literally true: `run_location_store_suite` takes a `&dyn LocationStore` and
  both backends call that one function, because a suite copied per backend drifts and the copy that
  drifts is the one that stops catching things. Serializability is a **revision predicate**, not an
  isolation level, so K1 holds wherever a single statement is atomic; the first write is its own
  compare-and-swap, because a `SELECT` then `INSERT` would let two nodes that both think an
  address-of-record is new through — and a cold start is exactly when both think that. The backend
  lives in the driver crate, since it is IO, which also keeps `tokio` a dependency of that crate and
  of nothing else.
- **A measured registration storm**: 500 devices at 716/s on first registration and 661/s on refresh —
  flat, which is the finding, because one REGISTER costs one read and one write however large the
  estate. The test asserts the *ratio* rather than a rate, so it catches an accidental O(n) read path
  without encoding one machine's disk into the suite. Refresh writes are deliberately not coalesced:
  ~14 seconds for ten thousand devices is not a reason to trade away the guarantee that the `200`
  follows the commit.
- **The PB vector table as a generated, checked report** (`PX-7`) —
  `docs/reference/proxy-conformance.md`: 34 of 42 rows proved, 8 deferred with a reason and a story.
  Coverage is derived from **test names**, never a hand-maintained list, so deleting a test deletes
  the claim rather than leaving it standing. The check fails three ways, and the third is the one
  that matters: a *stale deferral* — a row marked "not yet" that is in fact covered — is how a
  coverage report starts lying about what it proves.
- **Adversarial schedules over a pinned seed corpus** (`PX-7`) — 35 % loss, 30 % duplication,
  1–120 ms jitter and a retransmission loop. A retransmitted INVITE never forks twice, asserted on
  the branches the engine created rather than on messages observed, because the network legitimately
  duplicates messages and only the branch count is the property. `HARNESS_SEED` replays any failure;
  a malformed one panics rather than silently running the corpus, since otherwise a developer would
  think they had reproduced something they had not.
- **The registrar and the proxy meet** (`RG-6`) — a location lookup becomes forking targets, behind
  the proxy's optional `registrar-targets` feature. Optional on purpose: the forwarding core also
  forwards to trunks and to plain Request-URIs, so it must not *depend* on the registrar, and a
  feature keeps the coupling visible in the manifest. Ordering is not redone — the lookup's order is
  already a pure function of the set and `now`, so every node computes the same one, and a second
  opinion here would be one too many. The scenario is one node running both real halves: two
  endpoints register, one calls the other, and the call forks to both of the callee's devices.
- **CANCEL and Timer C** (`PX-6`) — §9's C1–C6, with the `PB-C` vectors plus five harness scenarios
  in which the **real** engine runs behind a driver for the first time. `AnswerCancel` is its own
  effect because a CANCEL is its own transaction and its `200` is unconditional — it acknowledges the
  CANCEL, not the cancellation. A branch that has produced no provisional gets its CANCEL *queued*
  until one arrives, because a CANCEL that overtakes its INVITE cancels a transaction that does not
  exist yet while the INVITE proceeds. Timer C with provisionals seen cancels; Timer C with total
  silence concludes as `408`, since there is nothing at the far end worth cancelling. Verified across
  24 seeds with jitter and duplication, and in virtual time — the same Timer C assertion against a
  real clock would take three minutes a run.
- **Stateful forwarding with forking** (`PX-5`) — `sipx-clstr-proxy`, 47 tests, running every
  `PB-V`, `PB-P`, `PB-F` and `PB-R` vector. Validation, route preprocessing, the §16.6 forwarding
  edits, parallel and sequential forking, response aggregation and best-response selection.
  `PB-F-1` asserts the effect **sequence**, not just its contents: a Timer C armed before its INVITE
  went out would measure the wrong interval, and only an ordered assertion catches that.
- The rules that are easy to get subtly wrong, each pinned by a vector: `Max-Forwards: 0` refuses
  every method including OPTIONS (a proxy answering on the target's behalf leaks topology); a branch
  `503` becomes `500` upstream (the caller must not be told the destination is unavailable when what
  happened is that we could not reach it); a second `2xx` for one INVITE is forwarded too, because
  RFC 6026 makes that a fork rather than a bug; a re-INVITE is not Record-Routed (RFC 6141 — it
  cannot alter an established route set, so it is pure cost); `401`/`407` aggregate every challenge
  from every challenging branch.


- **The location service** (`RG-3`) — `sipx-clstr-registrar`, 78 tests, plus 5 harness scenarios.
  **Every vector table in the specification is executable**: 22 canonicalization rows, 21 REGISTER
  rows, 6 consistency rows, 8 lookup rows, 3 shard-key rows.
  - **Canonicalization is a total function, and §19.1.4 stays a comparison.** RFC 3261's URI
    equivalence is non-transitive by its own example, so it can compare two contacts but can never
    key a hash — two spellings of one address-of-record would land on two shards. The canonical
    form is an injective printable encoding of §10.3 step 5's URI; the kernel's `equivalent` is used
    only for matching within one address-of-record, where it is a linear scan.
  - **The whole request commits or none of it does.** Expiry selection runs for every contact
    before the first mutation, because one too-brief contact fails the entire REGISTER — deciding
    per contact as you go leaves the first one committed. The quota is checked against the committed
    outcome rather than the request, so a refresh or a removal can never trip it.
  - **CAS serialization is proved under the harness rather than asserted at a store.** A synchronous
    read-then-commit cannot race — the scheduler runs one input to completion — so the scenario's
    registrar edge is two-phase, the way a driver awaiting a database is: a REGISTER reads and arms
    the round trip, and the timer commits. Two edges given the same address-of-record at the same
    instant both read revision *n*, one wins, the loser re-reads and commits at *n+2*. The test
    asserts the race **happened**, not just that the result looks right: a serialization test that
    silently never raced would pass for the wrong reason. It also asserts the ordering nobody thinks
    to check — the `200` follows the commit, because a UA told it is reachable before the write
    landed has been told something false.
- **The deterministic cluster harness** (`CF-5`) — `sipx-clstr-sim`, 49 tests. Virtual time as a
  discrete-event queue where advancing the clock *is* popping the queue; equal deadlines broken by
  insertion sequence so the event order is a function of (scenario, seed) rather than of hash
  iteration; timer cancellation by generation counter. Two link kinds whose difference is
  load-bearing: a datagram link gets its reordering from latency jitter the way a wire does, and a
  stream link is FIFO however the jitter lands, never loses a message however lossy the policy,
  and fails by *breaking* rather than by silence. Messages are serialized and re-parsed across
  every link, so a node that builds a message it cannot write out is a bug the harness catches,
  and an unparseable arrival is traced rather than dropped — a serializer bug and an idle network
  look identical from the receiving end.
  - **The generator is written out rather than imported.** `rand` documents that its
    general-purpose generators may change output between versions, and a cargo update that
    silently reshuffled every recorded seed would break the one property the harness exists for.
    Pinned by `SplitMix64`'s published vector, by xoshiro256\*\* from state `[1, 2, 3, 4]` worked
    out by hand, and by a composite.
  - **The first implementation of that generator was wrong, and the hand-computed vector caught
    it.** xoshiro's state update is sequential — each step reads what the one before it wrote —
    and the natural Rust spelling, a single array literal, computes every element from the *old*
    state and yields a different, weaker generator that passes every smell test. One assertion
    separates them.
  - A livelocked scenario fails with a virtual timestamp instead of hanging CI.
- **The proxy transaction driver is designed** (`PX-2`,
  [design](docs/designs/proxy-transaction-driver.md)) — the seam `PX-5` implements against: one
  task per proxied request owning its response context, branch streams in a `FuturesUnordered` so
  responses, timers and an upstream CANCEL become one input at a time, no locks on the signalling
  path, and the three places backpressure actually lives.

- **The RoutePlan and the resolver decision** (`RT-1`, design accepted). The epic's headline
  question — build an async shared-cache resolver here, or get one upstream — is settled
  **upstream**: sipx `T-17` shipped it in `v0.4.0`, including the `_sip._ws`/`_sips._wss` prefetch
  this story was written to chase, so no caching layer is built here.
  - The seam that made it possible is `Prefetched`: RFC 3263 selection is pure computation over
    records, so the kernel awaits first and hands the answers to a synchronous `resolve` — which is
    how a proxy and a UA share selection logic without either becoming async.
  - What stays local is the *plan*, not the resolution. `RoutePlan` is an ordered list of
    `Attempt`s that **wrap** the kernel's `Target` rather than restating it, adding provenance and
    trunk context. A second address type here would eventually disagree with the kernel about which
    host a TLS certificate must be valid for, and that disagreement is a silent downgrade.
  - Every await lands in the driver by construction: the sans-IO core emits the `ResolveTargets`
    effect it already has, and the plan comes back as one input — so `RT-4`'s failover vectors are
    ordinary harness scenarios rather than tests that need a nameserver.
- **Failure is a scripted input** (`CF-4`). `sipx-clstr-sim::fault` adds `Fault` — `KillNode`,
  `Partition`, `Heal`, `SetLinkPolicy`, `TimerSkew` — and `Schedule`, both plain values, so a
  scenario can generate, mutate or fuzz its weather without touching scenario logic, and composing
  two schedules is concatenating two lists.
  - **No third mechanism.** Every fault is an override of what the link layer already models; a
    partition is `partitioned` on the crossing links and a kill is that on all of a node's. Two
    mechanisms that can both drop a packet eventually disagree about whether one was dropped.
  - **Faults are queue entries, not a side channel**, so they interleave with deliveries and timers
    under the same insertion-sequence tie-break and a scheduled run replays byte for byte from its
    seed — asserted both ways: same seed gives the same trace, a different seed gives a different
    one, or the faults would not be seeded at all.
  - **A kill is not an isolation.** `KillNode` drops the node's timer generations as well as
    cutting its links, so nothing fires afterwards; a node that keeps timing out while unreachable
    is a different failure, spelled `Partition` over all of its links.
  - A fault at time zero lands *after* nodes are started, so a `TimerSkew` there does not affect
    the timer a node arms in reaction to `Started`. Weather arrives at a running scenario.
  - The sim-vs-real fidelity criterion moved to `CF-3`, which is the story that builds the sockets
    to compare against.

### Changed

- **The harness keeps the kernel's timer queue instead of its own** (`CF-7`). sipx `v0.7.0` made
  `TimerQueue` generic over its **instant**, so `impl Add<Duration> for SimTime` is the entire
  adapter and `sipx-clstr-sim` hands the kernel its own clock — no runtime, no wall time.
  `queue.rs`'s `Generations<K>` is deleted; `SetTimer`/`ClearTimer`/`KillNode` are now `set`,
  `clear` and `forget_matching`.
  - **The tie-break was the part that mattered.** Timers live in the kernel's queue and
    deliveries, breaks and faults in this crate's, because those carry payloads. At an equal
    instant the scheduler drains a *fault first*, then timers, then the rest — exactly what the
    single queue's insertion-sequence rule produced, since faults are scheduled during setup. The
    first attempt drained timers first and moved two scenarios by one ping each, because a kill at
    T raced the timer at T instead of stopping it.
  - **No seed changed meaning and no expectation was edited.** The story's escape hatch for
    recording deliberate seed changes was not needed.
  - The loopback link is **decided local** rather than adopted, per `CF-1`'s per-component split:
    the kernel's is two-party where the harness needs an N-node mesh, has no stream class for
    §16.9, and owns its own generator — adopting it would change what every seed means.
  - Cost, stated plainly: `sipx-clstr-sim` now links `tokio` transitively. The harness never
    builds a runtime, and the decision crates depend on the harness rather than the reverse, so
    the registrar's sans-IO guard is unaffected.

- **The kernel pin moves `v0.2.1` → `v0.7.0`**, in two steps and for two different reasons.
  `v0.4.0` brought both sides of digest (`S-16`, `X-20`) and cleared every filed row in the
  [upstream ledger](docs/upstream.md) at once. `v0.7.0` then closed the one row that reopened on
  contact with the code: `TimerQueue` is now `TimerQueue<K, I = Instant>`, generic over its
  **instant** rather than only its key, so a virtual clock can drive it — which `CF-7` needs and
  `X-14` had not delivered. The default type parameter means no existing caller changed.
  - Everything in the suite passes at the same seeds against `v0.7.0`; three kernel releases of
    drift produced no behavioural change here.
  - `KERNEL_VERSION` moves with it, and `kernel_pin.rs` fails the build if it ever does not —
    an operator reading `--version` during an incident gets the tag the binary was actually
    compiled against.
- **`Path` is read as a typed header** (kernel `T-14`). `HeaderName::Other(b"Path")` matched
  nothing once the kernel started typing it, so a REGISTER carrying a `Path` registered an *empty*
  route set and the branch toward the callee silently lost its route. Caught by
  `a_registered_path_becomes_the_route_set_the_invite_carries`, fixed at the lookup rather than by
  re-recording the expectation.
- **Via surgery is one upstream call instead of a collection rebuild** (`PX-3`). sipx `S-15`
  shipped `Headers::remove_first` in `v0.4.0`, so the two sites that built a fresh `Headers` and
  copied every header except the topmost `Via` into it — the proxy's `pop_via` and the harness
  stub's `pop_top_via` — are each a single call. `remove_all` in `forward.rs` stays: it means
  *every* occurrence, and adoption ends where the semantics differ.
  - `tests/header_surgery.rs` pins what the rebuild was silently guaranteeing. The existing `PB-R`
    rows only check that our own `Via` is gone, which is equally true of a response whose remaining
    stack came back shuffled; the new rows use a three-`Via` stack and assert arrival order
    survives, and that headers either side of the stack are untouched.
  - One test reads the crate's source rather than its behaviour, because "uses the upstream API
    *exclusively*" is not a behavioural claim — a reintroduced rebuild would pass every other
    assertion in the file.
- **The upstream ledger grew a state for "written but not released", because that is where `RG-2`
  is.** The kernel work M1's last story needs now exists — `S-16` (server-side digest: nonce
  minting, challenge emission, verification, a bounded replay window) and `X-20` (which makes those
  primitives reachable from a crate with no async runtime, since `sipx-ua` pulled `tokio`
  unconditionally and a sans-IO registrar cannot). Both are commits on a **local** `main` — sipx's
  `origin/main` is behind them and no tag contains them. This workspace pins a git revision from
  the public URL so that "which kernel version is this claim true of?" has an answer, and a commit
  no remote has is not a revision it can name. `RG-2`
  therefore stays `blocked` and M1's fifth exit criterion stays unproven — recorded rather than
  worked around, because the two available workarounds were writing digest a second time (the thing
  `S-16` exists to prevent) and `[patch]`ing to a local checkout (an unreproducible build that
  hides the dependency from the ledger tracking it).
- **The proxy driver is built on the kernel's endpoint, not beside it.** The epic design argued
  that `sipx_transport`'s API was "UA-shaped: one transaction, one target" and that a forking
  proxy therefore needed its own socket loop. Reading the released kernel rather than remembering
  it found that wrong where it counts: `Handle::send` creates one client transaction per call, N
  calls fan out over one transaction layer, and `send` inserts a `Via` only when one is absent —
  so a proxy that pushes its own keeps control of the branch, and that branch *is* the transaction
  key. The alternative would have meant reimplementing framing, three handshakes, NAT `Via`
  rewriting, the pool and the retransmission driver, which is the rule against shadow-implementing
  kernel logic in as many words.
- **M1 is defined** rather than described: fourteen stories in a fixed order, each with what it
  adds and the vectors that prove it, plus exit criteria that are commands and an explicit
  out-of-scope table with reasons. The board reflects it — only stories with no unmet dependency
  are `ready`, so the top of `Next` is always genuinely takeable.
- **M1 does not block on the kernel.** Three of the four filed sipx stories would make M1 code
  nicer, not possible; only server-side digest (`RG-2`) genuinely waits, and it sits ninth.

### Fixed

- **The probe's requests carried no `Via`** (found by `ET-3`'s end-to-end scenario). RFC 3261
  §8.1.1.7 makes `Via` how a response finds its way back, so a compliant proxy refused the INVITE as
  unanswerable and the probe reported a timeout it had caused itself. 24 unit vectors and 5 harness
  scenarios had all agreed the engine was right; the first component that actually *validated* a
  request disagreed within one run.
- **The probe engine matched responses to whatever step was outstanding** (`ET-2`), with no
  correlation at all. Under the duplication UDP produces routinely, a second `200` to REGISTER
  arrived while the probe awaited INVITE and was read as the INVITE's answer — reporting a
  `MarkerMismatch` the probe had manufactured. Found at seed 5 of a 16-seed sweep; responses are now
  matched by the `CSeq` of the request that provoked them. Fixing it exposed that the unit fixtures
  built responses with **no `CSeq`**, which is precisely why they could not have caught it; they now
  play a peer that echoes it, as a real one does.
- **The probe's rate bound did the opposite of bounding** (`ET-2`): `now.saturating_sub(per)` clamps
  to zero, so the entry recorded at time zero was trimmed immediately and a second run went straight
  through.
- **Two races in the `PostgreSQL` backend, both flaky rather than broken** — the dangerous kind, since
  each passed on a re-run. `CREATE TABLE IF NOT EXISTS` is *not* atomic against a concurrent `CREATE`,
  so nodes starting together — a cold deployment, every rollout — collide on
  `pg_type_typname_nsp_index`; the DDL now runs under an advisory lock. And the tests shared one
  tenant while running concurrently, so one truncated another's rows mid-run and a commit came back
  reporting the row had vanished. Each test now owns a tenant, which is the contract's own boundary.
  Verified by ten consecutive runs against a freshly dropped table.
- **`PB-V-9`'s test had been deleted and nothing noticed** — it went with the block rewritten to fix
  the loop-detection cookie, and the suite stayed green because 45 other tests still passed. The new
  coverage check flagged it on its first run, which is the entire argument for deriving coverage from
  the spec rather than from the suite's own opinion of itself.
- **`Max-Breadth` was not being honoured** (`PB-V-8`). It bounds *parallel* fan-out; with a budget of
  1 and two targets the code forked both. RFC 5393 §5.2 requires the surplus to be serialized behind,
  not truncated — truncating silently loses a device the user registered, and nobody notices until a
  call does not ring on one phone.
- **Every registered `Path` was silently dropped** (found by `RG-6`). `forward()` implemented every
  §16.6 step except step 6 — applying the target's route set — so a registration's stored path never
  reached the wire. RFC 3327 §5.3 makes that path the route toward the contact, and losing it makes a
  UA behind a proxy unreachable in exactly the deployments Path exists for. Invisible to all 40
  `PB-*` vectors because every one of them uses an empty route set: the defect lived in the gap
  between two crates that were each correct alone, which is the argument for the integration
  scenario. Now applied in stored order, ahead of any `Route` that survived preprocessing, with bare
  URIs bracketed — because `sip:p;lr` unbracketed makes `;lr` a *header* parameter and loses
  loose routing.
- **Two CANCEL tests were passing for the wrong reason** (`PX-6`), and asking *why* they passed is
  what surfaced it. The harness driver silently dropped every CANCEL — `self.perform(self.context.on_input(…))`
  borrows `self` twice, and the stub helper written to dodge that returned no effects — and the
  adversarial sweep passed anyway, because `run_until_idle` drains the queue past Timer C at 180 s, so
  every seed reached its `487` through the timer rather than through the CANCEL. The scenarios now
  advance a bounded 10 s of virtual time, and the constant that bounds them explains why.
- **The loop-detection cookie could not detect a loop, and PX-5 proved it.** `proxy-behavior` §6
  listed the topmost incoming `Via` among the cookie's fields. A looping request arrives carrying
  *our own* `Via` on top, so the recomputed cookie can never equal the one we minted — every loop is
  misjudged a spiral and forwarded, round the cycle, until `Max-Forwards` expires at every node on
  it. The topmost `Via` decides where the *response* goes, not where the *request* is routed; RFC
  3261 §16.6 step 8 recommends it as **entropy** for transaction uniqueness, which is a different
  job. The spec is corrected, the cookie now covers routing state only, the `Via` feeds the branch's
  unique part, and `PB-V-6` asserts an actual `482` end to end — which it could not have done
  before.


- **Two rows of the upstream ledger were wrong** (`CX-1`). TLS, WebSocket and WSS were recorded as
  unreleased kernel work; they shipped in sipx 0.2.0, with interop runs, and `tls`/`ws`/`wss` are
  default features. Nothing was blocked on a release that had already happened. The resolver row
  assumed its own answer, so the story filed for it is written to *settle* upstream-vs-local
  instead. The ledger gained a rule: a row can be wrong — re-read the kernel before believing one.
- **The kernel gaps are filed** (`CX-1`): six stories in the sipx repository, in its conventions,
  each naming a failing-first test and each earning its place in the kernel on its own rather than
  only as a downstream ask. `PX-2` added two more, both found by reading the endpoint's dispatch:
  a response matching no client transaction is **dropped**, though RFC 3261 §16.7 requires a
  stateful proxy with no response context to forward it statelessly; and incoming requests are
  delivered with `try_send`, so under backpressure they are lost silently, with no counter. A
  dropped INVITE is a missed call the peer retransmits — a dropped 2xx ACK is a call that never
  ends. Neither blocks M1; both must close before M2 claims a node can be killed.

## [0.3.0] — 2026-07-28

### Added

- **A front door.** `README.md` explains the project to a person arriving cold: the problem (a
  five-node SIP proxy has behaviour no RFC describes), the answer (no shared call state — a signed
  token in the message carries what routing needs), and where this actually is. The status is
  stated in the first screen rather than discovered on the third: four specs written, no Rust yet.
- **A logo** (`docs/assets/logo.svg`) — the sipx crab, kept identical for family resemblance,
  shrunk inside a three-node mesh. At favicon size what survives is an orange body in a dark
  triangle, which is the distinction worth keeping: sipx is a phone, sipx-clstr is a cluster of
  them.
- **A published documentation site** at
  [codewandler.github.io/sipx-clstr](https://codewandler.github.io/sipx-clstr/) — Docusaurus
  reading `docs/` directly rather than copying it, so there is one set of words. Curated sidebar
  (the story board and the archive stay working material), offline search, mermaid diagrams, and a
  palette taken from the logo. It deploys on published releases only, so the public site follows a
  tag rather than the last hour's work.
- **CI for the documentation gate** (`.github/workflows/docs.yml`), making executable what
  AGENTS.md describes: relative links resolve, every `epic:` slug has a design doc, every `design:`
  path exists. It joins a build job when `CX-2` lands the Cargo workspace rather than being
  replaced by one.
- **MIT and Apache-2.0 licenses**, matching sipx.

### Changed

- **AGENTS.md gained a map** — where each kind of document lives, which are generated, and the
  state of play in one line — plus the publishing rule and two additions to the gate.
- **The downstream boundary is deployment-agnostic.** The gap stories filed from a consuming
  deployment carried that deployment's name through their notes and prose; a platform repo that
  names one consumer invites requirements shaped for that consumer. Traceability is kept by citing
  the ledger entry rather than the repo.

### Fixed

- **Links that only worked for the author.** `../../sipx` was a path to a sibling checkout;
  `../AGENTS.md`, `../CHANGELOG.md` and the board pointed outside what gets published. All are now
  absolute URLs that resolve in the repository and on the site alike.
- Contact-set notations in the location-service spec are code-spanned, matching the convention
  already used by their neighbours in the same table.

## [0.2.0] — 2026-07-28

### Added

- **The M0 load-bearing specs land** — written concurrently and cross-reconciled. The proxy
  behavior spec (PX-1): RFC 3261 §16 amended by RFC 5393 as a sans-IO engine with 42 vectors.
  The location service spec (RG-1): a canonical AoR byte form, the per-AoR CAS contract, the
  PostgreSQL and in-memory mappings, forking-ordered lookups. The affinity token spec (AF-1):
  byte-level layout with direction and a 64 B module-facts region, AEAD by default, worst-case
  157 B against the 200 B URI-parameter budget, stateless replay semantics — and a settled
  no-mid-dialog-refresh rule (route sets are fixed at establishment) now reflected in the
  media-control reselection risk and KO-9. The hook framework spec (EX-1): thirteen phases
  aligned to the proxy pipeline, closed per-phase effect sets, a manifest whose state-key
  domain makes dialog-keyed module state unrepresentable. The harness design (CF-1): a
  discrete-event clock, links-with-policies with faults as scheduled overrides, scenarios as
  code over declarative schedule values, a per-component sipx-testkit upstream split, and the
  conformance registry as a requirement-grain local extension of the kernel's per-RFC registry,
  kernel rows inherited by reference. (PX-1, RG-1, AF-1, EX-1, CF-1)
- **The operator epic starts moving and the backlog deepens.** The Helm chart scaffold under
  `deploy/helm/` (KO-2, in progress), the active-drain story (KO-9), and fifteen new stories
  across routing (egress allowlist, number normalisation, asserted identity, source-IP
  admission, scoped routes), deployment (split listeners, CDRs, capture), extensions (async
  external routing hook, carrier quirk profiles), media (per-trunk codec/SRTP policy), and the
  operator (naming contract, anti-affinity, OCI chart).

## [0.1.0] — 2026-07-28

### Added

- **Design scaffold (M0).** The track backlog framework, the vision and roadmap, the upstream
  dependency ledger, eleven epic design docs (proxy engine, registrar & location, cluster
  affinity, routing & trunks, media control, extension framework, conformance harness,
  deployment, the deferred B2BUA services placeholder, the end-to-end call probe, and the
  Kubernetes operator), architecture charts, and a 60-story backlog: a six-story ready queue for
  the M0 specs plus the implementation backlog toward M1 and M2 (M3/M4 stories are seeded when
  their milestones near). No code yet — the Cargo workspace arrives with `CX-2` as the first act
  of M1.

### Fixed

- **Review findings resolved across the design layer.** The ICE stance no longer lets anchored
  calls negotiate around the relay; the affinity token gained its missing direction field and
  honest, stateless replay semantics; transaction affinity is now an explicit dataplane
  requirement instead of an unstated assumption; previously unowned work got owning stories
  (CF-5 harness implementation, CF-6 conformance-registry seeding, AF-7 connection ownership,
  ME-5 media-anchoring module); M1's stateless-mode promise, the sipx transport-milestone
  attribution, an RFC 3263 §4.3→§4.4 miscitation, and the impossible "answered-then-cancelled →
  487" wording were corrected; blockers and upstream markers moved into board-visible `note:`
  fields, and the charts were rewired to match the designs.
