# Validated synthesis of the independent adversarial reviews

**Review date:** 2026-07-30

**Reviewed revision:** `86e6b10` (`v0.12.0`)

**Coordinator:** primary review agent; this synthesis was written only after the three independent
passes were complete

## Inputs and independence

The three source reviews were assigned different lenses and were not allowed to read one another's
work before filing their reports:

- [Protocol and state-machine review](01-protocol-and-state-review.md) — SIP semantics, proxy and
  registrar state, and the sans-IO/driver boundary.
- [Security and operations review](02-security-and-operations-review.md) — hostile ingress,
  configuration, resource bounds, durable storage, packaging, and supply-chain posture.
- [Assurance, proof, and documentation review](03-assurance-and-documentation-review.md) — whether
  tests, gates, stories, and public claims prove the behavior attributed to them.

This document is not a vote count. I read all three reports after they were complete, traced every
claim retained below back through the current source/specification/documentation, compared overlaps,
and required one of these validation bases:

1. a focused runtime or isolated state-machine reproduction on `86e6b10` plus a source trace;
2. a deterministic source path whose branches make the claimed result unavoidable, plus confirmation
   that no test or driver layer supplies the missing behavior; or
3. for assurance/documentation findings, a direct execution of the relevant checker and comparison
   of generated truth with the published claim.

The protocol reviewer reproduced the lower-q fork revival and stale-index REGISTER defects in
isolated executables. The security/operations reviewer reproduced role bypass, TCP-only UDP exposure,
and discarded `cluster.security` policy against the real binary. I separately exercised the vector
coverage classifier with a virtual Rust source and confirmed that it credited a plain, non-test
function named `pb_v_999_plain_helper_is_not_a_test`.

## Executive conclusion

The repository's strongest component is its pure decision cores. Its dominant failure mode is that
the real node does not preserve or perform the facts and effects those cores produce. Five findings
are release blockers for the current “working/today” claims:

1. configured roles disappear before runtime dispatch;
2. matched CANCEL, branch cancellation, and Timer C are not wired to the real driver;
3. ACK and in-dialog routing do not follow SIP dialog next hops;
4. a terminal fork result can revive a never-launched lower-priority target; and
5. a multi-contact REGISTER can commit the wrong binding set through stale indices.

There is no evidence of `unsafe` use, a network-input panic, direct credential disclosure, or a
currently exploitable memory-safety defect. The release risk is instead protocol failure, fail-open
configuration, false-success durable-state behavior, and proof claims that stop short of the socket
boundary. The current binary should not be represented as a production-ready clustered SIP edge on
an untrusted network until the release blockers and the high-priority ingress/store issues below are
closed or the public capability matrix is narrowed.

## Story traceability

Each validated finding has exactly one primary delivery story. Supporting stories own a distinct
dependency or proof and do not duplicate the primary acceptance contract.

| Finding | Primary story | Supporting story(s) |
|---|---|---|
| V-01 | `DP-13` | — |
| V-02 | `PX-12` | `CX-7` |
| V-03 | `PX-13` | `ET-7` |
| V-04 | `PX-14` | — |
| V-05 | `RG-16` | — |
| V-06 | `FC-6` | — |
| V-07 | `FC-1` | `CX-7` |
| V-08 | `RG-17` | — |
| V-09 | `RG-18` | — |
| V-10 | `RG-19` | — |
| V-11 | `DP-14` | — |
| V-12 | `RT-12` | — |
| V-13 | `RG-20` | `CX-7` (kernel re-read; no new gap filed) |
| V-14 | `RG-21` | — |
| V-15 | `PX-15` | `AF-6` (cross-link) |
| V-16 | `CF-20` | — |
| V-17 | `CF-3` | — |
| V-18 | `DX-14` | — |
| V-19 | `KO-16` | — |
| V-20 | `DP-15` | — |

## Validated release blockers

### V-01 — Configured roles do not control runtime behavior

**Claim.** Projection uses the role set to select listeners and the location store, then drops the
roles before constructing `NodeConfig`. The one dispatcher consequently sends every REGISTER to the
registrar, every ACK to one stateless path, and every other method to the proxy. Probe and echo roles
are linked but are not selected by runtime dispatch.

