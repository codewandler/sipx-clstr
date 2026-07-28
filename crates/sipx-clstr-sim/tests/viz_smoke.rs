//! The constellation end-to-end smoke test: spawn the real `viz` example server and prove the
//! whole path without a browser — `/healthz`, the embedded page, and the live SSE frame stream
//! (`meta`, `tick`, `invariant`, and trace frames that are trace entries serialized), including
//! backlog resync for a client that joins late.
//!
//! Run it: `cargo test -p sipx-clstr-sim viz_smoke`. `cargo test` builds examples as part of its
//! default target set, and the test locates the binary next to itself, so no separate build step
//! is needed. The weather is `jittery`, not `storm`: a smoke test proves the stream, and must not
//! depend on where a seeded loss pattern happens to land.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Generous: at 16× the showcase scenario plays out in a second or two of wall time.
const DEADLINE: Duration = Duration::from_secs(20);

#[test]
fn viz_smoke_the_server_serves_the_page_and_streams_the_trace() {
    let port = free_port();
    let mut server = Server::start(port);

    // /healthz — polled until the listener accepts, which is also the startup barrier.
    let health = poll_healthz(port);
    assert!(health.contains("200 OK"), "healthz head: {health}");
    assert!(health.contains("ok"), "healthz body: {health}");

    // / — the embedded page, wired to the stream.
    let page = http_get(port, "/").expect("GET /");
    assert!(page.contains("200 OK"), "page head");
    assert!(page.contains("text/html"), "page content type");
    assert!(page.contains("<canvas id=\"stage\">"), "the stage canvas");
    assert!(
        page.contains("EventSource"),
        "the page subscribes to the stream"
    );

    // /events — the live feed. Read until the stream has shown every feed-level frame and the
    // scenario is visibly flowing (a send and a delivery), then assert on what arrived.
    let feed = read_stream(open_events(port), |text| {
        let kinds = frame_kinds(text);
        kinds.contains(&"meta")
            && kinds.contains(&"tick")
            && kinds.contains(&"invariant")
            && kinds.contains(&"started")
            && kinds.contains(&"sent")
            && kinds.contains(&"received")
    });
    assert_meta(&feed);
    assert_trace_frames(&feed);
    assert_tick_and_invariant(&feed);

    // A late client resyncs from the full backlog: the first frame it sees is the meta frame.
    let late = read_stream(open_events(port), |text| text.contains("\n\n"));
    let first = late
        .split("\n\n")
        .find(|block| !block.is_empty())
        .unwrap_or_default();
    assert!(
        first.contains("event: meta"),
        "a late client's first frame is the meta frame, got: {first}"
    );

    server.stop();
    eprintln!("--- smoke evidence ---\n{}", evidence(&feed));
}

// ---------------------------------------------------------------------------------------------
// assertions, one per route family
// ---------------------------------------------------------------------------------------------

/// The `meta` frame carries the whole stage: scenario, seed, weather, nodes with roles, links.
fn assert_meta(feed: &str) {
    let (event, _, data) = frames(feed)
        .into_iter()
        .find(|(event, _, _)| event == "meta")
        .expect("a meta frame");
    assert_eq!(event, "meta");
    assert_eq!(data["v"], 1, "schema version from day one");
    assert_eq!(data["scenario"], "register-call");
    let seed = data["seed"].as_str().unwrap_or_default();
    assert!(
        seed.starts_with("0x"),
        "the master seed is on the wire: {seed}"
    );
    assert_eq!(data["weather"], "jittery");

    let nodes = data["nodes"].as_array().expect("nodes is an array");
    assert_eq!(nodes.len(), 4, "edge + alice + two of bob's devices");
    let edge = nodes.iter().filter(|n| n["role"] == "edge").count();
    let wings = nodes.iter().filter(|n| n["role"] == "endpoint").count();
    assert_eq!((edge, wings), (1, 3), "one platform node, three endpoints");
    assert!(
        nodes
            .iter()
            .all(|n| n["name"].is_string() && n["id"].is_number()),
        "every node has a name and an id"
    );

    let links = data["links"].as_array().expect("links is an array");
    assert_eq!(links.len(), 3, "one link per endpoint");
    assert!(
        links.iter().all(|l| l["kind"] == "datagram"),
        "the showcase runs on datagram links"
    );
}

