//! `PX-9` — a fork's live branch must be heard while a dead branch is still silent.
//!
//! The engine forks correctly and emits one `Forward` per target (proxy-behavior §7 L4, RFC 3261
//! §16.6). The driver then had to *consume* those branches, and it consumed them one at a time:
//! `while let Some((branch, mut responses)) = pending.pop()` with an inner loop that ran each
//! stream to exhaustion. A stream only ends when its transaction does, so the first branch popped
//! owned the task until it concluded — and a branch pointing at a device that never answers does
//! not conclude until the kernel's Timer B, 64·T1 ≈ 32 s.
//!
//! What a user sees is two registered devices, one of them unreachable: roughly thirty seconds of
//! silence, and then the `200 OK` that had been sitting unread in the live branch's stream all
//! along.
//!
//! **Why this is a real-socket test and not a harness scenario.** `sipx-clstr-sim` models a node
//! sans-IO — it has no dependency on this crate and never runs `driver.rs` — so the defect is not
//! reachable from it at all. The bug lives in how a driver with real streams sequences them, and
//! the driver is the one layer allowed a socket (AGENTS.md #2). Four UDP endpoints:
//!
//! - the **node**, edge and registrar, from the same kind of cluster document the other driver
//!   tests use;
//! - a **live device**, which answers every INVITE `200 OK` at once;
//! - a **black hole**, bound so that nothing produces an ICMP refusal, and reading its socket so
//!   that nothing produces a full-buffer one either. It answers nothing, ever;
//! - the **caller**, which registers both contacts for one AoR and then calls it.
//!
//! **Both registration orders are exercised**, in two tests, because which branch the merge base
//! drained first was decided by the lookup's §7 L3 order (descending `q`, then *descending*
//! `refreshed_at`) and by `pop()` taking the back of the vector: the branch registered **first**
//! sorted last and was therefore drained first. So the black-hole-first orientation is the one that
//! fails at the merge base, and the live-first orientation is the one that passed there by luck of
//! ordering. A fix must make both fast, and pinning both is what stops the next change from
//! re-introducing the defect for one order only.
//!
//! The assertion is on **elapsed time to the relayed `200`**, with a budget an order of magnitude
//! below Timer B. It has to be real time — real sockets and the kernel's real timers are the whole
//! point — so the margin is deliberately wide in both directions: the answer arrives in
//! milliseconds when the branches are driven concurrently, the budget is seconds, and the defect
//! costs tens of seconds. Nothing in between is a plausible outcome, so nothing in between can make
//! this flaky.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use tokio::net::UdpSocket;

use sipx_clstr_node::config::{NodeIdentity, Role};
use sipx_clstr_node::driver::NodeConfig;
use sipx_clstr_node::startup;

/// How long the caller will wait for the live device's `200 OK`.
///
/// Eight times what a concurrent driver needs and eight times *below* the kernel's Timer B, so the
/// verdict is never a close call. See the module note on why this is real time.
const RELAY_BUDGET: Duration = Duration::from_secs(4);

/// The kernel's Timer B, 64·T1 — what the defect cost, and what the budget is measured against.
const TIMER_B: Duration = Duration::from_secs(32);

/// What the node says it is reached at. **Advertised, never bound** — see `tests/support/mod.rs`.
/// Nothing here routes to it: both devices answer the source address they received from, and the
/// caller addresses the node by the address the node reports.
const ADVERTISED: &str = "127.0.0.1:15091";

