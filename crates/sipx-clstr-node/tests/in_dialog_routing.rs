//! `PX-13` — an in-dialog request goes where the dialog says, not where the registrar does.
//!
//! Validated review finding **V-03**. Every `ACK` went through a path that ignored `Route`, treated
//! the Request-URI as an address of record, took the first registration and dropped the request when
//! there was none; every other in-dialog method was preprocessed correctly by the pure engine and
//! then had its next hop resolved as an AoR lookup by the driver. A remote `Contact` is not an
//! address of record, so an ordinary call's `ACK` was dropped and its `BYE` was answered `480`.
//!
//! **Why the harness never caught it.** The simulation's `ACK`/`BYE` are AoR-shaped: their
//! Request-URI *is* the configured address of record, so the lookup resolves by accident. This test
//! is built so that accident is not available, and so that the wrong behaviour is *observed* rather
//! than inferred from an absence:
//!
//! - the address of record is `sip:bob@example.test` and the dialog's remote target is the contact
//!   `sip:bob@127.0.0.1:<port>`. An explicit port is part of a canonical AoR
//!   (location-service §3 N7), so that contact is a **different key** from anything registered for
//!   the call — the ordinary case, and the one that loses the `ACK`;
//! - and that key *is* registered, to a **trap** socket nothing in the call should ever reach. An
//!   AoR lookup on an in-dialog Request-URI therefore does not merely fail here: it delivers the
//!   acknowledgement to a socket that is not in the dialog, which is V-03's claim made visible.
//!   Same arrangement for alice's contact and the `BYE`.
//!
//! **Why it is a socket test.** `sipx-clstr-sim` models a node sans-IO and never runs `driver.rs`;
//! the defect is in what the driver does with the engine's effects, and `ET-7` owns the synthetic
//! probe's own AoR-shaped shortcuts. So the far end here is a protocol-correct test UA: `alice`
//! builds her `ACK` per RFC 3261 §13.2.2.4 — Request-URI is the dialog's **remote target** (the
//! `Contact` from the `2xx`), `Route` is the route set the `Record-Route` gave her — and `bob` builds
//! his `BYE` the same way from the other side.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::net::UdpSocket;

use sipx_clstr_node::driver::NodeConfig;

/// How long any step waits for the message it expects.
///
/// Everything here is loopback and answered on arrival, so the real numbers are milliseconds. This
/// is the point at which "still nothing" is a better report than hanging, and it is far below the
/// kernel's Timer B so a lost message never reads as a slow one.
const BUDGET: Duration = Duration::from_secs(4);

/// How long a "and nothing else happened" assertion waits before believing itself.
///
/// Nothing on the ACK path is timer-driven — an ACK is forwarded once, with no transaction and no
/// retransmission — so a second one would arrive immediately or never. This is slack, not a race.
const SETTLE: Duration = Duration::from_millis(400);

/// What the node says it is reached at. **Advertised, never bound** — see `tests/support/mod.rs`.
///
/// Deliberately on `127.0.0.1`, the same host as every device here, because that is what a loopback
/// deployment looks like (`scripts/two-node-call.sh` advertises `127.0.0.1:5060` and gives its phones
/// `127.0.0.1:15081`). An edge identity is host-scoped and port-agnostic by design
/// (proxy-behavior §5), so this arrangement is also what makes §5 P1 — "the Request-URI is a value
/// this platform placed in a `Record-Route`" — discriminating rather than "any URI at our host".
const ADVERTISED: &str = "127.0.0.1:15111";

/// The `Call-ID` of the one call each test places.
const CALL_ID: &str = "px13-the-call";

