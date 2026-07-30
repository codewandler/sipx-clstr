# Design: Conformance & deterministic harness

**Status:** accepted — CF-1 decided the harness; CF-5 realizes it · **Pillar:** Platform ·
**Epic:** `conformance-harness` · **Stories:** CF-1 … CF-6

## Why

The north star made executable: seeded multi-node simulation, and coverage that is measured.

The north star — a cluster indistinguishable from one correct proxy — is a claim about behavior
under adversarial timing and partial failure, and such claims are only worth making if they are
*proved*, deterministically, on every commit. Nothing in the sipx kernel or elsewhere provides
this today: the kernel's sans-IO layers are deterministic by construction, but there is no
injectable clock, no in-memory transport, and no multi-node scenario runner anywhere. Meanwhile
"compliant" must be measurable: the platform explicitly does *not* promise every RFC, so it owes
an exact statement of what it does implement, per normative requirement. The kernel has begun
paying the coarser half of that debt for itself — a per-RFC registry (`docs/rfc/registry.toml`)
rendered into a checked compliance report, enforced in its gate — and this epic extends that
discipline to requirement grain rather than duplicating it. The two halves are one epic because
they share the machine-readable registry (`extension-framework`): coverage claims come from the
same data that drives codegen, and the harness produces the evidence the conformance report
cites.

## Approach

**The deterministic cluster harness** (CF-1, implemented by CF-5) is decided below as six
commitments: how time works, what a node is, what the network is, what a scenario is, what a
seed guarantees, and how load is modeled — then the upstream split and the conformance half.

**Time is a discrete-event queue.** The harness owns a virtual clock — `SimTime`, nanoseconds
since scenario start — that exists only inside the scheduler. One earliest-deadline-first event
queue holds everything that has not happened yet: message deliveries (arrival = now + sampled
link latency), timer expiries, fault-schedule actions, load-source ticks. Advancing time *is*
popping the queue: `now` jumps to the next event's deadline and nothing else moves it, so timers
fire only when the simulation advances and an idle cluster fast-forwards through silence for
free. Equal deadlines are ordered by insertion sequence, making the total event order a pure
function of (scenario, seed).

Sans-IO components never see any of this — their contract is unchanged. The proxy core takes
`TimerFired` inputs and emits `SetTimer`/`ClearTimer` effects with relative durations
([proxy-behavior §2](../specs/proxy-behavior.md)); the kernel's `TransactionLayer` has the same
shape (`on_timer`, `Output::SetTimer { after }`). The harness translates each `SetTimer` into a
queue entry at `now + after`, keyed `(node, owner, timer)`, and each pop back into a fired-timer
input to the owning component. Cancellation uses the generation-counter discipline the kernel
driver's timer queue already has — set/clear bumps a generation, stale entries are discarded at
pop — reused via the upstreamed queue (split, below) rather than re-implemented.

**A node is the shipped composition under a sim driver.** A simulated platform node runs the
same sans-IO stack the real binary will: proxy engine over the kernel transaction layer,
location logic over the in-memory store, token mint/verify, hook modules. Around it the harness
puts a *sim driver* mirroring the real driver's contract: feed inputs (messages, fired timers,
resolved targets, token verdicts), perform effects strictly in order, append every input and
effect to the trace. The kernel's tokio endpoint driver is not in the loop — the sim driver is
its stand-in, which is exactly the seam non-negotiable 2 promises. Because store and RPC access
are effects rather than ambient IO, "zero cross-node dialog lookups" is a trace query, not an
honor-system claim.

**The network is links with policies; faults are scheduled policy changes.** The topology
declares nodes (platform nodes, simulated UAs and carriers, the LB) and the links between them.
Two link kinds mirror transport reliability classes: a *datagram* link samples per message —
drop with probability `loss`, duplicate with probability `duplicate`, deliver at
`now + latency.sample(rng)` — so reordering emerges from latency jitter the way it does on the
wire; a *stream* link preserves FIFO order, applies latency to delivery, and fails by breaking
the connection, surfacing as a transport-error input (§16.9, PB-R-9). The knob set per link:
`loss`, `duplicate`, `latency` (constant | uniform | empirical table), `partitioned`.