/// A cluster document for one edge/registrar node.
///
/// The listener binds `127.0.0.1:0` (`CF-13`). These two tests used to take `15091` and `15092`,
/// chosen to dodge `admission_bound.rs` — and `auth_observable.rs` then chose the same pair, which is
/// the reason that workaround was replaced rather than extended.
fn document() -> String {
    format!(
        "
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: px9
  environment: dev
  zones: [a]
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

/// The node this scenario runs, from the document rather than from a literal.
fn node_config(tag: &str) -> NodeConfig {
    let path = support::write_document(&document(), tag);
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: [Role::Edge, Role::Registrar].into_iter().collect(),
    };
    let env = std::collections::BTreeMap::new();
    startup::from_document(&path, &identity, &env).expect("the document describes a node")
}

// ------------------------------------------------------------------------------- the messages ---
//
// Written as text, like `tests/admission_bound.rs`, because what this test is about is what goes
// over a socket. The node's real parser reads them, which is the point.

/// A REGISTER binding `contact_port` to `sip:bob@example.test`.
///
/// `round` varies the branch, the `Call-ID` and the `CSeq` so that every one of these is a new
/// server transaction rather than a retransmission the kernel answers out of its store. Two
/// different contacts are two independent bindings (location-service §5), so neither one's `CSeq`
/// says anything about the other's.
fn register(from_port: u16, contact_port: u16, round: u32) -> String {
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px9-reg-{contact_port}-{round}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:bob@example.test>;tag=px9reg{contact_port}{round}\r\n\
         To: <sip:bob@example.test>\r\n\
         Call-ID: {}\r\n\
         CSeq: {} REGISTER\r\n\
         Contact: <sip:bob@127.0.0.1:{contact_port}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n",
        register_call_id(contact_port, round),
        round + 1,
    )
}

fn register_call_id(contact_port: u16, round: u32) -> String {
    format!("px9-register-{contact_port}-{round}")
}

/// The `Call-ID` of the one call this scenario places.
const CALL_ID: &str = "px9-the-call";

fn invite(from_port: u16) -> String {
    format!(
        "INVITE sip:bob@example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px9-invite\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:alice@example.test>;tag=px9alice\r\n\
         To: <sip:bob@example.test>\r\n\
         Call-ID: {CALL_ID}\r\n\
         CSeq: 1 INVITE\r\n\
         Contact: <sip:alice@127.0.0.1:{from_port}>\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// A `200 OK` for `request`, echoing what a response has to echo.
///
/// Built by copying the request's own header lines rather than by composing new ones: the `Via`
/// stack is what the kernel matches the response to its client transaction on, and a device that
/// invented its own would be testing this test rather than the node.
fn answer_200(request: &str) -> String {
    let mut lines = vec!["SIP/2.0 200 OK".to_owned()];
    for line in request.lines() {
        let name = line.to_ascii_lowercase();
        if name.starts_with("via:")
            || name.starts_with("record-route:")
            || name.starts_with("from:")
            || name.starts_with("call-id:")
            || name.starts_with("cseq:")
        {
            lines.push(line.to_owned());
        } else if name.starts_with("to:") {
            // The callee's tag. A 2xx without one is not a dialog.
            lines.push(format!("{line};tag=px9bob"));
        }
    }
    lines.push("Content-Length: 0".to_owned());
    lines.push(String::new());
    lines.push(String::new());
    lines.join("\r\n")
}

fn call_id(datagram: &str) -> Option<String> {
    datagram
        .lines()
        .find_map(|line| line.strip_prefix("Call-ID:"))
        .map(|value| value.trim().to_owned())
}

// -------------------------------------------------------------------------------- the devices ---

/// A device that answers every INVITE `200 OK` immediately, and nothing else.
fn live_device(socket: UdpSocket) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = vec![0u8; 8192];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
                return;
            };
            let datagram =
                String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
            if !datagram.starts_with("INVITE ") {
                continue;
            }
            let _ = socket.send_to(answer_200(&datagram).as_bytes(), from).await;
        }
    })
}

/// A device that is reachable and mute — the dead phone.
///
/// It reads and discards so that the failure being modelled is "no answer" rather than "the socket
/// buffer filled up": the kernel retransmits an unanswered INVITE (Timer A) until Timer B, and a
/// full buffer could turn that into an ICMP error, which is a *transport* failure and concludes the
/// branch early. That would make the defect invisible.
fn black_hole(socket: UdpSocket) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = vec![0u8; 8192];
        while socket.recv_from(&mut buffer).await.is_ok() {}
    })
}

// -------------------------------------------------------------------------------- the scenario ---