/// The node this scenario runs: one edge/registrar on a kernel-chosen port.
///
/// Built in code rather than from a cluster document, which is the one place this test differs from
/// `fork_branches.rs` and is deliberate. The trap registrations are addresses of record whose domain is a
/// `127.0.0.1:<ephemeral port>` authority, and the ports are not known until the sockets are bound —
/// so a document's `tenant.domains` could not name them. A code-built node serves any domain
/// (`NodeConfig::listening`'s default), and what this test is about is routing rather than
/// projection, which `role_dispatch.rs` and `fork_branches.rs` already prove from documents.
fn node_config() -> NodeConfig {
    NodeConfig::advertising(support::ephemeral(), ADVERTISED).expect("a loopback node")
}

// ------------------------------------------------------------------------------ reading messages ---

/// The first value of a header, trimmed, or `None`.
fn header(datagram: &str, name: &str) -> Option<String> {
    values(datagram, name).into_iter().next()
}

/// Every value of a header, in arrival order.
fn values(datagram: &str, name: &str) -> Vec<String> {
    let wanted = format!("{}:", name.to_ascii_lowercase());
    datagram
        .lines()
        .filter(|line| line.to_ascii_lowercase().starts_with(&wanted))
        .filter_map(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_owned())
        .collect()
}

/// The URI inside a `<…>` header value — a `Contact` or a `Route`'s remote target.
fn bare_uri(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_start_matches('<')
        .split('>')
        .next()
        .unwrap_or(value)
        .to_owned()
}

fn call_id(datagram: &str) -> Option<String> {
    header(datagram, "Call-ID")
}

/// Read from `socket` until a datagram satisfies `wanted`, or the budget runs out.
async fn recv_until(
    socket: &UdpSocket,
    budget: Duration,
    wanted: impl Fn(&str) -> bool,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut buffer = vec![0u8; 8192];
    while let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
            Ok(Ok((len, _))) => {
                let datagram =
                    String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
                if wanted(&datagram) {
                    return Some(datagram);
                }
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => return None,
        }
    }
    None
}

/// Wait until `ready` holds, or give up after [`BUDGET`]. Returns whether it held.
async fn until(ready: impl Fn() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + BUDGET;
    while tokio::time::Instant::now() < deadline {
        if ready() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ready()
}

// ------------------------------------------------------------------------------ writing messages ---

/// A REGISTER binding `contact` to `aor`.
///
/// `round` varies the branch, `Call-ID` and `CSeq` so each attempt is a new server transaction
/// rather than a retransmission the kernel answers out of its store.
fn register(from_port: u16, aor: &str, contact: &str, round: u32) -> String {
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px13-reg-{tag}-{round}\r\n\
         Max-Forwards: 70\r\n\
         From: <{aor}>;tag=px13reg{tag}{round}\r\n\
         To: <{aor}>\r\n\
         Call-ID: {call}\r\n\
         CSeq: {cseq} REGISTER\r\n\
         Contact: <{contact}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n",
        tag = register_tag(aor),
        call = register_call_id(aor, round),
        cseq = round + 1,
    )
}

/// A branch-safe spelling of an AoR, for the parts of a message that must differ per registration.
fn register_tag(aor: &str) -> String {
    aor.chars().filter(char::is_ascii_alphanumeric).collect()
}

/// The `Route` header lines for a route set, topmost first.
fn route_lines(route_set: &[String]) -> String {
    let mut lines = String::new();
    for value in route_set {
        lines.push_str("Route: ");
        lines.push_str(value);
        lines.push_str("\r\n");
    }
    lines
}

fn register_call_id(aor: &str, round: u32) -> String {
    format!("px13-register-{}-{round}", register_tag(aor))
}

