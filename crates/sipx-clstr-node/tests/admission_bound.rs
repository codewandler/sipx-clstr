//! `DP-11` — a flood cannot put more transactions in flight than the node's bound.
//!
//! The driver spawns a task per new server transaction, and a proxied task lives for the whole
//! transaction — up to Timer B. So offered load converted directly into resident tasks, and the only
//! backpressure was the kernel's 1024-message queue, which bounds the *queue* and not concurrency.
//!
//! This is the scenario that says so. It runs the **real** node over a **real** socket, because an
//! admission bound is a property of the driver and the driver is the one layer allowed a socket
//! (AGENTS.md #2). Three UDP endpoints:
//!
//! - the **node**, from a cluster document that declares the bound;
//! - the **sink**, the callee's registered contact. It answers nothing, deliberately: an admitted
//!   INVITE then stays in flight until Timer B, which is what makes "in flight" observable at all;
//! - the **caller**, which registers the sink and then floods.
//!
//! The count is over *distinct transactions*, keyed by `Call-ID`, not over datagrams. The kernel
//! retransmits an unanswered INVITE (Timer A) and an unacknowledged `503` (Timer G), so counting
//! packets would count the specification rather than the load.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use tokio::net::UdpSocket;

use sipx_clstr_node::config::{NodeIdentity, Role};
use sipx_clstr_node::driver::NodeConfig;
use sipx_clstr_node::startup;

/// The bound the document declares. Small on purpose: the assertion is about the ceiling existing,
/// and a small ceiling is a fast test.
const BOUND: usize = 8;

/// What is offered. Five times the bound, so the merge base's "in flight tracks offered load" is not
/// a near miss but an obvious one, and so a lost datagram or two cannot rescue it.
const OFFERED: usize = 40;

/// What the node says it is reached at. **Advertised, never bound** — see `tests/support/mod.rs` on
/// why the two are allowed to differ here and why only the bind could ever collide. Nothing in this
/// scenario routes to it: the sink answers nothing at all, and the caller addresses the node by the
/// address the node reports.
const ADVERTISED: &str = "127.0.0.1:15081";

/// A cluster document for one edge/registrar node with an admission bound.
///
/// The listener binds `127.0.0.1:0` (`CF-13`): the kernel picks the port and the node reports it, so
/// two suites running at once cannot land on each other.
fn document(bound: usize) -> String {
    format!(
        "
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: dp11
  environment: dev
  zones: [a]
  admission:
    maxInFlightTransactions: {bound}
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: {bind}
      advertise: {ADVERTISED}
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [example.test]
",
        bind = support::EPHEMERAL,
    )
}

/// The node configuration this scenario runs, and whether the **document** supplied it.
///
/// The bound is declared in the document, which is where the acceptance puts it. A build that cannot
/// express it there falls back to the node exactly as it was — deliberately, so that what this test
/// reports is the *ceiling* rather than a configuration error. At the merge base that is the path
/// taken, and the ceiling assertion fails with the offered load as its count. It is also the right
/// behaviour for a regression: a knob that stopped being read would leave an unbounded node, and an
/// unbounded node is exactly what the assertion is looking for.
///
/// The fallback cannot hide, either: the scenario asserts the flag — **after** the ceiling, so that a
/// build with no bound at all fails on the bound rather than on the configuration.
fn node_config(tag: &str, bound: usize) -> (NodeConfig, bool) {
    let path = support::write_document(&document(bound), tag);
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: [Role::Edge, Role::Registrar].into_iter().collect(),
    };
    let env = std::collections::BTreeMap::new();
    match startup::from_document(&path, &identity, &env) {
        Ok(config) => (config, true),
        Err(error) => {
            println!(
                "the document could not express an admission bound ({error}); \
                 running the node as it was"
            );
            // `advertising` rather than `new`: an ephemeral bind has no port to advertise yet, and
            // `Advertised` refuses port zero. The fallback binds the same nothing-in-particular the
            // document does, so this path cannot collide either.
            let config =
                NodeConfig::advertising(support::ephemeral(), ADVERTISED).expect("a loopback node");
            (config, false)
        }
    }
}

// ------------------------------------------------------------------------------- the messages ---
//
// Written as text rather than built through `RequestBuilder`, because what this test is about is
// what goes over a socket. The real parser reads them, which is the point.

/// A REGISTER for the sink's contact.
///
/// `round` varies the branch, the `Call-ID` and the `CSeq`, so every one of these is a **new server
/// transaction** rather than a retransmission the kernel answers out of its transaction store. A
/// scenario that reused them would prove the kernel absorbs retransmissions, which is not the claim.
fn register(from_port: u16, sink_port: u16, round: u32) -> String {
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-dp11-reg-{round}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:sink@example.test>;tag=dp11reg{round}\r\n\
         To: <sip:sink@example.test>\r\n\
         Call-ID: {}\r\n\
         CSeq: {} REGISTER\r\n\
         Contact: <sip:sink@127.0.0.1:{sink_port}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n",
        register_call_id(round),
        round + 1,
    )
}

fn register_call_id(round: u32) -> String {
    format!("dp11-register-{round}")
}

