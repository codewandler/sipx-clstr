# Security and operations adversarial review

**Review date:** 2026-07-30

**Reviewed revision:** `86e6b10` (`sipx-clstr` 0.12.0; sipx `v0.10.0`, lockfile commit
`f9104b78a19073ccf3d5fffddade4dda0d2ac401`)

**Reviewer:** independent security/operations pass; no other review report was consulted

## Scope and method

This pass treated the public SIP socket, the cluster document, the location-store boundary, and the
build/deployment pipeline as hostile interfaces. It inspected the Rust workspace, executable driver,
configuration projection, PostgreSQL backend, Docker image, devspace and Helm assets, GitHub Actions,
tests, specifications, stories, and public documentation. The pure proxy and registrar engines were
checked as well, but the emphasis was whether their decisions survive contact with the real driver.

Three findings were also reproduced against `target/debug/sipx-clstr` on loopback with complete SIP
`REGISTER` datagrams:

1. an `inbound-proxy`-only node registered a contact and returned `200 OK`;
2. a document declaring only a TCP listener accepted a UDP registration and returned `200 OK`;
3. a document declaring `unknownSource: drop`, a User-Agent deny list containing the presented
   User-Agent, and an internal zone not containing the client still returned `200 OK` with no
   unapplied-configuration warning.

The focused Rust suites were green:

```text
cargo test -p sipx-clstr-node --all-features
cargo test -p sipx-clstr-proxy -p sipx-clstr-registrar --all-features
```

That does not contradict the findings. Several tests prove the sans-IO effect stream, not the real
driver that discards effects; the listener tests inspect projected values, not the additional UDP
socket the kernel opens; and PostgreSQL tests explicitly skip without
`SIPX_CLSTR_TEST_DATABASE_URL` (`crates/sipx-clstr-node/tests/postgres_store.rs:30-50`).

## Executive assessment

No critical, memory-safety, or direct credential-disclosure defect was found. The process does,
however, have six high-severity fail-open or availability defects at its real I/O boundary:
security policy and roles are not enforced, a TCP-only declaration exposes UDP, core CANCEL/Timer C
effects never happen, admission does not bound all spawned work, and the advertised shared store is
a single blocking connection without failure containment. These defects make the running product
materially less constrained and less reliable than its configuration and public documentation say.

The most important release posture is therefore: do not expose the current binary directly to an
untrusted network, do not use role separation as a security boundary, and do not treat its
PostgreSQL mode as a production-ready HA data plane.

## Findings

### SEC-01 — High — `cluster.security` is accepted, discarded, and reported as applied

**Impact.** An operator can declare source dropping, sanity checking, a User-Agent deny list, and an
internal network boundary, receive a successful startup, and get none of those controls. This is a
fail-open configuration boundary: the runtime serves traffic the operator explicitly intended to
reject. Because no unapplied warning is emitted, monitoring cannot distinguish this posture from an
enforced one.

**Evidence.** `SecuritySpec` contains only the fixed Max-Forwards value
(`crates/sipx-clstr-node/src/config/mod.rs:232-245`). `read_security` accepts the four policy key
names, validates none of their values, and unconditionally returns `SecuritySpec::default()`
(`crates/sipx-clstr-node/src/config/mod.rs:1444-1482`). `security` is a normal cluster key rather
than a deferred section (`crates/sipx-clstr-node/src/config/mod.rs:340-372`), so it is absent from the
computed `unapplied` paths (`crates/sipx-clstr-node/src/config/mod.rs:840-860`). The projected value
never reaches `NodeConfig`; `startup::node_config` applies tenant, auth, store, admission, and Timer C
but no security policy (`crates/sipx-clstr-node/src/startup.rs:188-235`).

This directly contradicts the repository's fail-closed rule—“Accepted means applied, or refused”
(`docs/designs/fail-closed-config.md:8-22`)—and the public table that calls `security` “validated and
applied” (`website/docs/reference/configuration.md:172-185`).

**Runtime validation.** A valid UDP registration with `User-Agent: evil-phone` received `200 OK`
under this accepted block:

```yaml
security:
  unknownSource: drop
  sanityCheck: true
  userAgentDenyList: [evil-phone]
  internalZone: {networks: [10.0.0.0/8]}
```

Startup emitted no `does NOT apply` or `SECURITY:` record for this section.