/// Alice's INVITE for `sip:bob@example.test`, with her own contact.
fn invite(from_port: u16) -> String {
    format!(
        "INVITE sip:bob@example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px13-invite\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:alice@example.test>;tag=px13alice\r\n\
         To: <sip:bob@example.test>\r\n\
         Call-ID: {CALL_ID}\r\n\
         CSeq: 1 INVITE\r\n\
         Contact: <sip:alice@127.0.0.1:{from_port}>\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// A response to `request`, echoing what a response has to echo.
///
/// Built by copying the request's own header lines rather than by composing new ones: the `Via`
/// stack is the return path and the `Record-Route` is the far end's route set, so a device that
/// invented either would be testing this test rather than the node.
fn respond(
    request: &str,
    status: u16,
    reason: &str,
    to_tag: Option<&str>,
    contact: Option<&str>,
) -> String {
    let mut lines = vec![format!("SIP/2.0 {status} {reason}")];
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
            match to_tag {
                // A 2xx without a tag is not a dialog; a response that echoes a `To` which already
                // has one must not add a second.
                Some(tag) if !name.contains(";tag=") => lines.push(format!("{line};tag={tag}")),
                _ => lines.push(line.to_owned()),
            }
        }
    }
    if let Some(contact) = contact {
        lines.push(format!("Contact: <{contact}>"));
    }
    lines.push("Content-Length: 0".to_owned());
    lines.push(String::new());
    lines.push(String::new());
    lines.join("\r\n")
}