fn invite(from_port: u16, index: usize) -> String {
    format!(
        "INVITE sip:sink@example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-dp11-{index}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:caller@example.test>;tag=dp11c{index}\r\n\
         To: <sip:sink@example.test>\r\n\
         Call-ID: dp11-flood-{index}\r\n\
         CSeq: 1 INVITE\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// The `Call-ID` of a message on the wire — the transaction's identity for counting purposes.
fn call_id(datagram: &str) -> Option<String> {
    datagram
        .lines()
        .find_map(|line| line.strip_prefix("Call-ID:"))
        .map(|value| value.trim().to_owned())
}

/// Whether a `200` for `round`'s REGISTER arrives within `budget`.
///
/// It reads past whatever else is queued on the socket, because under a flood the interesting
/// datagram is behind a pile of refusals and their retransmissions.
async fn answered_register(socket: &UdpSocket, round: u32, budget: Duration) -> bool {
    let wanted = register_call_id(round);
    let deadline = tokio::time::Instant::now() + budget;
    let mut buffer = vec![0u8; 4096];
    while let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
            Ok(Ok((len, _))) => {
                let datagram =
                    String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
                if datagram.starts_with("SIP/2.0 200")
                    && call_id(&datagram).as_deref() == Some(wanted.as_str())
                {
                    return true;
                }
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => return false,
        }
    }
    false
}

/// Collect datagrams from `socket` until nothing has arrived for `quiet`, or `budget` has elapsed.
async fn drain(socket: &UdpSocket, budget: Duration, quiet: Duration) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut seen = Vec::new();
    let mut buffer = vec![0u8; 4096];
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(quiet, socket.recv_from(&mut buffer)).await {
            Ok(Ok((len, _from))) => {
                let bytes = buffer.get(..len).unwrap_or_default();
                seen.push(String::from_utf8_lossy(bytes).into_owned());
            }
            // A read error on a bound loopback socket is not a case this scenario models.
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => break,
        }
    }
    seen
}

/// **The failing-first test for `DP-11`.** Offered load must not become resident work without limit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dp11_a_flood_cannot_exceed_the_admission_bound() {
    let sink = UdpSocket::bind("127.0.0.1:0").await.expect("bind the sink");
    let sink_port = sink.local_addr().expect("the sink's address").port();
    let caller = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the caller");
    let caller_port = caller.local_addr().expect("the caller's address").port();

    let (config, from_document) = node_config("flood", BOUND);
    // The node reports the port the kernel gave it, so nothing here has to know one in advance.
    let running = support::start_in_process(config).await;
    let node = running.target();

    // Register the sink. The `200` is the signal that the registrar is answering; the node is
    // already known to be bound, because `start_in_process` waited for it to say so.
    let mut registered = false;
    for round in 0..40u32 {
        caller
            .send_to(register(caller_port, sink_port, round).as_bytes(), &node)
            .await
            .expect("send REGISTER");
        if answered_register(&caller, round, Duration::from_millis(150)).await {
            registered = true;
            break;
        }
    }
    assert!(
        registered,
        "the node must answer a REGISTER before this scenario can flood it"
    );

    // The flood. One INVITE per transaction, as fast as the socket takes them.
    for index in 0..OFFERED {
        caller
            .send_to(invite(caller_port, index).as_bytes(), &node)
            .await
            .expect("send INVITE");
    }

    let (at_sink, at_caller) = tokio::join!(
        drain(&sink, Duration::from_secs(3), Duration::from_millis(400)),
        drain(&caller, Duration::from_secs(3), Duration::from_millis(400)),
    );

    // Admitted: a distinct transaction the node took on and forwarded. The sink never answers, so
    // each of these is still in flight when it is counted.
    let admitted: BTreeSet<String> = at_sink
        .iter()
        .filter(|datagram| datagram.starts_with("INVITE "))
        .filter_map(|datagram| call_id(datagram))
        .collect();

    let refused: BTreeSet<String> = at_caller
        .iter()
        .filter(|datagram| datagram.starts_with("SIP/2.0 503"))
        .filter_map(|datagram| call_id(datagram))
        .collect();

    running.stop();

    // Printed rather than only asserted: the numbers are the evidence, and a run that passes for the
    // wrong reason (nothing offered, nothing admitted) is visible here rather than inferred.
    println!(
        "offered {OFFERED}, admitted {}, refused {} (bound {BOUND})",
        admitted.len(),
        refused.len()
    );

    // The ceiling. This is the whole story: at the merge base the count tracks offered load.
    assert!(
        admitted.len() <= BOUND,
        "{} of {OFFERED} offered transactions were admitted at once; the bound is {BOUND}",
        admitted.len()
    );
    // …and a bound that admits nothing is not a bound, it is an outage.
    assert!(
        !admitted.is_empty(),
        "the node must still serve up to its bound; nothing was forwarded"
    );

    // Refusing over the bound is an answer, not a drop — the kernel's own shape.
    assert!(
        refused.len() >= OFFERED - BOUND - 4,
        "expected the excess to be refused with 503, got {} refusals for {OFFERED} offered",
        refused.len()
    );
    assert!(
        admitted.is_disjoint(&refused),
        "a transaction was both forwarded and refused"
    );
    let with_retry_after = at_caller
        .iter()
        .filter(|datagram| datagram.starts_with("SIP/2.0 503"))
        .filter(|datagram| datagram.contains("Retry-After:"))
        .count();
    let all_503 = at_caller
        .iter()
        .filter(|datagram| datagram.starts_with("SIP/2.0 503"))
        .count();
    assert_eq!(
        with_retry_after, all_503,
        "every 503 must carry Retry-After, as the kernel's own refusal does"
    );

    // Last, so that a build with no bound fails on the bound. The acceptance asks for the bound to be
    // configurable through the cluster document, and this is what says the document is where it came
    // from rather than a default this test happened to agree with.
    assert!(
        from_document,
        "the bound must be declared in the cluster document, not supplied by a fallback"
    );
}