**Evidence.** `ProjectedConfig.identity` carries the roles at
`crates/sipx-clstr-node/src/config/mod.rs:323-337`; `startup::node_config` does not transfer them at
`crates/sipx-clstr-node/src/startup.rs:156-235`; `NodeConfig` has no role/capability field at
`crates/sipx-clstr-node/src/driver.rs:34-84`; and dispatch is unconditional at `driver.rs:795-808`.
This conflicts with cluster-config R3 (`docs/specs/cluster-config.md:112-122`).

**Validation.** Two reviewers independently found the source path. A real binary started as
`inbound-proxy` accepted and stored a REGISTER with `200 OK`. The structural trace also establishes
the echo/e2e-tester failure without relying on that single runtime case.

**Disposition.** Verified, release-blocking. Role separation is a platform wiring boundary and
belongs here, not upstream.

### V-02 — The real driver does not implement matched CANCEL or Timer C

**Claim.** The proxy core models `UpstreamCancelled`, `AnswerCancel`, `CancelBranch`, and timer
effects, but the node creates an independent proxy context for a CANCEL, merely logs
`CancelBranch`, and discards `AnswerCancel`, `SetTimer`, `ClearTimer`, and `Terminate`.

**Evidence.** Core inputs/effects are handled at
`crates/sipx-clstr-proxy/src/context.rs:379-439`. The driver dispatch and discarded effects are at
`crates/sipx-clstr-node/src/driver.rs:795-808` and `:1213-1231`; its own comment states that Timer C
never fires. `docs/stories/PX-10-arm-the-timer-c-the-document-asks-for.md:73-88` records the caveat,
while `README.md:173` and `website/docs/intro.md:45` advertise the behavior as current.

**Validation.** All three reviews independently converged on this gap. The effect-discard match arm
is definitive; passing proxy vector tests prove only effect production, not execution.

**Disposition.** Verified, release-blocking. CANCEL-to-transaction association should first be
checked against the kernel boundary; scheduling proxy-TU Timer C and performing the existing effects
are local driver work.

### V-03 — ACK and in-dialog requests are routed as registrar lookups

**Claim.** Every ACK is sent through a path that ignores Route, treats the Request-URI as an AoR,
selects the first registration, and silently drops the request if no binding exists. Other
in-dialog methods are preprocessed correctly by the core, but the driver again treats their direct
next-hop URI as an AoR lookup. A normal remote Contact is not the registered AoR.

**Evidence.** `crates/sipx-clstr-node/src/driver.rs:800-807`, `:1189-1196`, and `:1237-1257`; route
preprocessing and next-hop selection at `crates/sipx-clstr-proxy/src/context.rs:111-169`. The probe
constructs ACK/BYE to a configured AoR rather than retaining the dialog remote target and route set
at `crates/sipx-clstr-probe/src/engine.rs:446-455` and `:635-658`.

**Validation.** The protocol and assurance reviews found the same end-to-end break independently.
The source trace is total for the real node: no alternate direct-resolution path exists. The passing
simulation uses AoR-shaped ACK/BYE and therefore cannot falsify the claim.

**Disposition.** Verified, release-blocking. Generic URI resolution and ACK transaction semantics
must be considered upstream first; choosing location lookup versus direct route/flow delivery is
cluster orchestration.

### V-04 — A final response or cancellation can revive a lower-q fork group

**Claim.** A 2xx, 6xx, or upstream cancellation cancels launched branches but leaves never-launched
targets in `queued`. When a cancelling branch later settles, the generic final-response path sees
the queue and calls `fork_next_group`, originating a new INVITE after the transaction was accepted,
globally rejected, or cancelled.

**Evidence.** Queue retention and group draining are at
`crates/sipx-clstr-proxy/src/context.rs:193-242`; terminal response handling at `:342-374`;
upstream cancellation at `:379-411`; and finish logic at `:631-647`.

**Validation.** The protocol reviewer reproduced the composition with q=1.0 targets A/B and q=0.5
target C: A's 200 emitted `[Respond, CancelBranch]`; B's subsequent 487 emitted `[Forward,
SetTimer]` for C. I re-traced that exact transition through the state machine. Existing vectors test
sequential grouping and terminal results separately, not this cross-product.

**Disposition.** Verified, release-blocking. This is protocol-generic proxy state-machine behavior
and should be fixed in sipx if the kernel owns the corresponding transaction policy; the current
state machine is local, so the immediate defect is here and must not be shadow-fixed elsewhere.