/// Whether a `200` for `round`'s REGISTER of `contact_port` arrives within `budget`.
async fn answered_register(
    socket: &UdpSocket,
    contact_port: u16,
    round: u32,
    budget: Duration,
) -> bool {
    let wanted = register_call_id(contact_port, round);
    let deadline = tokio::time::Instant::now() + budget;
    let mut buffer = vec![0u8; 8192];
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

/// Register `contact_port` for `sip:bob@example.test`, retrying until the node is serving.
async fn register_contact(caller: &UdpSocket, node: &str, caller_port: u16, contact_port: u16) {
    for round in 0..60u32 {
        caller
            .send_to(register(caller_port, contact_port, round).as_bytes(), node)
            .await
            .expect("send REGISTER");
        if answered_register(caller, contact_port, round, Duration::from_millis(150)).await {
            return;
        }
    }
    panic!("the node never answered a REGISTER for contact port {contact_port}");
}

/// How long the caller waited for the relayed `200 OK`, or `None` if it never came.
async fn time_to_answer(caller: &UdpSocket, budget: Duration) -> Option<Duration> {
    let started = tokio::time::Instant::now();
    let deadline = started + budget;
    let mut buffer = vec![0u8; 8192];
    while let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(remaining, caller.recv_from(&mut buffer)).await {
            Ok(Ok((len, _))) => {
                let datagram =
                    String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
                if datagram.starts_with("SIP/2.0 200")
                    && call_id(&datagram).as_deref() == Some(CALL_ID)
                {
                    return Some(started.elapsed());
                }
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => return None,
        }
    }
    None
}

/// Which of the two contacts is registered first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum First {
    /// The dead device registers first — the orientation the merge base drains first, and fails on.
    BlackHole,
    /// The live device registers first.
    Live,
}

/// One forked call with one live device and one black hole. Returns how long the `200` took.
async fn forked_call(tag: &str, first: First) -> Option<Duration> {
    let live = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the live device");
    let live_port = live.local_addr().expect("the live device's address").port();
    let dead = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the black hole");
    let dead_port = dead.local_addr().expect("the black hole's address").port();
    let caller = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the caller");
    let caller_port = caller.local_addr().expect("the caller's address").port();

    let live_task = live_device(live);
    let dead_task = black_hole(dead);
    let running = support::start_in_process(node_config(tag)).await;
    let node = running.target();

    // Registration order is what decides the merge base's drain order; see the module note.
    let (early, late) = match first {
        First::BlackHole => (dead_port, live_port),
        First::Live => (live_port, dead_port),
    };
    register_contact(&caller, &node, caller_port, early).await;
    register_contact(&caller, &node, caller_port, late).await;

    caller
        .send_to(invite(caller_port).as_bytes(), &node)
        .await
        .expect("send INVITE");

    let elapsed = time_to_answer(&caller, RELAY_BUDGET).await;

    running.stop();
    live_task.abort();
    dead_task.abort();
    elapsed
}

/// **The failing-first test for `PX-9`.** The dead device registered first, so the merge base
/// drained its branch first and the live device's `200` waited behind Timer B.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn px9_a_dead_branch_does_not_delay_a_live_ones_answer() {
    let elapsed = forked_call("black-hole-first", First::BlackHole).await;
    // Printed rather than only asserted: the number *is* the evidence, and a run that passed for
    // the wrong reason is visible here rather than inferred.
    println!("black-hole-first: 200 OK relayed after {elapsed:?} (budget {RELAY_BUDGET:?})");
    let elapsed = elapsed.unwrap_or_else(|| {
        panic!(
            "no 200 OK reached the caller within {RELAY_BUDGET:?}; the live device answered at \
             once, so the answer was waiting in a stream the driver had not read — Timer B is \
             {TIMER_B:?}"
        )
    });
    assert!(
        elapsed < RELAY_BUDGET,
        "the live device's 200 OK took {elapsed:?}; a fork must not serialize on its slowest \
         branch, and Timer B is {TIMER_B:?}"
    );
}

/// The other orientation, which passed at the merge base by luck of ordering, and must keep
/// passing: a fix that only helped one drain order would leave half of the two-device users waiting.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn px9_the_live_branch_is_still_relayed_when_it_registered_first() {
    let elapsed = forked_call("live-first", First::Live).await;
    println!("live-first: 200 OK relayed after {elapsed:?} (budget {RELAY_BUDGET:?})");
    let elapsed =
        elapsed.unwrap_or_else(|| panic!("no 200 OK reached the caller within {RELAY_BUDGET:?}"));
    assert!(
        elapsed < RELAY_BUDGET,
        "the live device's 200 OK took {elapsed:?}, and Timer B is {TIMER_B:?}"
    );
}