/// Trace frames are trace entries on the wire: `id:` is the entry's `seq`, the payload carries
/// the envelope verbatim, and order is total. The stop condition guaranteed these kinds arrived.
fn assert_trace_frames(feed: &str) {
    let trace_kinds = [
        "started",
        "sent",
        "received",
        "dropped",
        "duplicated",
        "broken",
        "malformed",
        "timer_set",
        "timer_fired",
        "timer_cleared",
        "note",
    ];
    let trace: Vec<(String, Option<u64>, serde_json::Value)> = frames(feed)
        .into_iter()
        .filter(|(event, _, _)| trace_kinds.contains(&event.as_str()))
        .collect();
    assert!(
        trace.len() >= 3,
        "the scenario is flowing: {} trace frames",
        trace.len()
    );

    let mut last_seq: Option<u64> = None;
    for (event, id, data) in &trace {
        assert_eq!(data["v"], 1, "{event}: schema version");
        let seq = data["seq"].as_u64().expect("seq is a u64");
        assert_eq!(*id, Some(seq), "{event}: the SSE id is the trace seq");
        if let Some(prev) = last_seq {
            assert!(seq > prev, "the stream is totally ordered");
        }
        last_seq = Some(seq);
        assert!(data["at"].is_number(), "{event}: virtual time");
        assert!(data["node"].is_number(), "{event}: node id");
        assert!(data["node_name"].is_string(), "{event}: node name");
        assert_eq!(
            data["kind"].as_str(),
            Some(event.as_str()),
            "kind matches the event name"
        );
    }

    let sent = trace
        .iter()
        .find(|(event, _, _)| event == "sent")
        .expect("a sent frame");
    let summary = sent.2["summary"].as_str().unwrap_or_default();
    assert!(
        summary.contains("REGISTER"),
        "the first send is a REGISTER: {summary}"
    );
}

/// The `tick` frame shows the harness's clocks; the `invariant` frame carries the DP-3 counter
/// set with uninstrumented counters present and null — never a pretend zero.
fn assert_tick_and_invariant(feed: &str) {
    let tick = frames(feed)
        .into_iter()
        .find(|(event, _, _)| event == "tick")
        .map(|(_, _, data)| data)
        .expect("a tick frame");
    assert!(tick["virtual"].is_number() && tick["wall"].is_number());
    assert!(tick["ratio"].as_f64().unwrap_or_default() > 0.0);

    let invariant = frames(feed)
        .into_iter()
        .find(|(event, _, _)| event == "invariant")
        .map(|(_, _, data)| data)
        .expect("an invariant frame");
    let counters = invariant["counters"]
        .as_array()
        .expect("counters is an array");
    let by_name = |name: &str| counters.iter().find(|c| c["name"] == name);
    assert_eq!(
        by_name("cross_node_dialog_lookups").and_then(|c| c["value"].as_u64()),
        Some(0),
        "instrumented and clean: a real zero"
    );
    assert!(
        by_name("token_verification_failures").is_some_and(|c| c["value"].is_null()),
        "no source yet: uninstrumented, present and null"
    );
}

// ---------------------------------------------------------------------------------------------
// frame picking and the evidence printout (visible with --nocapture)
// ---------------------------------------------------------------------------------------------

/// Split the raw stream into `(event, id, data-json)` triples; blocks without a `data:` line
/// (the HTTP head) are skipped.
fn frames(feed: &str) -> Vec<(String, Option<u64>, serde_json::Value)> {
    feed.split("\n\n")
        .filter_map(|block| {
            let mut event = None;
            let mut id = None;
            let mut data = None;
            for line in block.lines() {
                if let Some(value) = line.strip_prefix("event: ") {
                    event = Some(value.to_owned());
                } else if let Some(value) = line.strip_prefix("id: ") {
                    id = value.parse::<u64>().ok();
                } else if let Some(value) = line.strip_prefix("data: ") {
                    data = serde_json::from_str(value).ok();
                }
            }
            Some((event?, id, data?))
        })
        .collect()
}

fn frame_kinds(feed: &str) -> Vec<&str> {
    feed.lines()
        .filter_map(|line| line.strip_prefix("event: "))
        .collect()
}

