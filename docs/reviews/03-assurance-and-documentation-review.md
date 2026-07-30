# Assurance, proof, and documentation review

Review date: 2026-07-30

Review base: `86e6b10` (`main`, 83 commits ahead of `origin/main` when finalized)

This was an independent, adversarial repository review. It covered the vision and working agreement,
all normative specs and generated conformance data, the story board and a sample of story evidence,
the public site and README, CI and gate scripts, feature combinations, the Helm surface, and the Rust
runtime far enough to trace public claims to sockets. Version/release files changed in a coordinated
release while the review was running, so this report makes no version-string or release-process finding.

## Executive result

The repository has unusually good local decision-core tests and explicit deferred-vector accounting,
but several of its strongest public “today” claims stop at the sans-IO boundary. The real node does not
wire roles, CANCEL/Timer C, or dialog routing as the specs and site describe, and its advertised TCP
support is inbound-only. The proof system also has two important false-positive paths: vector coverage
does not establish that a function is a test, and the real-socket media assertion accepts zero sockets
while reporting exactly one.

| Severity | Finding |
|---|---|
| Critical | Configured roles do not control the runtime; `echo` and `e2e-tester` execute registrar/proxy paths |
| High | The shipped node drops CANCEL and Timer C effects that public docs mark “today” |
| High | Real-node ACK and mid-dialog routing re-query dialog targets as registered AoRs |
| High | TCP is accepted on a listener but outbound requests and Via are always UDP |
| Medium | The “independent implementation” interop proof shares the same kernel and has a fail-open socket assertion |
| Medium | The vector checker credits ordinary, non-running Rust functions as proofs |
| Medium | Public conformance counts and specification counts are stale |
| Medium | The loop-cookie HMAC key is derived from wall-clock time, not secret randomness |
| Low | Helm metadata and KO-2’s ledger prose describe states the current tree no longer has |

## Findings

### 1. Critical — roles are parsed and projected, then erased before runtime dispatch

The normative contract says a role selects the decision paths wired into a process and that an echo
process runs no proxy role (`docs/specs/cluster-config.md:116-122`). The public site repeats that promise
and specifically says an echo node cannot forward calls
(`website/docs/clustering/how-it-works.md:58-71`).

The implementation does not carry the role set into the driver:

- `ProjectedConfig` contains the `NodeIdentity`, including its role set
  (`crates/sipx-clstr-node/src/config/mod.rs:323-337`).
- `startup::node_config` accepts that projection, but the resulting `NodeConfig` has no role field
  (`crates/sipx-clstr-node/src/startup.rs:156-235`,
  `crates/sipx-clstr-node/src/driver.rs:34-84`).
- Runtime dispatch is consequently role-blind: every `REGISTER` enters the registrar, every `ACK` enters
  stateless forwarding, and every other method enters the proxy
  (`crates/sipx-clstr-node/src/driver.rs:795-808`).
- The node depends on `sipx-clstr-probe` (`crates/sipx-clstr-node/Cargo.toml:37-42`), but a repository-wide
  search finds no probe or echo engine use in the node runtime.

The ET-3 proof does not cover this direction. It verifies that the probe crate does not depend on the
proxy/node crates (`crates/sipx-clstr-probe/tests/role_separation.rs:1-44`), while the node is permitted
to depend on both and never dispatch to the echo. That is compatible with a green test and the broken
runtime. It also does not satisfy ET-3’s stronger checked acceptance statement that “the echo runs as
its own role/mode” (`docs/stories/ET-3-implement-the-echo-answering-endpoint.md:18-23`).

Impact: an `echo`-only process does not run the echo endpoint; it attempts to proxy calls and accepts
open-tenant registrations. An `e2e-tester` process does not run the probe engine. Role-separated Helm
workloads would therefore be labels around the same all-purpose runtime, defeating an explicit
security and architecture boundary.

Targeted regression proof: start the real binary on ephemeral listeners once for each role. A marked
INVITE to `echo` must be answered by `EchoEngine` with the marker, and a `REGISTER` must not mutate a
location store. An `edge`-only node must not run registrar behavior; a `registrar`-only node must not
proxy an INVITE. Also add a structural test that the driver configuration carries the projected role
set and maps each role to its engine. This wiring is cluster-specific and belongs here, not upstream.

### 2. High — CANCEL and Timer C are proved in the engine but discarded by the real node