/// A REGISTER is never held behind the bound — a registration storm *is* the overload.
///
/// The node is given a bound of 1 and then handed the whole flood; every one of those INVITEs is
/// either admitted or refused, and the registrar keeps answering throughout. A blanket cap in front
/// of the driver would fail this: the one thing a node under a registration storm most needs to do
/// is take registrations.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dp11_registration_survives_a_saturated_proxy_path() {
    let sink = UdpSocket::bind("127.0.0.1:0").await.expect("bind the sink");
    let sink_port = sink.local_addr().expect("the sink's address").port();
    let caller = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the caller");
    let caller_port = caller.local_addr().expect("the caller's address").port();

    let (config, _from_document) = node_config("saturated", 1);
    let running = support::start_in_process(config).await;
    let node = running.target();

    // Round 0 gets the registrar answering. Everything from round 1 arrives at a node whose bound is
    // spent.
    let mut up = false;
    for round in 0..40u32 {
        caller
            .send_to(register(caller_port, sink_port, round).as_bytes(), &node)
            .await
            .expect("send REGISTER");
        if answered_register(&caller, round, Duration::from_millis(150)).await {
            up = true;
            break;
        }
    }
    assert!(
        up,
        "the node must be serving before the proxy path is loaded"
    );

    for index in 0..OFFERED {
        caller
            .send_to(invite(caller_port, index).as_bytes(), &node)
            .await
            .expect("send INVITE");
    }

    let mut answered = 0u32;
    for round in 100..110u32 {
        caller
            .send_to(register(caller_port, sink_port, round).as_bytes(), &node)
            .await
            .expect("send REGISTER");
        if answered_register(&caller, round, Duration::from_millis(500)).await {
            answered += 1;
        }
    }
    running.stop();

    assert_eq!(
        answered, 10,
        "the registrar answered {answered} of 10 REGISTERs while the proxy path was saturated; \
         an admission bound must not sit in front of registration"
    );
}

/// The kernel's shed counters are read and **exported**, split the way the kernel splits them.
///
/// `Handle::shed()` existed and nothing in this repo called it: `outstanding()` was the only kernel
/// instrument used, and `website/docs/operate/scaling.md` names overload shed rate as the one number
/// that says the platform is past its limit. So this drives the **binary** and reads its **stderr**,
/// following `tests/startup_warns.rs`: a counter that is read into a variable and never emitted is a
/// counter nobody has, and no unit test can tell the difference.
///
/// **It waits for the line, it does not wait for a clock (`CF-13`).** This used to sleep a flat
/// 1500 ms against a 500 ms sampling tick and then assert on whatever had been written. Three ticks
/// of margin sounds ample and is not: under a parallel fan-out a debug-built node can lose that much
/// to scheduling before it emits anything, and an independent review watched this fail once and pass
/// on four subsequent runs. A test that fails one run in five trains people to re-run rather than to
/// read, which costs more than the test is worth. So the wait now ends on the *output* — as soon as
/// the load line has been emitted, and not before — which is both faster in the ordinary case and
/// indifferent to how loaded the machine is.
#[test]
fn dp11_the_shed_counters_reach_a_human() {
    let path = support::write_document(&document(BOUND), "shed-counters");
    let node = support::BinaryNode::start(&path);

    // The sampling tick is what this test is about, so the wait is for a sample rather than for a
    // duration. `shed_requests` is on the load line and on nothing else, so seeing it means the tick
    // has fired at least once — which is the whole precondition the sleep used to approximate.
    let _ = node.stderr_until(|seen| seen.contains("shed_requests"));
    // Stopping returns the same record, complete: the wait above decides *when* to read it, and this
    // decides *what* the assertions below get to quote.
    let stderr = node.stop();

    for field in [
        "shed_requests",
        "shed_acks",
        "shed_unmatched",
        "in_flight",
        "refused",
    ] {
        assert!(
            stderr.contains(field),
            "the load line must carry `{field}`. stderr was:\n{stderr}"
        );
    }
    // And the bound itself, so an operator can tell what the node will do before it does it.
    assert!(
        stderr.contains("max_in_flight_transactions"),
        "the node must say what its bound is. stderr was:\n{stderr}"
    );
}