**Recommendation.** Until every field has a decision path and executable negative tests, refuse a
non-empty `cluster.security` block at load and list it as security-relevant unapplied configuration.
Add a real-binary test for each field that asserts the rejected packet and the startup posture, not
only parser acceptance. This is cluster ingress orchestration, so it belongs here; any generic SIP
sanity primitives it needs should be proposed upstream first.

### SEC-02 — High — roles constrain listener selection but not the service behind a listener

**Impact.** An `inbound-proxy`, `outbound-proxy`, `e2e-tester`, or `echo` identity still runs the
normal registrar/proxy dispatcher. In particular, a proxy-only public listener is also an open
registrar. Because projection removes `locationStore` from a non-registrar identity, those
registrations silently land in a process-local store. Role-based network policy, blast-radius
separation, and independent scaling are therefore false security and operations boundaries.

**Evidence.** Projection filters listeners by role and removes the location store unless the identity
contains `registrar` (`crates/sipx-clstr-node/src/config/mod.rs:444-470`). The role set is then lost:
`NodeConfig` has no role field (`crates/sipx-clstr-node/src/driver.rs:39-84`) and
`startup::node_config` does not wire role-specific handlers (`crates/sipx-clstr-node/src/startup.rs:156-235`).
The one runtime dispatcher always routes `REGISTER` to the registrar, `ACK` statelessly, and every
other method to the proxy (`crates/sipx-clstr-node/src/driver.rs:795-808`).

This violates cluster-config R3, which says a role selects which decision paths are wired
(`docs/specs/cluster-config.md:112-122`), and the same public promise in
`website/docs/reference/cli.md:50-58` and `website/docs/operate/deploy.md:89-102`.

**Runtime validation.** A node started with identity `--roles inbound-proxy`, a listener assigned only
to `inbound-proxy`, and an ordinary tenant answered a UDP `REGISTER` with `200 OK`.

**Recommendation.** Carry a capability/handler set—not a role conditional—into the driver and build
the method dispatcher from the projected roles. Refuse startup if a listener would expose a method
with no wired service. Add a negative matrix test over all six roles and at least `REGISTER`,
`INVITE`, and probe/echo behavior.

### NET-01 — High — a TCP-only listener silently exposes and serves UDP

**Impact.** Network exposure is broader than the accepted configuration. A deployment that permits
only TCP to obtain connection-oriented behavior, policing, or firewall semantics still has a live
UDP SIP service on the same address. The node processes the packet rather than dropping it, so an
undeclared transport is an alternate ingress path into all request handlers.

**Evidence.** `Listeners::cleartext` chooses TCP when UDP was not declared, then
`endpoint_config` constructs the kernel endpoint with `sipx_transport::Config::new(bind)` and merely
sets the optional TCP flag (`crates/sipx-clstr-node/src/listen.rs:381-409`). When the resulting
endpoint reports an arrival on an undeclared transport, the driver explicitly warns and continues
serving it (`crates/sipx-clstr-node/src/driver.rs:562-570`). The dependency is immutably locked, but
the integration test suite never asserts that an undeclared protocol has no socket.

The public contract says an unservable transport is refused and never downgraded
(`website/docs/reference/configuration.md:66-78`), while the product table advertises UDP and TCP as
implemented (`website/docs/intro.md:43-49`).

**Runtime validation.** With the only listener declared as `transport: tcp` on
`127.0.0.1:25062`, a UDP `REGISTER` received `200 OK`; stderr recorded
`request arrived on an undeclared transport transport="UDP"`.

**Recommendation.** Make socket creation match the declaration exactly. If the current kernel
cannot disable UDP, refuse TCP-only documents until that generic endpoint capability is added
upstream. Treat an arrival on an undeclared transport as an invariant violation to be dropped and
health-signaled, never as a request to serve from a fallback listener. Add black-box socket tests in
both directions.

### CALL-01 — High — the real driver drops CANCEL and Timer C effects

**Impact.** The pure proxy produces correct cancellation and timer decisions, but real calls do not
perform them. A matching upstream CANCEL is not answered through `AnswerCancel`, losing branches are
not sent CANCEL, and Timer C is never scheduled. Ringing or silent branches can remain live until a
different kernel timeout—or indefinitely if none applies—holding transaction tasks and admission
permits and continuing to ring endpoints after the caller has cancelled.