The README calls forking, `CANCEL`, and Timer C working functionality (`README.md:169-177`), and the
public capability table labels all three “today” (`website/docs/intro.md:43-48`). The real driver does
not implement that contract:

- There is no `Method::Cancel` dispatch into an existing INVITE response context; `serve` creates the
  ordinary proxy path for it (`crates/sipx-clstr-node/src/driver.rs:795-808`).
- `CancelBranch` only logs that it is not wired to a socket
  (`crates/sipx-clstr-node/src/driver.rs:1213-1218`).
- `AnswerCancel`, `SetTimer`, `ClearTimer`, and `Terminate` are discarded. The adjacent comment states
  plainly that Timer C never fires in this driver
  (`crates/sipx-clstr-node/src/driver.rs:1219-1231`).

PX-10 itself records the caveat accurately (`docs/stories/PX-10-arm-the-timer-c-the-document-asks-for.md:73-88`),
but that truth did not reach the release-facing capability pages. PX-6’s completed acceptance proves
the proxy state machine and deterministic harness (`docs/stories/PX-6-implement-cancel-and-timer-c.md:15-25`),
not effect execution by the shipped node.

Impact: an upstream CANCEL cannot fan out to the live INVITE branches, losing branches are not
cancelled after a winning final response, and a branch quiet after a provisional is not reaped by
Timer C. Kernel transaction timeouts may eventually end some traffic, but that is not the specified
proxy behavior.

Targeted regression proof: a real-socket node test with two downstream UASes. Let both ring, have one
answer, and assert that the other observes a correctly formed CANCEL. In a second case send an
upstream CANCEL and assert immediate `200` to CANCEL plus `487` on the INVITE. In a virtual-clockable
driver test, fire the configured Timer C and assert cancellation/termination. Before implementation,
re-check whether CANCEL-to-INVITE transaction association belongs in `sipx`; executing the proxy
effects and scheduling the proxy-TU timer remain local driver work.

### 3. High — the real driver cannot follow a learned dialog route to a contact

The public call guide says `BYE`, re-INVITE, and other in-dialog requests are routed by the Route set
learned from Record-Route (`website/docs/guides/registrations-and-calls.md:60-70`). The engine correctly
pops this node’s Route and emits a `ResolveTargets` query for the remaining next hop or Request-URI
(`crates/sipx-clstr-proxy/src/context.rs:111-126`, `:152-169`). The node interprets every such query as
an address-of-record and looks it up in the registration store
(`crates/sipx-clstr-node/src/driver.rs:1189-1196`). A normal dialog Request-URI is the remote Contact,
not the AoR under which that Contact was registered, so the lookup is normally empty and the engine
returns `480`.

The special 2xx ACK path has the same defect more directly: it canonicalizes the ACK Request-URI as an
AoR and silently drops the ACK when the store has no matching binding
(`crates/sipx-clstr-node/src/driver.rs:1237-1256`). The simulator smoke test does not exercise the real
path: its stub caller constructs ACK and BYE to the peer’s AoR, and its stub edge performs its own
username lookup (`crates/sipx-clstr-sim/tests/smoke.rs:110-141`, `:282-293`).

Impact: the initial registered-user INVITE may succeed while its end-to-end ACK, BYE, re-INVITE, or a
Route-directed request fails or is dropped. This violates both SIP dialog routing and the project’s
“state rides the message” principle.

Targeted regression proof: use two real UAs against an ephemeral real node, preserve the Contact and
Route set learned from the `200`, and assert the callee receives the 2xx ACK and BYE and the caller
receives the BYE `200`. Include both loose and strict Route sets. The driver’s resolver response should
distinguish direct routed next hops from location-service AoR queries rather than applying one lookup
to both.

### 4. High — advertised TCP support does not survive outbound target selection

The public site calls UDP and TCP a current one-node capability
(`website/docs/guides/does-this-fit.md:39-49`, `website/docs/intro.md:43-48`). Binding a TCP listener is
implemented, but forwarding is not transport-aware:

- `destination_of` parses a Contact URI and then unconditionally returns `TransportKind::Udp`, ignoring
  `;transport=tcp` (`crates/sipx-clstr-node/src/driver.rs:1261-1273`).
- The proxy engine unconditionally pushes `Via: SIP/2.0/UDP`
  (`crates/sipx-clstr-proxy/src/forward.rs:132-141`).
- The existing listener test explicitly documents that every outbound branch still says UDP and then
  proves only that TCP was enabled on the inbound endpoint
  (`crates/sipx-clstr-node/tests/advertised_listeners.rs:167-173`, `:227-240`).