/// Alice's `ACK` for the `2xx`, built the way RFC 3261 §13.2.2.4 says to build one.
///
/// The Request-URI is the dialog's **remote target** — the `Contact` the `2xx` carried — and the
/// `Route` is the route set the `Record-Route` established. Neither is an address of record, which
/// is the whole point: this is what every phone sends, and at the merge base it is dropped.
fn ack_for_2xx(from_port: u16, remote_target: &str, route_set: &[String], to: &str) -> String {
    let routes = route_lines(route_set);
    format!(
        "ACK {remote_target} SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px13-ack\r\n\
         Max-Forwards: 70\r\n\
         {routes}\
         From: <sip:alice@example.test>;tag=px13alice\r\n\
         To: {to}\r\n\
         Call-ID: {CALL_ID}\r\n\
         CSeq: 1 ACK\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// Alice's `ACK` for a **non-2xx**, which is a different message with a different owner.
///
/// RFC 3261 §17.1.1.3: it is part of the `INVITE` transaction, so it keeps the `INVITE`'s
/// Request-URI and route set rather than the dialog's remote target — there is no dialog. It is
/// absorbed by the server transaction that sent the final response and is never forwarded.
fn ack_for_non_2xx(from_port: u16, to: &str) -> String {
    format!(
        "ACK sip:bob@example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px13-invite\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:alice@example.test>;tag=px13alice\r\n\
         To: {to}\r\n\
         Call-ID: {CALL_ID}\r\n\
         CSeq: 1 INVITE\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// Bob's `BYE`, from the other side of the same dialog.
///
/// `From`/`To` are swapped, the Request-URI is alice's `Contact` and the `Route` is the route set
/// bob learned from the `Record-Route` on the `INVITE` he answered.
fn bye(from_port: u16, remote_target: &str, route_set: &[String]) -> String {
    let routes = route_lines(route_set);
    format!(
        "BYE {remote_target} SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-px13-bye\r\n\
         Max-Forwards: 70\r\n\
         {routes}\
         From: <sip:bob@example.test>;tag=px13bob\r\n\
         To: <sip:alice@example.test>;tag=px13alice\r\n\
         Call-ID: {CALL_ID}\r\n\
         CSeq: 1 BYE\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

// --------------------------------------------------------------------------------- the test UAs ---

/// What bob saw, for the test to assert on.
#[derive(Debug, Default)]
struct Seen {
    /// Every `ACK` that reached him. Exactly one is correct; zero is the merge base.
    acks: usize,
    /// Every final response to his `BYE`, as it arrived — the datagram, so the `Via` path is
    /// assertable and not merely the status.
    bye_responses: Vec<String>,
}

/// The callee: answers one `INVITE` with `status`, then optionally hangs up with a `BYE`.
///
/// It is the far end of a dialog rather than a socket that echoes: it keeps the route set the
/// `Record-Route` gave it and the caller's `Contact`, and builds its `BYE` from those. A device that
/// sent the AoR instead is the shortcut this story exists to remove.
fn callee(
    socket: UdpSocket,
    node: String,
    status: u16,
    reason: &'static str,
    hang_up: bool,
    seen: Arc<Mutex<Seen>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let contact = format!(
            "sip:bob@127.0.0.1:{}",
            socket.local_addr().expect("bob's address").port()
        );
        let mut route_set: Vec<String> = Vec::new();
        let mut remote_target = String::new();
        let mut buffer = vec![0u8; 8192];
        loop {
            let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
                return;
            };
            let datagram =
                String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
            if datagram.starts_with("INVITE ") {
                // §12.1.1: the callee's route set is the `Record-Route` list as it arrived, and the
                // remote target is the caller's `Contact`.
                route_set = values(&datagram, "Record-Route");
                remote_target = header(&datagram, "Contact")
                    .map(|value| bare_uri(&value))
                    .unwrap_or_default();
                let answer = respond(&datagram, status, reason, Some("px13bob"), Some(&contact));
                let _ = socket.send_to(answer.as_bytes(), from).await;
            } else if datagram.starts_with("ACK ") {
                let first = {
                    let mut record = seen.lock().unwrap();
                    record.acks += 1;
                    record.acks == 1
                };
                if hang_up && first {
                    let port = socket.local_addr().expect("bob's address").port();
                    let _ = socket
                        .send_to(bye(port, &remote_target, &route_set).as_bytes(), &node)
                        .await;
                }
            } else if datagram.starts_with("SIP/2.0")
                && header(&datagram, "CSeq").is_some_and(|cseq| cseq.ends_with("BYE"))
            {
                seen.lock().unwrap().bye_responses.push(datagram);
            }
        }
    })
}

/// A socket that only counts what reaches it: the trap an AoR lookup would deliver to.
fn trap(socket: UdpSocket, hits: Arc<AtomicUsize>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buffer = vec![0u8; 8192];
        while let Ok((len, _)) = socket.recv_from(&mut buffer).await {
            if len > 0 {
                hits.fetch_add(1, Ordering::Relaxed);
            }
        }
    })
}

/// Register `contact` for `aor`, retrying until the node answers.
async fn register_contact(
    caller: &UdpSocket,
    node: &str,
    caller_port: u16,
    aor: &str,
    contact: &str,
) {
    for round in 0..60u32 {
        caller
            .send_to(register(caller_port, aor, contact, round).as_bytes(), node)
            .await
            .expect("send REGISTER");
        let wanted = register_call_id(aor, round);
        let answered = recv_until(caller, Duration::from_millis(150), |datagram| {
            datagram.starts_with("SIP/2.0 200") && call_id(datagram).as_deref() == Some(&wanted)
        })
        .await;
        if answered.is_some() {
            return;
        }
    }
    panic!("the node never answered a REGISTER for {aor}");
}

async fn bound() -> (UdpSocket, u16) {
    let socket = UdpSocket::bind("127.0.0.1:0").await.expect("bind");
    let port = socket.local_addr().expect("an address").port();
    (socket, port)
}

// ---------------------------------------------------------------------------------- the scenario ---

/// The call's real registration, and the two traps.
///
/// A trap is registered under the **canonical AoR of a contact** — `sip:bob@127.0.0.1:<port>`, port
/// and all, because an explicit port is part of the key (location-service §3 N7). That is exactly
/// what a location lookup on an in-dialog Request-URI resolves to, so reaching a trap is a positive
/// observation that a lookup decided the next hop rather than an inference from silence.
async fn arm_the_store(alice: &UdpSocket, node: &str, ports: (u16, u16, u16, u16)) {
    let (alice_port, bob_port, bob_trap_port, alice_trap_port) = ports;
    register_contact(
        alice,
        node,
        alice_port,
        "sip:bob@example.test",
        &format!("sip:bob@127.0.0.1:{bob_port}"),
    )
    .await;
    register_contact(
        alice,
        node,
        alice_port,
        &format!("sip:bob@127.0.0.1:{bob_port}"),
        &format!("sip:trap@127.0.0.1:{bob_trap_port}"),
    )
    .await;
    register_contact(
        alice,
        node,
        alice_port,
        &format!("sip:alice@127.0.0.1:{alice_port}"),
        &format!("sip:trap@127.0.0.1:{alice_trap_port}"),
    )
    .await;
}

/// The other half of the dialog: the callee's `BYE` must reach alice by the route set, and its `200`
/// must come back down the `Via` path the `BYE` built.
async fn the_callee_hangs_up(
    alice: &UdpSocket,
    node: &str,
    alice_port: u16,
    seen: &Arc<Mutex<Seen>>,
) {
    let hangup = recv_until(alice, BUDGET, |datagram| {
        datagram.starts_with("BYE ") && call_id(datagram).as_deref() == Some(CALL_ID)
    })
    .await
    .expect("the callee's BYE, forwarded by the node");
    assert!(
        hangup.starts_with(&format!("BYE sip:alice@127.0.0.1:{alice_port} SIP/2.0")),
        "the BYE must keep the dialog's remote target as its Request-URI:\n{hangup}"
    );
    alice
        .send_to(respond(&hangup, 200, "OK", None, None).as_bytes(), node)
        .await
        .expect("answer the BYE");

    let answered = until(|| !seen.lock().unwrap().bye_responses.is_empty()).await;
    assert!(
        answered,
        "the BYE's 200 never came back to the callee; the response follows the Via path the request \
         built"
    );
    let response = seen
        .lock()
        .unwrap()
        .bye_responses
        .first()
        .cloned()
        .unwrap_or_default();
    assert!(
        response.starts_with("SIP/2.0 200"),
        "the BYE must be answered 200 by the far end, not by the proxy:\n{response}"
    );
    assert!(
        header(&response, "Via").is_some_and(|via| via.contains("z9hG4bK-px13-bye")),
        "the node pops its own Via and returns the response on the existing path, so the topmost \
         Via left is the callee's own:\n{response}"
    );
}

/// **The failing-first test for `PX-13`.**
///
/// At the merge base the `ACK`'s Request-URI is resolved as an address of record, and the trap is
/// registered under exactly that key — so the acknowledgement lands on a socket that is not in the
/// call, bob never sees it, and no `BYE` is ever sent. Fixed, the `ACK` follows the `Route` set to the
/// dialog's remote target exactly once, both traps stay silent, and bob's `BYE` reaches alice and is
/// answered back down the `Via` path it arrived on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn px13_a_2xx_ack_and_a_route_set_bye_reach_a_contact_that_is_not_an_aor() {
    let (bob, bob_port) = bound().await;
    let (alice, alice_port) = bound().await;
    let (bob_trap, bob_trap_port) = bound().await;
    let (alice_trap, alice_trap_port) = bound().await;

    let seen = Arc::new(Mutex::new(Seen::default()));
    let bob_trap_hits = Arc::new(AtomicUsize::new(0));
    let alice_trap_hits = Arc::new(AtomicUsize::new(0));
    let traps = (
        trap(bob_trap, Arc::clone(&bob_trap_hits)),
        trap(alice_trap, Arc::clone(&alice_trap_hits)),
    );

    let running = support::start_in_process(node_config()).await;
    let node = running.target();
    let bob_task = callee(bob, node.clone(), 200, "OK", true, Arc::clone(&seen));
    arm_the_store(
        &alice,
        &node,
        (alice_port, bob_port, bob_trap_port, alice_trap_port),
    )
    .await;

    alice
        .send_to(invite(alice_port).as_bytes(), &node)
        .await
        .expect("send INVITE");
    let answer = recv_until(&alice, BUDGET, |datagram| {
        datagram.starts_with("SIP/2.0 200") && call_id(datagram).as_deref() == Some(CALL_ID)
    })
    .await
    .expect("the callee's 200 OK, relayed by the node");

    // §12.1.2: the caller's route set is the `Record-Route` list reversed, and the remote target is
    // the `Contact` from the `2xx`.
    let mut route_set = values(&answer, "Record-Route");
    route_set.reverse();
    assert!(
        !route_set.is_empty(),
        "the node must Record-Route a dialog-forming INVITE, or there is no route set to test:\n\
         {answer}"
    );
    let remote_target = header(&answer, "Contact")
        .map(|value| bare_uri(&value))
        .expect("the callee's Contact");
    assert_eq!(
        remote_target,
        format!("sip:bob@127.0.0.1:{bob_port}"),
        "the remote target is the callee's contact, which is not a registered AoR"
    );
    let to = header(&answer, "To").expect("a To with the callee's tag");

    alice
        .send_to(
            ack_for_2xx(alice_port, &remote_target, &route_set, &to).as_bytes(),
            &node,
        )
        .await
        .expect("send ACK");

    let arrived = until(|| seen.lock().unwrap().acks >= 1).await;
    assert!(
        arrived,
        "the 2xx ACK never reached the callee. Its Request-URI is the dialog's remote target \
         ({remote_target}), which is not an address of record — an ACK routed by a location lookup \
         is dropped, or delivered to whatever the AoR happens to hold. Trap hits: bob {}, alice {}",
        bob_trap_hits.load(Ordering::Relaxed),
        alice_trap_hits.load(Ordering::Relaxed),
    );
    tokio::time::sleep(SETTLE).await;
    assert_eq!(
        seen.lock().unwrap().acks,
        1,
        "an ACK is forwarded once, with no transaction of its own and nothing to retransmit it"
    );

    // The BYE bob sent on receiving the ACK must reach alice by the same route set, and its
    // Request-URI must be her contact rather than her address of record.
    the_callee_hangs_up(&alice, &node, alice_port, &seen).await;

    // Neither in-dialog request consulted the location service: if either had, it would have found
    // the trap registration for its Request-URI's canonical AoR and gone there.
    assert_eq!(
        bob_trap_hits.load(Ordering::Relaxed),
        0,
        "the ACK's Request-URI was resolved as the address of record sip:bob@127.0.0.1:{bob_port}"
    );
    assert_eq!(
        alice_trap_hits.load(Ordering::Relaxed),
        0,
        "the BYE's Request-URI was resolved as the address of record \
         sip:alice@127.0.0.1:{alice_port}"
    );

    running.stop();
    bob_task.abort();
    traps.0.abort();
    traps.1.abort();
}

