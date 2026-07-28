# Design: Live cluster visualization (`constellation`)

**Status:** active · **Pillar:** Platform · **Epic:** `cluster-viz` ·
**Stories:** `VZ-1` done (the replay feed and the page) · VZ-2 … VZ-4 scoped below, unfiled

## Why

The north star, made watchable: the cluster's behavior under adversarial timing and partial
failure, rendered live from the same event stream that proves it.

The [vision](../vision.md)'s principle 6 — deterministic before distributed — means every cluster
behavior already reproduces in the seeded harness before it touches a socket, and the harness's
product is an append-only, totally ordered, timestamped trace
([conformance-harness](conformance-harness.md)). That trace is exactly the input a live animation
needs, and today it is readable only as a fixed-width text diff — the right format for catching a
regression, the wrong one for seeing a partition heal, a timer storm, or a BYE route around a dead
edge. Meanwhile the invariant metrics DP-3 owes
([DP-3](../stories/DP-3-implement-observability-that-proves-the-invariants.md)) are numbers an
operator must already believe in to look at. The constellation closes the legibility gap: the
architecture chart drawn live, messages as particles, faults as visible events, and the invariants
as counters that must not move — in the simulation first, and against a real deployment when the
metric sets land. An existence proof that this works lives outside telephony: a Zig database
renders its own deterministic simulator live in the browser, faults and all. The load-bearing
rationale here is ours, though — principle 6 plus the invariant metrics DP-3 owes — and nothing
about the rendering is borrowed behavior.

## Approach

**The stream already exists; the visualization never invents behavior.** Every pixel is a trace
entry. The harness `Trace` (`crates/sipx-clstr-sim/src/trace.rs`) logs one `Entry` per happening —
`at` (virtual time), `seq` (global order), `node`, `node_name`, and an `Event` variant: `Started`,
`Sent`, `Received`, `Dropped`, `Duplicated`, `Broken`, `Malformed`, `TimerSet`, `TimerFired`,
`TimerCleared`, `Note`. Decided: the constellation's wire schema is a serialization of that
vocabulary plus three feed-level additions (`meta`, `tick`, `invariant`, below) — not a parallel
event model. A behavior that is not in the trace cannot appear on screen, which is what keeps the
animation honest: it is a *view of the evidence*, not a demo. The particle tooltips reuse the same
`summarize` the text render uses, so the screen and the trace diff never disagree about what a
message was.

**Three feeds, one schema, one page.** The page never changes; the source does.

1. **Sim replay (VZ-1).** A dev adapter runs a scenario — `register_then_call`, the proxy torture
   vectors, a seeded load run — and paces the virtual clock against the wall clock, emitting each
   trace entry as it appears. A recorded trace file replays through the same path (the degenerate
   feed: no live sim, just the log). This feed works today against the existing harness with zero
   new protocol code, and because the nodes under the sim driver are the shipped sans-IO stack,
   every behavior on screen is real.
2. **Interactive sim (VZ-2).** The same stream, plus a control channel: `POST` endpoints that
   inject faults into the running `Sim` — kill a node, partition a link set for a window, ramp
   offered load — mapping onto `Sim::set_partitioned` and the CF-4 fault-schedule machinery
   ([CF-4](../stories/CF-4-add-fault-injection-to-the-simulation.md)). The master seed and scenario
   name are always on screen; "share this run" is copying the seed, replaying it is the harness's
   own `HARNESS_SEED` discipline. A readout shows virtual time against wall time — idle periods
   fast-forward, storms play out — the harness's fast-forward made visible rather than hidden.
3. **Real cluster (VZ-3, specified now, built when its sources exist).** The same schema fed by a
   live deployment: invariant-metric changes from the DP-3 set, probe verdicts from ET-5
   ([ET-5](../stories/ET-5-publish-probe-results-as-metrics-and-alerts.md)), rollout stages from
   the operator's staged rollouts
   ([KO-8](../stories/KO-8-apply-live-config-changes-as-a-staged-rollout.md)), and — for real
   message particles — DP-7's selective signalling capture
   ([DP-7](../stories/DP-7-duplicate-signalling-to-a-capture-target-selectively.md)), which is the
   hot-path-safe tap by design. Nothing streams from the real cluster today: the node is a
   skeleton and the metric sets are backlog stories, and this design does not pull them forward.