Knobs are **per-link defaults set by the topology; the fault schedule overrides them in time
windows** — no third mechanism. `Partition({e0}, {e1, loc}, 3s..9s)` is a scheduled override of
`partitioned` on the crossing links; `KillNode(e0)` partitions every link of a node permanently
and drops its pending timers; timer skew is a per-node rate multiplier applied when translating
`SetTimer` durations. The dataplane's 5-tuple stickiness (DP-2) is modeled by an LB actor owning
the flow→edge map, and a **stickiness miss is a fault like any other**: a scheduled
`RemapFlow(flow, edge)` action, or a probabilistic `stickiness_miss` rate on the LB. PB-A-1 is
then literal: INVITE delivered to edge-0; `RemapFlow` fires; the retransmission lands on edge-1;
the scenario asserts the duplicate-fork counter incremented and stays within the bound the
schedule implies.

**Scenarios are code; schedules and policies are data.** Decided: scenarios are Rust — a
builder for topology, plain values for link policies and fault schedules, typed assertions over
the trace — and each scenario is a `#[test]`. Rationale: the assertions that matter here are
queries over typed effect streams (lookup counts, goodput curves, counter bounds), which a
declarative format would have to grow expressions for; parameterization (node counts, seed
sweeps, rate ramps) is ordinary code, and a schema that acquires loops and variables is an
embedded interpreter by installments — the vision's no-DSL non-goal, the same rejection
routing-trunks gave a routing script language. What *is* declarative is the data: `LinkPolicy`,
`Fault`, and load shapes are plain serializable values, so CF-4 can generate, mutate, or fuzz
schedules without touching scenario logic, and scenario composition is value composition —
merging two fault schedules is concatenating two lists. Spec vectors
([proxy-behavior §12](../specs/proxy-behavior.md)) run as degenerate scenarios: one node, clean
links, scripted inputs — message in, ordered effects asserted out; the PB-A rows are the
multi-node exception and use the LB actor above. Sketch:

```rust
#[test]
fn bye_survives_edge_kill() {
    let mut sim = Sim::new(seed());                    // env override, else the pinned default
    let top = sim.topology(|t| {
        t.edges(3);                                    // proxy core + token lib per edge
        t.lb("vip", Stickiness::FiveTuple);
        t.location_store("loc");
        t.uas(["alice", "bob"]);
        t.link_default(LinkPolicy { latency: Latency::uniform_ms(2, 20), ..LinkPolicy::CLEAN });
    });
    sim.schedule([(at_secs(30), Fault::KillNode(top.edge(0)))]);

    let call = sim.drive(establish("alice", "bob"));   // INVITE … ACK; Record-Route tokens minted
    sim.advance_to(at_secs(31));                       // edge-0 is gone; vip re-homes the flow
    let bye = sim.drive(bye(&call, "bob"));

    assert_eq!(bye.final_status(), Some(200));         // token-routed via a surviving edge
    assert_eq!(sim.trace().dialog_lookups_on_signalling_path(), 0);
}
```