**Evidence.** The driver logs `CancelBranch` without sending anything and discards `AnswerCancel`,
`SetTimer`, `ClearTimer`, and `Terminate` (`crates/sipx-clstr-node/src/driver.rs:1213-1231`). Its own
comment states that Timer C is armed with the configured value and never fires. This is the exact
opposite of normative rules C1-C6 (`docs/specs/proxy-behavior.md:230-239`). The public product status
still says forking, CANCEL, and Timer C work today (`README.md:169-176` and
`website/docs/intro.md:43-49`).

**Validation boundary.** `timer_c_armed` and the proxy vector suite pass because they assert the
emitted effect. They do not prove a real clock fires it or a socket transmits CANCEL. The source
discard arm is definitive runtime evidence.

**Recommendation.** Remove CANCEL and Timer C from “working” documentation until the driver owns a
timer registry and cancellable branch transactions. Add real-socket tests that observe `200` to an
upstream CANCEL, a downstream CANCEL with the INVITE branch, a Timer C firing after provisional
response, and permit release after termination.

### DOS-01 — High — admission does not bound spawned response or registration work

**Impact.** The configured ceiling bounds only selected proxy transactions, not process work. Every
over-limit request creates a response task, while every `REGISTER` is exempt and creates a full
handler task. Each registration also emits an unconditional info/warn authentication record. A
remote registration flood can therefore grow runnable or blocking task count and log volume without
crossing the advertised admission gauge. With PostgreSQL enabled, those tasks can all block behind
one database mutex, magnifying the failure into runtime thread and memory exhaustion.

**Evidence.** `REGISTER` and `ACK` are explicitly exempt (`crates/sipx-clstr-node/src/driver.rs:306-319`).
The accept loop spawns a task even for each refused request (`crates/sipx-clstr-node/src/driver.rs:524-545`)
and one for every admitted or exempt arrival (`crates/sipx-clstr-node/src/driver.rs:550-587`). Every
REGISTER unconditionally calls `record_authentication` (`crates/sipx-clstr-node/src/driver.rs:930-949`),
whose ordinary open-tenant outcome logs at info (`crates/sipx-clstr-node/src/driver.rs:836-886`). This
contrasts with the carefully sampled overload path, whose comments correctly identify per-message
logging as a cost multiplier (`crates/sipx-clstr-node/src/driver.rs:640-656`). The existing flood
test offers INVITEs and verifies forwarded/refused counts; it does not bound spawned refusal tasks or
REGISTER work (`crates/sipx-clstr-node/tests/admission_bound.rs:250-320`).

**Recommendation.** Add a separate, cheap total-work/task bound and a bounded response executor;
preserve protocol-specific admission semantics inside it. Rate-limit registration work by source and
tenant before database access, and aggregate repetitive authentication outcomes without losing the
audit facts. Prove a REGISTER flood and a deliberately stalled response transport keep task,
blocking-thread, memory, and log growth within measured limits.

### DB-01 — High — PostgreSQL mode is a single blocking proof connection on the SIP hot path

**Impact.** Every AoR on a node serializes through one synchronous client, so one slow query or lost
connection stalls unrelated registrations and call lookups. There is no pool, query deadline, circuit
breaker, or reconnect path. Because the synchronous adapter enters `block_in_place` before acquiring
the store's client mutex, concurrent request tasks may consume blocking capacity merely waiting for
the one connection. A database incident can therefore exhaust node resources and make otherwise
healthy SIP sockets unresponsive.

**Evidence.** The implementation explicitly says a real deployment needs a pool and that the one
connection only proves the contract; `PostgresStore` contains `Mutex<Client>`
(`crates/sipx-clstr-node/src/postgres_store.rs:65-76`). Every location-store operation uses
`tokio::task::block_in_place` (`crates/sipx-clstr-node/src/blocking_store.rs:1-26,52-73`). The only
connection is created at startup (`crates/sipx-clstr-node/src/postgres_store.rs:96-123`) and no code
replaces it after failure. A read failure is converted into an empty binding set
(`crates/sipx-clstr-node/src/postgres_store.rs:227-270`), so database failure becomes false
unreachability on lookup rather than an explicit service-unavailable outcome.

This backend is nevertheless presented as the current shared location service that makes two nodes
one registrar (`website/docs/intro.md:37-51`, `website/docs/reference/configuration.md:94-101`). The
database tests cover consistency and a serial storm, but not a concurrent workload, query stall,
connection loss, recovery, or async scheduler health.

