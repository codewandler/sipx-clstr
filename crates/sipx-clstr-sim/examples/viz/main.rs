//! The constellation replay feed (`VZ-1`): pace a seeded scenario against the wall clock and
//! stream its trace live over SSE, plus the embedded canvas page that renders it.
//!
//! A dev driver, never a role: localhost-only, no deployment surface. The stream is the trace and
//! nothing else — every `sent`/`received`/`dropped`/… frame is one trace entry serialized (see
//! `src/viz.rs`), `id:` set to the entry's `seq` so a reconnecting client sees gaps rather than
//! missing them silently.
//!
//! ```sh
//! cargo run -p sipx-clstr-sim --example viz -- --seed 0xc0ffee --links storm --speed 8
//! curl -N http://127.0.0.1:8975/events     # the frame stream, exactly as the page sees it
//! ```
//!
//! Routes: `GET /` the canvas page · `GET /events` the SSE stream · `GET /healthz`.
//!
//! Flags: `--seed N|0xN` (default 0xc0ffee11) · `--speed R` virtual seconds per wall second
//! (default 1) · `--links clean|jittery|storm` (default jittery) · `--port P` (default 8975).
//! Silences play at 8× — the harness's fast-forward made visible rather than hidden.

mod scenario;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use sipx_clstr_sim::viz::{self, Frame};
use sipx_clstr_sim::{LinkKind, LinkPolicy, SimTime};

const PAGE: &str = include_str!("page.html");

/// How much virtual time one pacing step covers. Small enough that a busy scenario streams
/// smoothly at 1×, large enough that an idle one does not spin.
const SLICE: Duration = Duration::from_millis(25);
/// Silences play this much faster than busy stretches — bounded fast-forward, not skipping:
/// every entry still crosses the wire, the quiet gaps between them just cost less wall time.
const IDLE_ACCELERATION: f64 = 8.0;
/// Per-client queue depth. A client this far behind is resynced instead of fed late frames.
const CLIENT_QUEUE: usize = 512;

const USAGE: &str = "usage: cargo run -p sipx-clstr-sim --example viz -- \
[--seed N|0xN] [--speed R] [--links clean|jittery|storm] [--port P]";

#[derive(Debug, Clone)]
struct Config {
    seed: u64,
    speed: f64,
    links: String,
    port: u16,
}

fn main() -> ExitCode {
    let cfg = match parse_args() {
        Ok(cfg) => cfg,
        Err(message) => {
            eprintln!("viz: {message}");
            return ExitCode::from(2);
        }
    };
    let hub = Arc::new(Mutex::new(Hub::default()));

    let sim_cfg = cfg.clone();
    let sim_hub = Arc::clone(&hub);
    thread::spawn(move || {
        if let Err(error) = run_sim(&sim_cfg, &sim_hub) {
            eprintln!("viz: the scenario halted: {error}");
        }
    });

    match serve(&cfg, &hub) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("viz: {error}");
            ExitCode::FAILURE
        }
    }
}

// ------------------------------------------------------------------------------------------
// the pacing loop: virtual time against the wall clock, frames as the trace grows
// ------------------------------------------------------------------------------------------

fn run_sim(cfg: &Config, hub: &Arc<Mutex<Hub>>) -> Result<(), sipx_clstr_sim::SimError> {
    let Some(policy) = policy_of(&cfg.links) else {
        eprintln!("viz: unknown link weather {:?} — {USAGE}", cfg.links);
        return Ok(());
    };
    let scenario::BuiltScenario {
        mut sim,
        nodes,
        links,
    } = scenario::register_call(cfg.seed, policy);
    broadcast(hub, &meta_sse(cfg, &nodes, &links));

    let wall_start = Instant::now();
    let mut cursor = 0_usize;
    let mut slice = 0_u64;
    loop {
        sim.advance(SLICE)?;

        let new_entries = sim.trace().entries().get(cursor..).unwrap_or(&[]);
        let busy = !new_entries.is_empty();
        for entry in new_entries {
            broadcast(hub, &trace_sse(&Frame::from_entry(entry)));
        }
        cursor = cursor.saturating_add(new_entries.len());

        slice = slice.saturating_add(1);
        if slice.is_multiple_of(4) {
            broadcast(hub, &tick_sse(&sim, wall_start));
        }
        if slice.is_multiple_of(40) {
            broadcast(hub, &invariant_sse(sim.trace()));
        }

        let pace = if busy {
            cfg.speed
        } else {
            cfg.speed * IDLE_ACCELERATION
        };
        thread::sleep(SLICE.div_f64(pace));
    }
}