Impact: the node accepts TCP traffic but cannot call a TCP-only registered contact correctly, and its
top Via advertises the wrong transport for a branch that should leave over TCP. “TCP listener” is not
equivalent to end-to-end TCP proxy support.

Targeted regression proof: register a contact containing `;transport=tcp` from a TCP-only UAS, invite
it through the node, and assert that the downstream connection is TCP and the added Via begins
`SIP/2.0/TCP`. Add a direct `destination_of` UDP/TCP matrix. Check upstream first for a typed URI
transport/target-selection primitive; do not add a second transport-parameter parser here.

### 5. Medium — the interop proof is a useful process test, but not an independent parser proof

The site says the far end is an “independent implementation, not this one talking to itself”
(`website/docs/guides/registrations-and-calls.md:20-23`). The script goes further and says it proves
that this parser agrees with somebody else’s serializer (`scripts/e2e-call.sh:3-15`). CI actually
clones the exact sipx tag pinned by this workspace and builds “the kernel’s own phone” from it
(`.github/workflows/ci.yml:126-143`); the node itself pins `sipx-sip` and `sipx-transport` from that same
tag (`Cargo.toml:70-85`). Separate processes and real sockets are valuable, but parser, serializer,
transaction, and transport defects can be correlated across both ends. This is a same-kernel system
test, not an independent SIP implementation or external interoperability proof.

The media proof also fails open. The script counts matching UDP sockets and fails only when the count
is greater than one; zero passes and then prints “the node holds one socket”
(`scripts/e2e-call.sh:264-272`). The public guide promises an exact-one assertion
(`website/docs/guides/registrations-and-calls.md:76-84`). Missing `ss`, hidden PID information, or a
non-matching output shape can therefore produce a false pass.

Targeted regression proofs:

1. Keep this test, rename its claim to “same-kernel, real-process end to end,” and add a genuinely
   independent interop leg under CF-3 (for example SIPp vectors plus at least one independently
   implemented UA).
2. Make the socket check require exactly one and fail explicitly if `ss` is unavailable or its output
   cannot identify the node. Test the helper with injected `ss` output for zero, one, two, and an error.
3. Assert receipt and success of BYE, not merely that each phone reports an answered media interval.

### 6. Medium — vector coverage proves a function name, not that Cargo executes the function

The public methodology says coverage comes from test function names and that deleting a test deletes
the claim (`website/docs/reference/conformance.md:25-40`). The checker matches any Rust function
signature (`scripts/check-vectors.py:172-175`), scans every function in every `.rs` file, and credits a
matching name without inspecting `#[test]`, `#[ignore]`, or conditional compilation
(`scripts/check-vectors.py:505-544`).

This was reproduced without changing the repository: a temporary source containing only
`fn pb_v_999_not_a_test() { assert_eq!(1, 1); }` made `covered()` return `['PB-V-999']`. The function
would never be run by Cargo. An audit of the currently credited named functions found test attributes,
so this demonstrates a gate blind spot rather than asserting that one of today’s 125 proved rows is
already false.

Impact: deleting only `#[test]`, adding `#[ignore]`, or moving a proof behind a false `cfg` can leave
the conformance count and generated report green while no assertion executes. The methodology’s main
durability claim is therefore not enforced.

Targeted regression proof: add checker fixtures for a plain function, an ignored test, a disabled-cfg
test, and a normal test; only the last may count. Prefer intersecting discovered proof names with
`cargo test --workspace --all-features -- --list` (with a machine-readable approach if available), or
at minimum parse adjacent attributes and reject ignored/disabled proofs.

### 7. Medium — the public conformance headline has two obsolete denominators

The generated report currently says **125 of 549** rows proved, 19 shape-only, and 405 deferred
(`docs/reference/conformance.md:7-14`), matching `scripts/check-vectors.py --check`. The README badge and
status table still say **134 of 492**, with 358 deferred (`README.md:12-16`, `:169-176`). The public
conformance page wisely says it does not copy generated numbers because copies rot
(`website/docs/reference/conformance.md:83-90`), but the README is an unchecked copy.

There are now eleven normative spec files under `docs/specs/`, including
`sipx-cluster-crd.md`, and the checker registers its `SC` vectors. The README and public conformance
page still call the set “ten specifications” (`README.md:173-175`,
`website/docs/reference/conformance.md:92-99`).