**The wire format is SSE, one event per trace entry.** Server-Sent Events over plain HTTP:
`event:` named by kind, `data:` a single JSON object, `id:` set to the trace `seq` so a
reconnecting client resumes with `Last-Event-ID` and a gap is detectable rather than silent. The
schema carries `v: 1` from day one. Three feed-level frames round it out: `meta` (scenario name,
master seed, link weather, nodes with names and role hints, links with kinds — everything the renderer needs to
draw the stage), `tick` (virtual time, wall time, and their ratio), and `invariant` (a counter
snapshot, below). Sketch:

```json
{ "v": 1, "at": 4213370000, "seq": 1187, "node": 3, "node_name": "edge-1",
  "kind": "received", "from": 0, "summary": "INVITE sip:bob@example.test [a1b2c3 #1 INVITE]" }
```

The mapping from trace variant to frame kind to stage visual is total — a compile-time test
asserts every `Event` variant has a mapping, so a new trace variant fails a build rather than
rendering as nothing:

| Trace event | Frame `kind` | On stage |
|---|---|---|
| `Sent` / `Received` | `sent` / `received` | a particle crosses the link, colored by method/status |
| `Dropped` | `dropped` | the particle fizzles mid-link |
| `Duplicated` | `duplicated` | the particle splits in two |
| `Broken` | `broken` | the stream link snaps with a flash |
| `Malformed` | `malformed` | the particle arrives scrambled and dissolves at the receiver |
| `TimerSet` / `TimerFired` / `TimerCleared` | `timer_set` / `timer_fired` / `timer_cleared` | a thin ring sweeps the node; fires flash it — retransmission timers and Timer C become legible |
| `Note` | `note` | a HUD log line — the seam store lookups, token verdicts and sheds already use |
| `Started` | `started` | the node lights up |

Partitions and node kills are not trace events; they are fault-schedule actions, so the adapter
emits them as `fault` frames (kind, links or node affected, window) and the stage draws a dashed
cut across the affected links or dims the dead node.

**The stage is the architecture chart, not a graph layout.** The canvas is
[architecture](../architecture.md) chart 1 drawn live: UAs, carriers and the e2e-tester probe on
the left; the VIP/L4 balancer; the edge tier; registrar shards with PostgreSQL, routing/policy and
the echo endpoint on the right; the media pool along the bottom. Positions come from a fixed
layout keyed by the role hints in `meta` — an operator recognizes the deployment they run, and two
runs of the same scenario look identical. Particle colors are a small fixed grammar: INVITE amber,
REGISTER cyan, BYE red, 2xx green, 4xx/5xx magenta, CANCEL white.

**The invariant HUD is the point.** Permanently on screen, from the DP-3 acceptance list:
cross-node dialog lookups (must read zero — the M2 exit criterion), token verification failures by
cause, flow-RPC outcomes, per-trunk breaker state. In the sim feeds these are trace queries over
`Note` and effect entries; in the real feed they are the metrics themselves — same vocabulary, so
the animation and the alerts can never tell two stories. One honesty rule, in the spirit of the
probe's `inconclusive` verdict ([e2e-tester](e2e-tester.md)): a counter with no instrumented
source renders as *uninstrumented*, never as zero — silence and a clean reading must not look
alike.

**Pacing and scale are explicit.** The adapter paces emission by virtual time at a selectable
ratio (default 1:1), compressing silence at a bounded fast-forward so quiet scenarios don't bore
and storms don't blur. Under load, particles exceed any sane budget: past a per-link rate the
renderer degrades to throughput sparklines with sampled particles — the visual analog of the
trace's retention modes for load runs (a risk the harness design already names). SSE is one-way,
so a slow client can never backpressure the sim: the adapter drops frames for that client and
sends a `gap` notice; the `seq` ids make the hole visible.

**The adapter is a dev driver, never a role.** It ships as a cargo example in `sipx-clstr-sim`
(`examples/viz/`, dev-dependencies only, serving an embedded static page), binds localhost by
default, and holds no write path to anything in feeds 1 and 2 beyond the running sim it owns. It
is not part of the deployment surface: nothing lands in `deploy/helm/`, and a consuming deployment
never sees it — the downstream boundary applies to tooling too. The sans-IO crates are untouched;
this is a driver over the harness, which is what drivers are for.

**Upstream checkpoint (AGENTS.md rule 6): considered — no.** The stage, the invariants, the
topology and the fault vocabulary are cluster orchestration semantics; the event vocabulary is the
harness trace, already decided local in the CF-1 split
([conformance-harness](conformance-harness.md)). Nothing here is protocol-generic.