**One seed, one trace.** A scenario runs from a single `u64` master seed. Every randomness
consumer gets a named stream derived by keyed hash of (master, stable label) —
`"link:alice->vip"`, `"node:edge-1:branch"`, `"load:carrier-a"` — so adding a stream never
reshuffles the others and a scenario edit does not silently invalidate unrelated seeds. The
kernel already takes randomness as an injected source (the resolver's RNG parameter); nodes draw
branch and nonce randomness the same way, from their stream. The run's product is the trace: an
append-only log of (time, seq, node, input | effect); same scenario, same seed — byte-identical
trace. A failing assertion reports scenario name, master seed, the event index it failed at, and
the trace tail; replay is `HARNESS_SEED=0x… cargo test <scenario>`. CI runs every scenario at
its pinned default seed, plus a nightly sweep of fresh seeds; a sweep failure is pinned into the
suite as an explicit regression seed.

**Load is modeled, not deferred** (routing-trunks names the RT-3 overload-collapse scenario as a
CF-1 input). Two pieces make collapse expressible. *Open-loop sources*: a load actor emits new
calls on a rate function over virtual time — constant cps, ramp, step — regardless of how the
cluster is coping; closed-loop sources self-throttle and hide exactly the phenomenon RFC 7339
exists for. *A node service model*: each sim driver applies a configurable per-message service
time (a distribution, per node class) and a bounded input queue whose overflow sheds the way the
real driver does (`503` + `Retry-After`); without a cost model simulated capacity is infinite
and overload cannot exist. Goodput is measured from the trace — calls established within their
timer budget per unit virtual time, against offered rate — and the collapse assertion has a
shape: past saturation, goodput plateaus; it must not fall off a cliff while offered load keeps
rising. Reports reuse the cause-counted, percentile vocabulary of `sipx-testkit`'s load module
(pure data types), so a simulated run and a CF-3 real-socket run read identically. The source
actor and the service-model seam are in CF-5's runner from day one; RT-3 turns the assertion on
when overload control lands.

**The M2 exit assertions are three scenarios.**

1. *Node kill* — `bye_survives_edge_kill`, sketched above: establish through edge-0, kill it,
   route the BYE by token through a survivor; assert `200` and zero dialog lookups.
2. *Partition* — `partition_spares_mid_dialog`: partition edge-2 from the location store for
   3s..9s under steady call load. Inside the window, new INVITEs at edge-2 fail fast and bounded
   (the `ResolveTargets` effect errors; a 5xx per policy within its timer budget, never a hang)
   while established dialogs keep forwarding through edge-2 untouched — token-routed, store
   never consulted. After heal, new calls succeed. Assert: mid-dialog goodput flat across the
   window; zero dialog lookups on the mid-dialog path; every request concluded within its timer
   budget.
3. *Foreign-edge spray* — `foreign_edge_spray`: establish N calls, then spray mid-dialog traffic
   (BYE, re-INVITE, 2xx ACK) across all edges with `RemapFlow` faults plus a
   retransmission-window stickiness miss. Assert: everything token-routed per PB-A-2/PB-A-3, the
   PB-A-1 duplicate-fork counter within the schedule's bound, zero dialog lookups — "any edge
   handles any mid-dialog message", measured.

**The sipx-testkit split, per component** (the upstream checkpoint, AGENTS.md rule 6; recorded
in the [ledger](../upstream.md)):

| Component | Decision |
|---|---|
| Virtual clock | **Local.** Not a kernel trait: sans-IO layers take fired timers by contract and must never see a clock; the clock is the scheduler's loop variable. The kernel driver's own tests already virtualize tokio time and need nothing more. |
| Deterministic timer queue | **Upstream.** Generalize the kernel transport's `TimerQueue` — generic over key, `now` passed in instead of read from `tokio::time::Instant::now()` inside `set` — so the tokio driver and this scheduler share one generation-counter cancellation discipline instead of two. |
| Loopback / in-memory link | **Upstream** into `sipx-testkit`: a seeded in-process byte link with loss/duplication/latency knobs. The testkit's own manifest promises loopback transports and ships none; the kernel needs exactly this to test retransmission under loss. |
| Simulated network (topology, LB/stickiness actor, partitions, fault-schedule interpreter) | **Local.** Edges, VIPs, zones and stickiness are cluster orchestration semantics by definition. |
| Multi-node scenario runner, trace, assertions | **Local.** It exists to prove cluster claims. |
| Seeded randomness | **Local composition, nothing to move.** Kernel APIs already accept injected RNGs; the stream-derivation discipline is harness-owned. |
| Simulated UAs / carriers | **Local**, composed from kernel sans-IO layers; the scriptable real-socket UA remains CF-3's job (the sipx CLI phone). |
| Load sources + service model | **Local**; the report vocabulary (cause taxonomy, percentiles) is reused from `sipx-testkit`'s load module rather than re-invented. |

The upstream asks reduce to one CX-1 story with two items — the timer-queue generalization and
the seeded loopback link — both small, both useful to the kernel on their own.

**The conformance database** (CF-2): per normative requirement — RFC section, requirement text
reference, applicability (role, transport) — a recorded status: implemented / not applicable to
this role / profile-disabled / partially implemented / known deviation / interop workaround; plus
the tests that prove it. Four coverage kinds are reported separately: **syntax** (parse +
serialize), **behavioral** (normative state transitions), **role** (UAC/UAS/proxy/registrar), and
**interop** (verified against independent implementations). The report is generated from the
registry the way the story board is generated from frontmatter — never hand-maintained.

**Registry alignment: extend the kernel's format, as an independent instance, inheriting by
reference.** The kernel measures itself per RFC: `docs/rfc/registry.toml` (status, layer, roles,
headers, methods, evidence) rendered by `rfc-report.py` into `docs/compliance.md`, with
`--check` in its gate holding claims against the code. Decided: this repo's registry (seeded by
CF-6, schema owned by EX-2) adopts that schema as its base and extends the grain — an entry
gains `[[rfc.requirement]]` rows (section, requirement reference, applicability by role,
transport and profile, status including deviation and workaround, proving tests) plus the four
coverage kinds. It is an independent instance in this repo, because the platform claims
different roles (proxy, registrar) over a different RFC set, with its own generator honoring the
same two rules — generated, and checked: our checker verifies cited vector IDs (PB-\* rows,
harness scenario names) exist, where the kernel's verifies parser tables. Kernel-implemented
behavior is inherited by reference, never re-claimed: an entry may declare `inherits`, naming a
kernel registry row at the pinned kernel version, and the checker verifies that row exists — so
transaction-layer claims live in one place and cannot drift. Offering the requirement-grain
extension upstream so both repos converge on one schema goes through CX-1 as an offer, not a
dependency ([ledger](../upstream.md)).

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
- **Declarative scenario files (TOML/YAML) instead of code.** Rejected for scenario logic, kept
  for data: assertions over typed traces and parameterized topologies would force expressions,
  loops and variables into a schema — an embedded interpreter by installments, the vision's
  no-DSL non-goal. Fault schedules and link policies stay declarative *values* inside the code,
  which is the part CF-4 needs to generate and fuzz.