Impact: the first quantitative assurance signal a visitor sees is false, and the public methodology
claims every spec is registered while omitting the newly registered CRD spec from its inventory.

Targeted regression proof: have `check-vectors.py --check` also verify every rendered copy of its
headline, or generate the README badge/table fragment from the same data. Derive the spec count and
inventory from `SPECS` rather than prose. Add a test that changes a fixture denominator and requires
all public copies to fail together.

### 8. Medium — a keyed loop cookie is instantiated with a predictable key

The proxy spec relies on the loop cookie being keyed so outsiders cannot forge it
(`docs/specs/proxy-behavior.md:126-135`), and the implementation correctly uses HMAC and treats the key
as redacted secret material (`crates/sipx-clstr-proxy/src/cookie.rs:52-58`). The real driver constructs
that key as the string `sipx-clstr/<nanoseconds since Unix epoch>`
(`crates/sipx-clstr-node/src/driver.rs:771-782`). A startup timestamp is not a cryptographic secret.

Impact: the stated 64-bit MAC forgery resistance assumes an unknown key; here an observer who can
bound startup time and obtain a cookie can test timestamp candidates offline. Exact feasibility
depends on timestamp uncertainty, but the construction does not meet the documented security
property. AF-6’s future distribution/rotation work does not make a predictable per-process key safe
today.

Targeted regression proof: obtain key bytes from an injected randomness/key source in the driver;
production uses an OS CSPRNG or a configured secret, deterministic tests use a fixed source. Test the
injection and redaction, not statistical randomness. The loop-cookie primitive remains generic, but
cluster key sourcing/distribution belongs here and should stay aligned with AF-6.

### 9. Low — chart and story metadata lag the just-landed CRD state

`Chart.yaml` says the chart installs the operator, CRDs, RBAC, and a working local environment
(`deploy/helm/Chart.yaml:3-9`). The only chart template is the `SipxCluster` custom resource; there is
no operator, CRD, or RBAC template (`deploy/helm/templates/sipxcluster.yaml:1-45`). The board correctly
keeps KO-2 blocked (`docs/stories/README.md:64-65`), but KO-2 still says KO-1 has not pinned the CRD and
that the API is provisional (`docs/stories/KO-2-ship-the-helm-chart-for-a-local-k3s-environment.md:26-57`)
while KO-1 is now done (`docs/stories/KO-1-specify-the-sipxcluster-crd-and-the-values-contract.md:1-20`).
The template comment repeats the obsolete “KO-1 has not pinned it” statement
(`deploy/helm/templates/sipxcluster.yaml:21-30`).

Impact: the generated board status is broadly honest, but `helm show chart`, the template, and the
blocked story disagree about what landed and what the package installs. That weakens the ledger as a
handoff mechanism and makes a skeleton look runnable outside the board context.

Targeted regression proof: until KO-2 closes, describe the artifact as a schema-valid skeleton that
renders an unserved custom resource. Refresh KO-2’s blocker to KO-3/ET-4. When KO-2 is implemented,
add a rendered-kind assertion for CRD, operator Deployment, RBAC, and exactly one `SipxCluster`.

## Gate observations and validations

The following completed successfully on the reviewed tree:

- `cargo test --workspace --all-features`
- `scripts/check-features.sh` — every current optional-feature combination builds
- `scripts/check-docs.sh`
- `scripts/check-vectors.py --check` — 125/549 proved, 19 shape-only, 405 deferred
- `scripts/check-proof-domains.py`
- `scripts/check-site.py`
- `deploy/helm/check-values.sh` — Helm lint plus rendered config load for all six roles

Those green results are meaningful: feature-off builds are covered, the generated vector report is
self-consistent, doc links and sidebar reachability are checked, documented CLI invocations are held
against the built binary, and the Helm values tree currently loads through the application schema.
The defects above survive because the checks prove narrower properties than their surrounding prose
claims.

Two already-recorded assurance debts were not counted again as novel findings:

- CF-18 already tracks done-story/CHANGELOG/unchecked-acceptance inconsistencies
  (`docs/stories/README.md:52-55`).
- KO-14 explicitly records that `deploy/helm/check-values.sh` is not part of `scripts/gate.sh`
  (`docs/stories/KO-14-bring-the-chart-to-the-config-schema.md:47-60`). The script passed when run
  directly, but the omission remains a CI blind spot.

No commit was created, and no repository file outside this review was intentionally modified.