### V-05 — Multi-contact REGISTER reconciliation uses stale indices

**Claim.** REGISTER processing parses the original binding vector once, looks up every operation in
that immutable parallel vector, then mutates a separate `BindingSet` by the old index. Removing an
early binding shifts later indices; later operations can be skipped or applied to the wrong slot.

**Evidence.** `crates/sipx-clstr-registrar/src/process.rs:132-150`, `:184-210`, and `:230-235`.

**Validation.** The protocol reviewer reproduced current bindings `{A, B}` with one REGISTER that
removes A and B; the returned commit still contained B. I independently traced the first removal
shifting B from index 1 to 0 and the second operation taking the `get(1) == None` continue branch.

**Disposition.** Verified, release-blocking. Binding reconciliation is the platform's location
service contract and belongs here; URI equivalence remains a kernel primitive.

## Validated high-priority findings

### V-06 — `cluster.security` is accepted, discarded, and not reported as unapplied

`read_security` recognizes four keys, validates none of their values, and returns the default struct,
which contains only fixed Max-Forwards (`crates/sipx-clstr-node/src/config/mod.rs:232-245` and
`:1444-1482`). These paths are not added to `Config.unapplied`, and no security policy reaches the
driver. A real run with source-drop, sanity-check, internal-zone, and a matching User-Agent deny rule
still returned `200 OK` and emitted no unapplied warning. This is **verified high** and contradicts
`website/docs/reference/configuration.md:172-185`. The ingress policy is local orchestration; reusable
SIP sanity primitives should be considered upstream.

### V-07 — A TCP-only declaration also exposes a working UDP service

`Listeners::endpoint_config` creates the kernel's cleartext endpoint from the TCP listener and merely
enables TCP (`crates/sipx-clstr-node/src/listen.rs:381-409`). The driver explicitly serves an arrival
whose transport was not declared (`crates/sipx-clstr-node/src/driver.rs:562-570`). A real TCP-only
node answered a UDP REGISTER with `200 OK`. This is **verified high** because the accepted network
exposure is broader than the configured one. If the kernel cannot bind TCP without UDP, that endpoint
capability belongs upstream; the local loader must refuse the unsupported declaration meanwhile.

### V-08 — PostgreSQL read/decode errors become successful absence

`PostgresStore::read` maps every database/decode error to an empty set at revision zero
(`crates/sipx-clstr-node/src/postgres_store.rs:227-240`). `apply` returns Noop/Reject without a commit
or CAS (`crates/sipx-clstr-registrar/src/store.rs:95-113`), so REGISTER queries and no-op removals can
return success while durable state remains unknown. Lookup uses the same conversion and turns an
outage into no targets (`postgres_store.rs:265-270`). This is **verified high** against the source
contract; no live fault injection was available. A fallible store/lookup interface is platform
state orchestration and belongs here.

### V-09 — Registrar domain and principal authorization gates are incomplete

The specified principal-to-AoR authorization step has no policy input or implementation, while
`RegisterCommand.principal` is assumed to be “already authorized.” The node's served-domain check
uses the To-derived AoR rather than the REGISTER Request-URI and returns 403 rather than the S1 404
(`crates/sipx-clstr-node/src/driver.rs:993-1013`; `docs/specs/location-service.md:205-220`). This is
**verified high but partly latent**: the missing S4 check becomes directly exploitable when a real
credential source is enabled; the wrong-authority S1 check is present now. Principal authorization is
tenant/platform policy here; generic auth and URI-authority primitives remain upstream concerns.

### V-10 — The wire response drops registrar facts required by the contract

The core retains contact q, Path, bad extension tags, the minimum expiry, and the required extension
name (`crates/sipx-clstr-registrar/src/command.rs:97-170`, `:215-240`). The node renders only status
and `<contact>;expires=N` (`crates/sipx-clstr-node/src/driver.rs:1015-1038`). It therefore omits q,
Path, `Supported: path`, `Unsupported`, and `Min-Expires` on the corresponding responses. This is
**verified high**. Header syntax/building is kernel territory; mapping the local registrar outcome
onto those headers is local driver work.

### V-11 — The advertised admission bound does not bound REGISTER or refusal tasks