**Recommendation.** Replace the synchronous trait seam with an async driver and bounded pool, set
connect/acquire/query deadlines, and implement reconnect with backoff and readiness degradation.
Propagate lookup failure distinctly so the proxy can return a temporary failure rather than false
absence. Do not describe PostgreSQL mode as operationally shared/HA until fault-injection tests prove
isolation between AoRs, bounded behavior during a stall, and recovery after connection loss.

### NET-02 — Medium — registered TCP contacts are always targeted over UDP

**Impact.** TCP registration does not imply TCP reachability for subsequent requests. A contact with
`;transport=tcp` is parsed for host and port and then sent to a new UDP destination. Client-initiated
flow ownership is also discarded at the registrar-to-proxy bridge. Calls to TCP-only clients, and
especially clients behind NAT reachable only over their accepted connection, fail or leak onto the
wrong transport.

**Evidence.** `destination_of` always returns `TransportKind::Udp`, ignoring URI transport parameters
(`crates/sipx-clstr-node/src/driver.rs:1261-1273`). Registrar targets carry an opaque `flow_ref`
(`crates/sipx-clstr-registrar/src/lookup.rs:19-31,52-60`), but `targets_from_lookup` copies only
contact, route set, and q (`crates/sipx-clstr-proxy/src/from_registrar.rs:16-30`). The public guide
correctly admits that the Kubernetes proof does not test TCP (`website/docs/guides/docker-and-k3d.md:89-100`),
but the top-level product table calls UDP and TCP current.

**Recommendation.** Until flow ownership is implemented, either refuse TCP listeners/registrations
or state that TCP is ingress-only and unsupported for registered reachability. Preserve transport and
flow reference through route planning, then add a real TCP registration-and-call test that asserts
reuse of the accepted flow where required.

### DB-02 — Medium — PostgreSQL transport is hard-coded to `NoTls`

**Impact.** Registration records and their routing metadata cannot be protected in transit to the
database, and a database that requires TLS cannot be used. The stored JSON includes contacts, observed
source, Path, principal, instance/reg-id, flow reference, and push parameters
(`crates/sipx-clstr-registrar/src/binding.rs:106-137`), so this is sensitive operational metadata even
when SIP signaling itself is cleartext.

**Evidence.** The only connector imports and passes `postgres::NoTls`
(`crates/sipx-clstr-node/src/postgres_store.rs:20-23,112-114`). There is no alternate TLS connector or
configuration. The public deployment design places an HA database behind multiple zones without
documenting this restriction (`website/docs/operate/deploy.md:34-73`).

**Recommendation.** Support a verified TLS connector, server-name/CA configuration by secret
reference, and an explicit TLS mode that defaults to verification for non-loopback databases. Refuse
`sslmode=require`-style intent if the build cannot honor it.

### SEC-03 — Medium — the loop-cookie HMAC key is predictable wall-clock text

**Impact.** Loop detection relies on an unforgeable cookie, but the per-process HMAC key is the
decimal Unix-epoch nanosecond at startup with a fixed prefix. An observer who knows a node's restart
window and can obtain one cookie for known routing state has an offline verifier for candidate seeds.
Successful recovery permits forged “our Via” loop cookies and can cause targeted `482 Loop Detected`
responses or undermine spiral/loop classification.

**Evidence.** `cookie_key()` derives the entire key from `SystemTime::now().duration_since(UNIX_EPOCH)`
and the constant string `sipx-clstr/` (`crates/sipx-clstr-node/src/driver.rs:771-782`). The proxy type
says the key exists so outsiders cannot forge cookies (`crates/sipx-clstr-proxy/src/config.rs:40-46`),
uses a 64-bit truncated HMAC (`crates/sipx-clstr-proxy/src/cookie.rs:52-58,140-165`), and treats a
matching attacker-supplied Via cookie as a loop (`crates/sipx-clstr-proxy/src/validate.rs:98-104,149-160`).
HMAC is not the weakness; key generation is.

**Recommendation.** Generate at least 256 bits from the operating system CSPRNG and inject the key
through the existing sans-IO configuration boundary. Distribution and rotation can remain separate;
a safe per-process key does not need to wait for cluster-wide key management.

### BUILD-01 — Medium — the documented container build produces the unoptimized dev binary

**Impact.** The ordinary documented image is compiled with the Cargo dev profile. On a public SIP
parser and transaction path, substantially lower throughput and debug-profile behavior reduce the
load needed for CPU exhaustion and make capacity tests unrepresentative of a release artifact.