/// What a human gets to see in the test log: the stage, the frame census, and the HUD readings.
fn evidence(feed: &str) -> String {
    use std::fmt::Write as _;

    let parsed = frames(feed);
    let mut out = String::new();

    let meta = parsed.iter().find(|(event, _, _)| event == "meta");
    if let Some((_, _, data)) = meta {
        let nodes = data["nodes"].as_array().map_or(0, Vec::len);
        let links = data["links"].as_array().map_or(0, Vec::len);
        let _ = writeln!(
            out,
            "meta: scenario={} seed={} nodes={nodes} links={links}",
            data["scenario"], data["seed"]
        );
    }

    let mut census: Vec<(&str, usize)> = Vec::new();
    for kind in frame_kinds(feed) {
        if let Some(entry) = census.iter_mut().find(|(known, _)| *known == kind) {
            entry.1 += 1;
        } else {
            census.push((kind, 1));
        }
    }
    let census = census
        .iter()
        .map(|(frame_kind, count)| format!("{frame_kind}×{count}"))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(out, "frames: {census}");

    if let Some((_, _, data)) = parsed.iter().find(|(event, _, _)| event == "sent") {
        let _ = writeln!(out, "first send: {}", data["summary"]);
    }
    if let Some((_, _, data)) = parsed.iter().find(|(event, _, _)| event == "tick") {
        let _ = writeln!(
            out,
            "tick: virtual={} wall={} ratio={}",
            data["virtual"], data["wall"], data["ratio"]
        );
    }
    if let Some((_, _, data)) = parsed.iter().find(|(event, _, _)| event == "invariant") {
        let _ = writeln!(out, "invariants: {}", data["counters"]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// the server under test: spawn, poll, stop
// ---------------------------------------------------------------------------------------------

/// The running example. Killed on drop so a failed assertion never orphans a listener.
#[derive(Debug)]
struct Server {
    child: Child,
}

impl Server {
    fn start(port: u16) -> Self {
        let child = Command::new(example_binary())
            .args(["--port", &port.to_string()])
            .args(["--links", "jittery", "--speed", "16"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the viz example");
        Self { child }
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The example binary sits next to the test binary's directory: `target/<profile>/examples/viz`.
/// `cargo test` builds examples as part of its default target set, so a plain test run suffices.
fn example_binary() -> PathBuf {
    let test_exe = std::env::current_exe().expect("the test binary's own path");
    let examples = test_exe
        .parent()
        .and_then(std::path::Path::parent)
        .map(|profile| profile.join("examples"))
        .expect("the test binary lives in target/<profile>/deps");
    let binary = examples.join(format!("viz{}", std::env::consts::EXE_SUFFIX));
    assert!(
        binary.is_file(),
        "the viz example is not built at {}; run `cargo build -p sipx-clstr-sim --example viz`",
        binary.display()
    );
    binary
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("the bound address")
        .port()
}

// ---------------------------------------------------------------------------------------------
// minimal HTTP: fixed-length GETs, and the streaming GET with a done condition
// ---------------------------------------------------------------------------------------------

/// A plain GET against a `Connection: close` route: read to end, return head and body together.
fn http_get(port: u16, path: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(DEADLINE))?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")?;
    stream.flush()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn poll_healthz(port: u16) -> String {
    let deadline = Instant::now() + DEADLINE;
    loop {
        match http_get(port, "/healthz") {
            Ok(response) if response.contains("200 OK") => return response,
            _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            other => panic!("the server never came up on :{port}: {other:?}"),
        }
    }
}

/// Open the SSE route: the head arrives immediately, frames follow until the client goes away.
fn open_events(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to /events");
    stream
        .set_read_timeout(Some(Duration::from_millis(250)))
        .expect("a read timeout");
    stream
        .write_all(b"GET /events HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .expect("write the /events request");
    stream
}

/// Read frames until `done` says the stream has shown enough (or the deadline passes and the
/// assertions report what was missing). Read timeouts are the poll cadence, not a failure.
fn read_stream(mut stream: TcpStream, done: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + DEADLINE;
    let mut buf = Vec::new();
    while Instant::now() < deadline {
        let mut chunk = [0_u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Some(bytes) = chunk.get(..n) {
                    buf.extend_from_slice(bytes);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("reading /events: {error}"),
        }
        let text = String::from_utf8_lossy(&buf);
        if text.contains("text/event-stream") && done(&text) {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}