/// The `meta` frame: everything the renderer needs to draw the stage before the first entry.
fn meta_sse(cfg: &Config, nodes: &[viz::NodeMeta], links: &[viz::LinkMeta]) -> String {
    sse(
        "meta",
        None,
        &serde_json::json!({
            "v": 1,
            "scenario": "register-call",
            "seed": format!("0x{:016x}", cfg.seed),
            "weather": cfg.links,
            "nodes": nodes.iter().map(|node| serde_json::json!({
                "id": node.id.index(),
                "name": node.name,
                "role": node.role.as_str(),
            })).collect::<Vec<_>>(),
            "links": links.iter().map(|link| serde_json::json!({
                "from": link.from.index(),
                "to": link.to.index(),
                "kind": if link.kind == LinkKind::Datagram { "datagram" } else { "stream" },
            })).collect::<Vec<_>>(),
        }),
    )
}

/// One trace entry as a frame. The frame *is* the entry (`Frame::from_entry`), so the stream and
/// the trace cannot diverge — there is no second event model on this path.
fn trace_sse(frame: &Frame) -> String {
    sse(
        frame.kind.as_str(),
        Some(frame.seq),
        &serde_json::json!({
            "v": 1,
            "at": frame.at.as_nanos(),
            "seq": frame.seq,
            "node": frame.node.index(),
            "node_name": frame.node_name,
            "kind": frame.kind.as_str(),
            "peer": frame.peer.map(sipx_clstr_sim::NodeId::index),
            "summary": frame.summary,
            "timer": frame.timer.map(|timer| timer.0),
            "timer_at": frame.timer_at.map(SimTime::as_nanos),
        }),
    )
}

/// The `tick` frame: virtual time against wall time, so the page can show the harness's
/// fast-forward rather than hide it.
fn tick_sse(sim: &sipx_clstr_sim::Sim, wall_start: Instant) -> String {
    let virtual_elapsed = sim.now().since(SimTime::START).as_secs_f64();
    let wall_elapsed = wall_start.elapsed().as_secs_f64();
    sse(
        "tick",
        None,
        &serde_json::json!({
            "v": 1,
            "virtual": virtual_elapsed,
            "wall": wall_elapsed,
            "ratio": if wall_elapsed > 0.0 { virtual_elapsed / wall_elapsed } else { 0.0 },
        }),
    )
}

/// The `invariant` frame: the DP-3 counter set as trace queries, uninstrumented where the sim
/// has no source — never a pretend zero.
fn invariant_sse(trace: &sipx_clstr_sim::Trace) -> String {
    sse(
        "invariant",
        None,
        &serde_json::json!({
            "v": 1,
            "counters": viz::invariants(trace).iter().map(|counter| {
                serde_json::json!({ "name": counter.name, "value": counter.value })
            }).collect::<Vec<_>>(),
        }),
    )
}

fn broadcast(hub: &Arc<Mutex<Hub>>, frame: &str) {
    hub.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .broadcast(frame);
}

/// One SSE frame: `event:` names the stage handler, `id:` (on trace frames only) is the trace
/// `seq`, `data:` one JSON object, single line — SSE forbids embedded newlines and `json!`
/// output has none.
fn sse(event: &str, id: Option<u64>, data: &serde_json::Value) -> String {
    use std::fmt::Write as _;

    let mut out = format!("event: {event}\n");
    if let Some(id) = id {
        let _ = writeln!(out, "id: {id}");
    }
    out.push_str("data: ");
    out.push_str(&data.to_string());
    out.push_str("\n\n");
    out
}