## Alternatives considered

- **A post-hoc trace-file player as the whole feature.** Rejected: it cannot do interactive fault
  injection, which is where the understanding is. Kept as feed 1's offline mode — same schema,
  same page, no live sim.
- **WebSocket instead of SSE.** Rejected: the stream is strictly server→client; SSE is plain HTTP,
  curl-able (a `curl -N` shows exactly what the page sees, same debuggability ethos as the text
  trace), and reconnects with `Last-Event-ID` for free. The only client→server messages are fault
  commands, which are ordinary POSTs — no duplex to justify.
- **Metrics pipeline first (render Grafana, skip the stage).** Deferred, not rejected: that is
  DP-3's job for operators, and time-series dashboards do not express particles, faults and
  topology. The HUD reuses DP-3's counter vocabulary precisely so the two never diverge.
- **Compile the sim to WASM and run it in the browser.** Rejected for now: a server-side run
  streamed over SSE gets the same visual with the real code and no second build target, and the
  interactive feed needs a server-owned sim anyway. Revisit if a standalone page with no backend
  becomes a requirement.
- **Force-directed or 3D layout.** Rejected: the stage is the fixed architecture chart —
  recognition, not novelty. Wobbling layouts hide the topology the operator already knows.

## Risks & open questions

- **Schema drift** between trace variants and frame kinds — mitigated by deriving the payload
  types from the trace types and the compile-time totality test above; a new `Event` variant
  breaks a build, not a browser.
- **Role hints in `meta`.** Node names are scenario-defined; the fixed layout needs roles. Open
  whether the harness grows an explicit role annotation (cleanest) or the adapter infers from
  names (cheapest, brittle). VZ-1 decides.
- **Feed 3's dependencies are real.** DP-3, ET-5 and DP-7 are backlog stories; the real-cluster
  feed is specified here so the schema accommodates it, and built only when its sources exist.
- **The pixels are not the evidence.** Same seed gives the same stream, but rendering is
  wall-clock and lossy under aggregation. The claim stays precise: the *stream* is deterministic
  and complete; the screen is a view. Anything asserted still asserts on the trace.
- **Publishing.** Whether a replay of a pinned showcase scenario should be served from the docs
  site ([deployment](deployment.md) covers the release surface; the site deploys on release) is
  open — attractive, but only once VZ-1 exists and the showcase scenario is worth showing.

## Run it

The VZ-1 feed is live:

```sh
cargo run -p sipx-clstr-sim --example viz -- --links storm --speed 8
# open http://127.0.0.1:8975/ — or read the raw feed the page renders:
curl -N http://127.0.0.1:8975/events
```

Flags: `--seed N|0xN` (default `0xc0ffee11`), `--speed R` virtual seconds per wall second
(default 1; silences play at 8×), `--links clean|jittery|storm` (default `jittery`), `--port P`
(default 8975). Routes: `GET /` the canvas page, `GET /events` the SSE stream, `GET /healthz`.
The browser-free end-to-end proof is the smoke test — `cargo test -p sipx-clstr-sim viz_smoke`
spawns the real server and asserts health, the page, and a live stream of
`meta`/`tick`/`invariant`/trace frames, including backlog resync for a late client. The full
runbook lives next to the adapter in `crates/sipx-clstr-sim/examples/viz/README.md`.

## Acceptance / done

Scoped as four stories, filed when the epic is scheduled:

- **VZ-1 — the replay feed and the page.** The SSE adapter (sim example) streams a seeded scenario
  live; the canvas renders the stage, particles, fault visuals and the invariant HUD; `curl -N`
  shows the documented frame stream; the totality test covers every trace variant. Proved by:
  replaying `register_then_call` and a torture scenario at two speeds, frame-for-frame equal to
  the trace.
- **VZ-2 — interactive faults.** POST kill/partition/load-ramp against the running sim; seed and
  scenario always displayed; a shared seed reproduces the shared run.
- **VZ-3 — the real-cluster feed.** Same page, feed switch, HUD unchanged; sources are DP-3
  metrics, ET-5 verdicts and DP-7 capture. Blocked until those land.
- **VZ-4 — load mode.** Aggregation (sparklines, sampled particles) once load scenarios produce
  volumes that need it.

Done means: a seeded scenario plays live in a browser with every fault and invariant visible, the
stream is demonstrably the trace and nothing else, and the gate — including `check-docs` and the
provenance check — stays green.