**Evidence.** `CARGO_PROFILE` defaults to `dev`, selecting plain `cargo build` and copying
`target/debug/sipx-clstr` (`Dockerfile:34-52`). The published command is simply
`docker build -t sipx-clstr .` and does not override the profile
(`website/docs/guides/docker-and-k3d.md:13-26`). The runtime hardening is otherwise sound: it uses an
unprivileged UID (`Dockerfile:64-70`), and the development node manifests drop capabilities and use a
read-only root filesystem (`deploy/devspace/manifests/node.yaml:186-193,287-294`).

**Recommendation.** Default the distributable image to `release`; provide an explicitly named dev
target/profile for rapid iteration. Build the exact release image in CI and run the real-socket and
load smoke tests against that image.

### CI-01 — Medium — the end-to-end proof does not use the immutable kernel revision

**Impact.** The Rust build is reproducibly locked to one sipx commit, but the independent CLI in the
end-to-end job is fetched by mutable tag name. If that tag moves, CI can pass or fail against a CLI
revision different from the libraries linked into the node while reporting that both came from the
same pin. This weakens both release evidence and supply-chain provenance.

**Evidence.** `Cargo.lock:1636` records the exact commit, while the e2e workflow extracts `v0.10.0`
from `Cargo.toml`, performs `git clone --branch "$tag"`, and builds whatever commit the remote tag
currently resolves to (`.github/workflows/ci.yml:126-143`). CI and Pages actions are also selected by
mutable major-version tags rather than full commit SHA—for example
`.github/workflows/ci.yml:25-34` and `.github/workflows/website.yml:32-46`—and container bases/services
use mutable image tags (`Dockerfile:25,56`; `.github/workflows/ci.yml:158-166`).

**Recommendation.** Resolve the sipx lockfile commit and checkout exactly that object, then assert its
version/tag for readability. Pin third-party actions and published base images by reviewed digest,
with a deliberate update mechanism. Produce an SBOM and run dependency/image vulnerability policy
on the artifact that will be released.

### K8S-01 — Low — Helm package metadata promises an installation the chart cannot perform

**Impact.** `helm show chart` tells an operator that installation creates the operator, CRDs, RBAC,
and a working local environment, but the chart only emits a custom resource for which no CRD or
controller exists. Automation that trusts package metadata can report a successful Helm release
while no SIP workload runs.

**Evidence.** The chart description makes the installation promise
(`deploy/helm/Chart.yaml:3-9`), while its only template is
`deploy/helm/templates/sipxcluster.yaml:30-58`. `values.yaml` accurately says nothing runs yet
(`deploy/helm/values.yaml:33-41`), as does the public deployment page
(`website/docs/operate/deploy.md:8-26`), so the defect is inconsistent package metadata rather than a
hidden implementation state.

**Recommendation.** Change `Chart.yaml` to say that this is a schema/rendering preview and cannot be
installed until the CRD/controller exist. Add `helm install` against a disposable API server to the
future chart acceptance; rendering alone cannot prove a deployable package.

## Priority order

1. Refuse `cluster.security` and role combinations the driver cannot enforce; stop serving undeclared
   transports. These are immediate fail-open boundaries.
2. Correct the public “today” matrix for CANCEL, Timer C, TCP reachability, and PostgreSQL readiness.
3. Put a total bound around tasks/logging/database work, then replace the synchronous single-client
   database seam with an async, deadline-bounded pool.
4. Wire real CANCEL and Timer C effects and prove them on sockets.
5. Replace predictable cookie keys, require database TLS, and harden release reproducibility.

## Positive controls and residual risk

The workspace-wide `unsafe_code` prohibition and network-input panic policy are strong foundations.
Configuration is generally closed-world, database startup failure does not fall back to memory,
stored mutations use a revision predicate, change feeds are bounded, CI runs feature-off builds, and
the development containers use a non-root user with capability drops. These controls materially
reduce accidental corruption and privilege risk; the findings above are boundary-integration defects,
not an absence of defensive intent.

This review deliberately does not duplicate the open upstream-ledger findings about digest nonce
uniqueness, replay-window complexity, or per-message overload logging. They remain release risks in
addition to this report. There is also no production operator/CRD or HA Kubernetes deployment to
penetration-test yet. The current devspace profile has no SIP-node liveness/readiness probe, one
ephemeral PostgreSQL instance, no NetworkPolicy, and intentionally inline development credentials;
the public guide labels it development-only, so these are residual limitations rather than separate
misconfiguration findings.