fn policy_of(name: &str) -> Option<LinkPolicy> {
    match name {
        "clean" => Some(LinkPolicy::CLEAN),
        "jittery" => Some(LinkPolicy::jittery(5, 60)),
        "storm" => Some(
            LinkPolicy::jittery(5, 80)
                .with_loss(0.15)
                .with_duplication(0.2),
        ),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------
// the hub: every client's resync point is the full backlog, because the trace is retained
// ------------------------------------------------------------------------------------------

/// Frames so far, plus the live subscribers. A dev tool can keep the whole run in memory — the
/// trace itself is retained the same way.
#[derive(Debug, Default)]
struct Hub {
    backlog: Vec<String>,
    senders: Vec<SyncSender<String>>,
}

impl Hub {
    fn broadcast(&mut self, frame: &str) {
        self.backlog.push(frame.to_owned());
        // Full or closed queues are dropped: the client's forwarding loop then sees the channel
        // hang up, closes the socket, and the browser's EventSource reconnects into a full
        // catch-up from the backlog. Resync rather than silent loss — and the `id:` on every
        // trace frame makes any hole visible regardless.
        self.senders
            .retain(|sender| sender.try_send(frame.to_owned()).is_ok());
    }

    fn subscribe(&mut self) -> (Vec<String>, Receiver<String>) {
        let (sender, receiver) = mpsc::sync_channel(CLIENT_QUEUE);
        self.senders.push(sender);
        (self.backlog.clone(), receiver)
    }
}

// ------------------------------------------------------------------------------------------
// the HTTP server: std only — this is a localhost dev tool, and SSE is plain text over HTTP
// ------------------------------------------------------------------------------------------

fn serve(cfg: &Config, hub: &Arc<Mutex<Hub>>) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", cfg.port))?;
    eprintln!(
        "constellation: http://127.0.0.1:{}/  (seed 0x{:x}, {} links, {}x)",
        cfg.port, cfg.seed, cfg.links, cfg.speed
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let hub = Arc::clone(hub);
                thread::spawn(move || {
                    let _ = handle(stream, &hub);
                });
            }
            Err(error) => eprintln!("viz: accept failed: {error}"),
        }
    }
    Ok(())
}

fn handle(stream: TcpStream, hub: &Arc<Mutex<Hub>>) -> std::io::Result<()> {
    let head = read_request_head(&stream)?;
    let Some(path) = head.first().map(String::as_str).and_then(request_path) else {
        return Ok(());
    };
    match path.as_str() {
        "/" | "/index.html" => respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        ),
        "/healthz" => respond(stream, "200 OK", "text/plain; charset=utf-8", b"ok\n"),
        "/events" => stream_events(stream, hub),
        _ => respond(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found\n",
        ),
    }
}

/// The request line and headers, capped — this server answers three routes and reads nothing
/// else, so anything beyond a small head is the client talking to someone else.
fn read_request_head(stream: &TcpStream) -> std::io::Result<Vec<String>> {
    let mut reader = BufReader::new(stream.try_clone()?.take(16 * 1024));
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end().to_owned();
        if line.is_empty() {
            break;
        }
        lines.push(line);
        if lines.len() >= 64 {
            break;
        }
    }
    Ok(lines)
}

fn request_path(request_line: &str) -> Option<String> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    if method != "GET" {
        return None;
    }
    Some(path.to_owned())
}

fn respond(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

fn stream_events(mut stream: TcpStream, hub: &Arc<Mutex<Hub>>) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n",
    )?;
    let (backlog, receiver) = hub
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .subscribe();
    for frame in &backlog {
        stream.write_all(frame.as_bytes())?;
    }
    stream.flush()?;
    // A dead or resyncing client hangs the channel up; a write to a gone client errors. Either
    // way this thread's only job is over.
    loop {
        match receiver.recv() {
            Ok(frame) => {
                stream.write_all(frame.as_bytes())?;
                stream.flush()?;
            }
            Err(_) => return Ok(()),
        }
    }
}

// ------------------------------------------------------------------------------------------
// arguments
// ------------------------------------------------------------------------------------------

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config {
        seed: 0x0000_c0ff_ee11,
        speed: 1.0,
        links: "jittery".to_owned(),
        port: 8975,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            return Err(USAGE.to_owned());
        }
        let Some(value) = args.next() else {
            return Err(format!("{arg} needs a value\n{USAGE}"));
        };
        match arg.as_str() {
            "--seed" => cfg.seed = parse_seed(&value)?,
            "--speed" => {
                cfg.speed = value
                    .parse::<f64>()
                    .map_err(|_| format!("--speed wants a number, got {value:?}"))?;
                if !matches!(
                    cfg.speed.partial_cmp(&0.0),
                    Some(std::cmp::Ordering::Greater)
                ) {
                    return Err(format!("--speed must be positive, got {value:?}"));
                }
            }
            "--links" => {
                if policy_of(&value).is_none() {
                    return Err(format!("--links wants clean|jittery|storm, got {value:?}"));
                }
                cfg.links = value;
            }
            "--port" => {
                cfg.port = value
                    .parse::<u16>()
                    .map_err(|_| format!("--port wants 0-65535, got {value:?}"))?;
            }
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    Ok(cfg)
}

fn parse_seed(text: &str) -> Result<u64, String> {
    let parsed = if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        text.parse::<u64>()
    };
    parsed.map_err(|_| format!("--seed wants a u64 (decimal or 0x…), got {text:?}"))
}
