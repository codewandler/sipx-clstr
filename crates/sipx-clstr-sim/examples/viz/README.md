# constellation — the live cluster animation (VZ-1)

The deterministic cluster harness, rendered live: the register-then-call showcase scenario —
alice and two of bob's devices register, alice calls bob, the edge forks — paced against the wall
clock and streamed over SSE to an embedded canvas page. Every pixel is a trace entry; the stream
is the trace and nothing else (see `docs/designs/cluster-viz.md`).

## Run it

```sh
cargo run -p sipx-clstr-sim --example viz
# open http://127.0.0.1:8975/
```

Adversarial weather, faster, a different run:

```sh
cargo run -p sipx-clstr-sim --example viz -- --seed 0xc0ffee --links storm --speed 8
```

Same seed, same stream — the run is reproducible, which is the point.

| Flag | Values | Default |
|---|---|---|
| `--seed` | decimal or `0x…` | `0xc0ffee11` |
| `--speed` | virtual seconds per wall second | `1` |
| `--links` | `clean`, `jittery`, `storm` | `jittery` |
| `--port` | `0–65535` | `8975` |

Silences play at 8× — the harness's fast-forward made visible rather than hidden. The topbar
shows virtual time against wall time, so the acceleration is on screen, not behind it.

## Routes

| Route | What it is |
|---|---|
| `GET /` | the canvas page |
| `GET /events` | the SSE frame stream, exactly as the page sees it |
| `GET /healthz` | `ok` |

The raw feed is plain text — no browser needed to watch a run:

```sh
curl -N http://127.0.0.1:8975/events
```

Frames: `meta` (scenario, seed, link weather, nodes with roles, links — the stage), `tick` (virtual/wall clock
ratio), `invariant` (the DP-3 counter set; uninstrumented counters are present and null, never a
pretend zero), and one frame per trace entry — `started`, `sent`, `received`, `dropped`,
`duplicated`, `broken`, `malformed`, `timer_set`, `timer_fired`, `timer_cleared`, `note` — with
`id:` set to the trace `seq`, so a reconnecting client sees gaps rather than missing them
silently.

## Smoke test

The end-to-end proof, browser not required — spawns the real server, polls `/healthz`, fetches
the page, and asserts a live stream of `meta`/`tick`/`invariant`/trace frames plus backlog resync
for a late client:

```sh
cargo test -p sipx-clstr-sim viz_smoke
```

Add `-- --nocapture` to see the evidence: the stage description, a frame census, and the HUD
readings as the test observed them.
