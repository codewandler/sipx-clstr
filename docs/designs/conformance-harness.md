# Design: Conformance & deterministic harness

**Status:** proposed · **Pillar:** Platform · **Epic:** `conformance-harness` ·
**Stories:** CF-1 … CF-6

## Why

The north star made executable: seeded multi-node simulation, and coverage that is measured.

The north star — a cluster indistinguishable from one correct proxy — is a claim about behavior
under adversarial timing and partial failure, and such claims are only worth making if they are
*proved*, deterministically, on every commit. Nothing in the sipx kernel or elsewhere provides
this today: the kernel's sans-IO layers are deterministic by construction, but there is no
injectable clock, no in-memory transport, and no multi-node scenario runner anywhere. Meanwhile
"compliant" must be measurable: the platform explicitly does *not* promise every RFC, so it owes
an exact statement of what it does implement, per normative requirement. The two halves are one
epic because they share the machine-readable registry (`extension-framework`): coverage claims
come from the same data that drives codegen, and the harness produces the evidence the
conformance report cites.

## Approach

**The deterministic cluster harness** (CF-1): a simulation runtime that owns time (virtual clock,
timers fire only when advanced), transport (in-memory links with configurable loss, duplication,
reordering, latency and partitions), randomness (seeded), and topology (N platform nodes, M
simulated UAs/carriers, a scripted fault schedule: node kill, partition, timer skew). Every
sans-IO component (proxy core, location logic on the in-memory store, token library, route
planning) runs unmodified inside it; a failing seed is a reproducible test case. CF-1 also
decides the upstream split — what generalizes into `sipx-testkit` (clock trait, loopback
transport) versus what is cluster-specific and stays here ([upstream ledger](../upstream.md)).

**The conformance database** (CF-2): per normative requirement — RFC section, requirement text
reference, applicability (role, transport) — a recorded status: implemented / not applicable to
this role / profile-disabled / partially implemented / known deviation / interop workaround; plus
the tests that prove it. Four coverage kinds are reported separately: **syntax** (parse +
serialize), **behavioral** (normative state transitions), **role** (UAC/UAS/proxy/registrar), and
**interop** (verified against independent implementations). The report is generated from the
registry the way the story board is generated from frontmatter — never hand-maintained.

**Real-network interop** (CF-3) sits on top, never instead: SIPp scenario suites and the sipx CLI
phone as a scriptable UA against a containered deployment, mirroring the kernel's own
Docker-based interop harness pattern; rtpengine joins for media-path tests (ME epic). Fault
injection against real sockets (CF-4's network half) validates that the simulation's failure
model matches reality.

## Alternatives considered

- **Integration tests on real sockets with retries as the primary strategy.** Rejected: timing
  flake either gets retried into meaninglessness or blocks CI; adversarial schedules (the
  interesting ones) can't be expressed at all.
- **Model checking the protocols instead of simulating them.** Deferred, not rejected: the
  sans-IO discipline keeps exhaustive-exploration options open later; simulation with seeds gives
  most of the value at a fraction of the cost now.
- **Hand-maintained conformance spreadsheet.** Rejected: it drifts; generated-from-registry is
  the same discipline as the generated board.

## Risks & open questions

- Fidelity gap between simulated and real transports (TLS handshake behavior, socket-level
  backpressure) — bounded by CF-4's comparison runs.
- Requirement extraction effort: seeding the registry with §16 and §10 requirements is real work;
  scope it to the RFCs the profiles actually claim. Owned by CF-6, so it cannot silently land
  inside CF-2's or EX-2's scope.
- Whether harness scenarios live as code or as declarative scripts; decided in CF-1.

## Acceptance / done

The union of CF-1 … CF-6: harness design accepted (M0, CF-1) and implemented early in M1 (CF-5)
so the proxy core's first vectors run under it; the M2 cluster assertions (node kill, partition,
zero cross-node lookups) expressed as seeded scenarios; the registry seeded with the M1
profile's normative requirements (CF-6) and the conformance report generated from it with all
four coverage kinds (CF-2); SIPp + CLI interop green in CI against the reference deployment.