/// The other two ACKs, which are **not** separately routed requests and must not be treated as one.
///
/// RFC 3261 §17.1.1.3 splits the method three ways, and only one of them is the proxy's to forward:
///
/// - the `ACK` for a non-2xx that goes **downstream** is generated by the client transaction that
///   received the final response — the kernel's, not ours, so exactly one reaches the callee;
/// - the `ACK` for a non-2xx that arrives from **upstream** is part of the server transaction that
///   sent the final response. It is absorbed there: it stops the response being retransmitted
///   (§17.2.1, Completed → Confirmed) and it is never forwarded, so the callee's count stays at one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn px13_a_non_2xx_ack_is_absorbed_by_its_server_transaction() {
    let (bob, bob_port) = bound().await;
    let (alice, alice_port) = bound().await;

    let seen = Arc::new(Mutex::new(Seen::default()));
    let running = support::start_in_process(node_config()).await;
    let node = running.target();
    let bob_task = callee(
        bob,
        node.clone(),
        486,
        "Busy Here",
        false,
        Arc::clone(&seen),
    );

    register_contact(
        &alice,
        &node,
        alice_port,
        "sip:bob@example.test",
        &format!("sip:bob@127.0.0.1:{bob_port}"),
    )
    .await;

    alice
        .send_to(invite(alice_port).as_bytes(), &node)
        .await
        .expect("send INVITE");
    let busy = recv_until(&alice, BUDGET, |datagram| {
        datagram.starts_with("SIP/2.0 486") && call_id(datagram).as_deref() == Some(CALL_ID)
    })
    .await
    .expect("the callee's 486, relayed by the node");

    // The downstream ACK is the client transaction's, and it is already on its way — the kernel sent
    // it when the 486 arrived, before the response was relayed here.
    let acknowledged = until(|| seen.lock().unwrap().acks >= 1).await;
    assert!(
        acknowledged,
        "the kernel's client transaction owes the callee an ACK for its 486 (RFC 3261 §17.1.1.3)"
    );

    // Answered immediately, so the absorption is proved against the server transaction's own
    // retransmission timer rather than against a wall clock: Timer G would fire at T1 = 500 ms.
    let to = header(&busy, "To").expect("a To with the callee's tag");
    alice
        .send_to(ack_for_non_2xx(alice_port, &to).as_bytes(), &node)
        .await
        .expect("send the ACK for the 486");

    let retransmitted = recv_until(&alice, SETTLE * 4, |datagram| {
        datagram.starts_with("SIP/2.0 486") && call_id(datagram).as_deref() == Some(CALL_ID)
    })
    .await;
    assert!(
        retransmitted.is_none(),
        "the ACK for a non-2xx is absorbed by the server transaction that sent it, which stops \
         Timer G; a retransmitted 486 means it was not:\n{}",
        retransmitted.unwrap_or_default()
    );
    assert_eq!(
        seen.lock().unwrap().acks,
        1,
        "an upstream non-2xx ACK is absorbed, never forwarded — a second ACK at the callee means \
         the proxy re-routed a message that belonged to a transaction"
    );

    running.stop();
    bob_task.abort();
}