REGISTER and ACK are exempt, each exempt arrival is spawned, and each over-limit request also gets a
new response task (`crates/sipx-clstr-node/src/driver.rs:306-319` and `:524-588`). REGISTER logs an
authentication outcome per request. With PostgreSQL, requests can accumulate behind synchronous
`block_in_place` calls and one client mutex. This is **verified high as an unbounded-work design
defect**; no claim is made here about the exact request rate needed to exhaust a particular runtime.
Admission and logging policy are local; the already-ledgered kernel per-message overload logging
remains an upstream item.

### V-12 — Outbound transport selection is UDP-only and target failures are not settled

The forwarding core stamps `SIP/2.0/UDP`; the node ignores a URI transport parameter, refuses hostname
contacts, and constructs only UDP targets (`crates/sipx-clstr-proxy/src/forward.rs:132-141` and
`crates/sipx-clstr-node/src/driver.rs:1261-1273`). If target materialization fails, `perform` logs and
continues without sending `BranchTransportError` back to the state machine (`driver.rs:1198-1208`).
This is **verified high**: TCP is inbound-only, and a valid hostname target can produce no upstream
final. Resolution/transport selection is protocol-generic and must be pursued upstream; feeding a
driver failure back into the local context is local work.

## Validated medium and low findings

### V-13 — Malformed registration parameters are treated as absence

A present non-numeric Expires header or Contact `expires` parameter becomes `None`, and malformed
Path values are dropped by `filter_map` (`crates/sipx-clstr-registrar/src/parse.rs:179-184`,
`:241-250`, and `:306-314`). The resulting request can use a fallback lifetime or commit a partial
route set. **Verified medium.** Parsing behavior is protocol-generic and should be fixed upstream if
the necessary typed parse result belongs in sipx; local admission must not silently reinterpret it.

### V-14 — PostgreSQL mode is a serialized, cleartext proof backend

The backend uses one `Mutex<Client>`, the synchronous adapter enters `block_in_place`, connection
creation uses `NoTls`, and there is no pool/reconnect/deadline mechanism
(`crates/sipx-clstr-node/src/postgres_store.rs:65-76`, `:96-123`, and
`crates/sipx-clstr-node/src/blocking_store.rs:52-73`). **Verified medium operational limitation.**
The security review rated the single-client shape High; this synthesis lowers it because the source
explicitly calls it a contract proof and no measured outage curve was produced. The read-error
correctness defect remains separately High in V-08. Database operation is local driver work.

### V-15 — The loop-cookie key is predictable startup-time text

The HMAC primitive is sound, but the driver supplies `sipx-clstr/<Unix epoch nanoseconds>` as its
entire key (`crates/sipx-clstr-node/src/driver.rs:771-782`). **Verified medium.** This violates the
documented unforgeable-key assumption; exploit cost depends on how narrowly an observer can bound
startup time, so no stronger exploitability claim is retained. Key sourcing/distribution is local;
the HMAC primitive remains upstream-capable.

### V-16 — The proof system has two false-positive paths

`scripts/check-vectors.py` recognizes any Rust function name matching a vector and never verifies
`#[test]`, `#[ignore]`, or whether Cargo lists/runs it (`scripts/check-vectors.py:505-544`). The
coordinator reproduction confirmed a plain function is credited. Separately, the real-socket script
fails only when the node's UDP socket count is greater than one, so zero sockets passes and prints
“one socket” (`scripts/e2e-call.sh:264-272`). **Verified medium.** The current credited vector
functions were audited as tests, so this is a gate weakness rather than proof that today's 125
credited rows are fabricated. Test-discovery machinery may belong in the kernel testkit if reusable;
these repository-specific report and shell checks belong here.

### V-17 — The real-socket test is same-kernel, not independent-parser interop

CI builds the `sipx` CLI from the same sipx tag whose libraries the node uses
(`.github/workflows/ci.yml:126-143`; `Cargo.toml:70-85`). Separate processes and real sockets are
valuable, but correlated parser/serializer/transaction defects remain possible. The script also does
not prove compliant dialog ACK/BYE behavior, as V-03 explains. **Verified medium, with qualification:**
the test remains a useful same-kernel process integration test; only the “independent implementation”
and independent-parser claims are rejected. A genuinely independent interop target is a valid named
integration target under the provenance carve-out.