- **A paused async runtime's clock as the simulation clock.** Rejected: it virtualizes one
  runtime's timers, not a cluster — auto-advance is scheduler-coupled, ordering across N nodes
  and M links is not a pure function of a seed, and it ties the harness to an async runtime the
  sans-IO stack deliberately does not have. Fine for kernel driver tests; not a simulation.
- **Hand-maintained conformance spreadsheet.** Rejected: it drifts; generated-from-registry is
  the same discipline as the generated board.

## Risks & open questions

- Fidelity gap between simulated and real transports (TLS handshake behavior, socket-level
  backpressure) — bounded by CF-4's comparison runs. The service model is the sharpest edge:
  until calibrated against CF-3 measurements, simulated capacity numbers are model, not
  measurement, and RT-3's absolute thresholds must be re-validated on real sockets.
- Requirement extraction effort: seeding the registry with §16 and §10 requirements is real work;
  scope it to the RFCs the profiles actually claim. Owned by CF-6, so it cannot silently land
  inside CF-2's or EX-2's scope.
- Seed-stability discipline: RNG stream labels are identifiers; renaming one invalidates every
  pinned regression seed that touched it. Convention: labels are stable API, renames are
  breaking and reviewed as such.
- Trace volume under load scenarios: a high-rate run cannot retain every effect verbatim. The
  trace needs retention modes — full for vectors and small scenarios, counters plus sampled
  windows for load runs — without breaking "same seed, same trace" for what is retained.

## Acceptance / done

The union of CF-1 … CF-20: harness design accepted (M0, CF-1) and implemented early in M1 (CF-5)
so the proxy core's first vectors run under it; the M2 cluster assertions (node kill, partition,
zero cross-node lookups) expressed as seeded scenarios in the code-as-scenarios format decided
here; the registry seeded with the M1 profile's normative requirements (CF-6) and the
conformance report generated from it with all four coverage kinds (CF-2); SIPp + CLI interop
green in CI against the reference deployment.

## Validated review remediation (2026-07-30)

`CF-3` labels same-kernel process tests as integration and reserves interoperability claims for an
independently implemented peer. `CF-20` makes proof discovery fail closed: a vector counts only when
an enabled test is listed and executed, while the e2e assertion requires exactly one observable UDP
socket owned by the node and treats unavailable or ambiguous inspection as failure.