/// A next hop with no address settles as a **state-machine input**, and never as a `continue`.
///
/// `destination_of` resolves only address literals — a name needs RFC 3263, which is `RT-1`'s — so a
/// registered contact naming a host is a branch this driver cannot send. That is §16.9's case, a
/// branch that failed, and it has to reach the engine as one: R10 concludes the branch `503` and R8
/// turns it into the `500` the caller sees, because a hop we could not reach is not the destination
/// being overloaded.
///
/// The merge base skipped it with a `warn!` and a `continue`. The engine had already recorded the
/// branch as pending, so the context waited on a request that was never sent: nothing upstream was
/// ever answered, the caller heard silence, and the transaction sat on its admission slot (`DP-11`)
/// with no timer to reap it. A logged drop is still a drop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn px13_a_branch_with_no_addressable_next_hop_is_answered_not_abandoned() {
    let (alice, alice_port) = bound().await;
    let running = support::start_in_process(node_config()).await;
    let node = running.target();

    // A contact naming a host rather than an address. The registrar stores what the UA registered
    // (F2 keeps it verbatim), so the engine forwards to it and the driver has nothing to resolve.
    register_contact(
        &alice,
        &node,
        alice_port,
        "sip:bob@example.test",
        "sip:bob@nowhere.invalid",
    )
    .await;

    alice
        .send_to(invite(alice_port).as_bytes(), &node)
        .await
        .expect("send INVITE");
    let final_response = recv_until(&alice, BUDGET, |datagram| {
        datagram.starts_with("SIP/2.0 5") && call_id(datagram).as_deref() == Some(CALL_ID)
    })
    .await;

    assert!(
        final_response.is_some(),
        "the caller was never answered: a branch the driver cannot address must settle as \
         `BranchTransportError` so §16.7 can conclude the context, not be skipped while the context \
         waits for a response nothing will send"
    );
    let final_response = final_response.unwrap_or_default();
    assert!(
        final_response.starts_with("SIP/2.0 500"),
        "§16.9 into R8: a transport failure is a branch `503`, and a `503` that becomes the best \
         response is sent on as `500` — the caller must not be told the destination is overloaded \
         when what happened is that we could not get there:\n{final_response}"
    );

    running.stop();
}