### V-18 — Release-facing counts and capability statements are stale

The generated report says 125/549 proved, 19 shape-only, 405 deferred, while README badges/table say
134/492 and 358 deferred. There are eleven normative specs, while README/site prose says ten
(`docs/reference/conformance.md:5-14`; `README.md:12-16`, `:173-175`). More importantly, public
“today” rows claim CANCEL, Timer C, dialog routing, TCP, role separation, and independent interop more
broadly than the real driver proves. **Verified medium for release integrity**; the protocol defects
retain their higher severities above. Documentation generation/checking belongs here.

### V-19 — Packaging metadata overstates deployability

`deploy/helm/Chart.yaml:3-9` says the chart installs an operator, CRDs, RBAC, and a working local
environment, but its only template is `deploy/helm/templates/sipxcluster.yaml`; KO-2 remains blocked.
Its story/template also still say KO-1 has not pinned the CRD although KO-1 is done. **Verified low.**
The chart itself accurately produces only the custom resource, so this is metadata/ledger drift, not
a hidden controller implementation. Kubernetes packaging is local platform work.

### V-20 — Release artifact hardening is incomplete

The documented default Docker build selects `CARGO_PROFILE=dev` and copies the debug binary
(`Dockerfile:34-52`). The e2e CLI is checked out by tag rather than the exact Cargo.lock commit, and
GitHub actions/base images use mutable version tags (`.github/workflows/ci.yml:25-34`, `:126-143`).
**Verified low-to-medium hardening debt.** The reviews did not measure a throughput regression or
observe a moved tag/compromised action, so those stronger consequences are not retained as active
failures. Artifact and CI policy belong here.

## Claim disposition and severity reconciliation

Every factual finding in the three reports is represented above, either directly or consolidated
with its duplicates. I made these material qualifications:

- The security review's single blocking PostgreSQL client finding is retained as Medium operational
  limitation, while its error-to-empty behavior remains a separate High correctness defect.
- Predictable loop-key exploitation is plausible but not timed; the invalid key construction is
  verified, not a claimed practical forgery rate.
- Mutable tags/actions and a dev-profile image are verified hardening gaps, not evidence that a
  dependency has already moved or that production capacity has already failed.
- The same-kernel e2e run is not discarded: its real-process/socket/media evidence remains valid.
  Only its independent-implementation/parser and exact-one-socket claims fail validation.
- The missing principal-to-AoR gate is explicitly marked latent until a real credential source is
  enabled; the Request-URI/To-domain mismatch exists in today's open registrar.

No report claim was rejected as factually false after those scope and severity qualifications.

## Recommended order of work

1. Correct the public capability matrix immediately so it describes the real driver rather than the
   pure engines.
2. Carry roles/capabilities into runtime dispatch and refuse all security/network declarations the
   process cannot enforce.
3. Repair CANCEL/timer execution and ACK/dialog routing, then prove them over real sockets with
   compliant remote-target and Route-set behavior.
4. Fix fork terminality and stable binding reconciliation with failing-first cross-product vectors.
5. Make store reads fallible, render every registrar response fact, and settle every unroutable
   branch with an explicit state-machine input.
6. Bound total registrar/refusal work and replace the proof-only PostgreSQL path before calling it an
   HA production data plane.
7. Close the proof-system and release-metadata gaps so future capability claims cannot outrun the
   executable evidence again.

These should be filed as stories before implementation. Each story/design must record the upstream
decision stated above rather than reimplement generic SIP parsing, resolution, transaction, or
testkit behavior locally.

## Validation record

The review set was exercised with:

- `scripts/gate.sh` on the reviewed tree;
- `cargo audit` over the locked dependency graph;
- `deploy/helm/check-values.sh` for all six role renderings;
- `scripts/e2e-call.sh` with a real local `sipx` CLI, interpreted only within the narrowed proof scope;
- focused all-feature proxy, registrar, and node test suites;
- real-binary UDP reproductions for V-01, V-06, and V-07;
- isolated state-machine reproductions for V-04 and V-05; and
- a direct in-memory exercise of `scripts/check-vectors.py` for V-16.

Green gates do not contradict these findings: most are missing compositions or driver effects outside
the unit/vector boundary, and V-16 identifies a property the gate does not currently inspect.
