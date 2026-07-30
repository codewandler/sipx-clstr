//! `FC-6` — a node performs only the methods its declared roles wire (`cluster-config` §4 R3).
//!
//! The roles were read, validated, used to pick listeners and the location store — and then dropped
//! before `NodeConfig` was built, so the one dispatcher sent every REGISTER to the registrar and
//! everything else to the proxy whatever the node had been started as. A node started as
//! `inbound-proxy` therefore answered `200 OK` to a REGISTER and stored the binding, which is a
//! registrar nobody deployed, holding state no operator knows about, on a node whose configuration
//! says it is not one. Found by the independent review of `86e6b10` (`v0.12.0`) as finding **V-01**
//! and reproduced against the real binary.
//!
//! These scenarios run the **real** node over a **real** socket, because what is under test is the
//! driver's dispatch and the driver is the one layer allowed a socket (AGENTS.md #2). Three UDP
//! endpoints, as in `tests/admission_bound.rs`:
//!
//! - the **node**, from a cluster document, projected through the identity under test;
//! - the **sink**, the contact a REGISTER would bind. It answers nothing — it exists to say whether
//!   anything was ever routed to it, which is how "no binding was created" is observed from outside
//!   a process whose store is in-memory;
//! - the **caller**, which sends the REGISTER and then the INVITE that would use the binding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::time::Duration;

use tokio::net::UdpSocket;

use sipx_clstr_node::config::{NodeIdentity, Role};
use sipx_clstr_node::driver::NodeConfig;
use sipx_clstr_node::startup;

/// What the node says it is reached at. Advertised, never bound — see `tests/support/mod.rs`.
const ADVERTISED: &str = "127.0.0.1:15101";

/// A cluster document that declares one listener per role set under test.
///
/// A listener is projected onto a node only when its roles intersect the node's (§5 P2), so each
/// identity below sees exactly one of these and §5 P6 — two projected listeners of one transport —
/// is never reached.
fn document() -> String {
    format!(
        "
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: fc6
  environment: dev
  zones: [a]
  listener:
    - roles: [inbound-proxy]
      transport: udp
      bind: {bind}
      advertise: {ADVERTISED}
    - roles: [registrar]
      transport: udp
      bind: {bind}
      advertise: {ADVERTISED}
    - roles: [echo]
      transport: udp
      bind: {bind}
      advertise: {ADVERTISED}
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

/// The node this identity describes, built the way the binary builds it.
///
/// No fallback: the whole claim is that the *document* decides what the node serves, so a document
/// that cannot express it is a failure of this story rather than a reason to run something else.
fn node_config(tag: &str, roles: &[Role]) -> NodeConfig {
    let path = support::write_document(&document(), tag);
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: roles.iter().copied().collect(),
    };
    startup::from_document(&path, &identity, &BTreeMap::new())
        .unwrap_or_else(|error| panic!("the document must describe a startable node: {error}"))
}

// ------------------------------------------------------------------------------- the messages ---

fn register(from_port: u16, sink_port: u16) -> String {
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-fc6-reg\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:sink@example.test>;tag=fc6reg\r\n\
         To: <sip:sink@example.test>\r\n\
         Call-ID: fc6-register\r\n\
         CSeq: 1 REGISTER\r\n\
         Contact: <sip:sink@127.0.0.1:{sink_port}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

fn invite(from_port: u16) -> String {
    format!(
        "INVITE sip:sink@example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-fc6-inv\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:caller@example.test>;tag=fc6c\r\n\
         To: <sip:sink@example.test>\r\n\
         Call-ID: fc6-invite\r\n\
         CSeq: 1 INVITE\r\n\
         Content-Length: 0\r\n\
         \r\n"
    )
}

/// The first response to arrive on `socket`, or `None` if none does within `budget`.
async fn response(socket: &UdpSocket, budget: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut buffer = vec![0u8; 4096];
    while let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buffer)).await {
            Ok(Ok((len, _from))) => {
                let datagram =
                    String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
                if datagram.starts_with("SIP/2.0 ") {
                    return Some(datagram);
                }
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => return None,
        }
    }
    None
}

/// The status line's code, for a datagram known to be a response.
fn status_of(datagram: &str) -> String {
    datagram
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Whether anything at all arrives on `socket` within `budget`.
async fn anything(socket: &UdpSocket, budget: Duration) -> Option<String> {
    let mut buffer = vec![0u8; 4096];
    match tokio::time::timeout(budget, socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, _from))) => {
            Some(String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned())
        }
        Ok(Err(error)) => panic!("recv failed: {error}"),
        Err(_) => None,
    }
}

/// **The failing-first test for `FC-6`.** A node that is not a registrar does not register anyone.
///
/// Two assertions, and the second is the one the story names. The answer says the node refused
/// rather than dropped the request — §4 R3's wiring, reported as RFC 3261 §21.4.6's `405` — and the
/// INVITE that follows proves the refusal was not merely cosmetic: an `inbound-proxy` still proxies,
/// so the INVITE is dispatched, looked up, and reaches nobody, because there is no binding to reach.
///
/// On `86e6b10` the REGISTER is answered `200 OK` and the INVITE arrives at the sink.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fc6_an_inbound_proxy_does_not_register_anyone() {
    let sink = UdpSocket::bind("127.0.0.1:0").await.expect("bind the sink");
    let sink_port = sink.local_addr().expect("the sink's address").port();
    let caller = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the caller");
    let caller_port = caller.local_addr().expect("the caller's address").port();

    let running =
        support::start_in_process(node_config("inbound-proxy", &[Role::InboundProxy])).await;
    let node = running.target();

    caller
        .send_to(register(caller_port, sink_port).as_bytes(), &node)
        .await
        .expect("send REGISTER");
    let answer = response(&caller, Duration::from_secs(2))
        .await
        .expect("a node that will not register must say so, not go silent");
    let status = status_of(&answer);

    // Then the request that would use a binding, had one been made.
    caller
        .send_to(invite(caller_port).as_bytes(), &node)
        .await
        .expect("send INVITE");
    let at_sink = anything(&sink, Duration::from_secs(2)).await;

    running.stop();

    println!("REGISTER was answered `{status}`; the sink received {at_sink:?}");

    assert!(
        !status.starts_with("SIP/2.0 2"),
        "a node whose roles are `inbound-proxy` answered `{status}` to a REGISTER"
    );
    assert!(
        status.starts_with("SIP/2.0 405"),
        "the refusal must be RFC 3261 §21.4.6's `405 Method Not Allowed`, not `{status}`"
    );
    assert!(
        answer.contains("Allow:"),
        "a `405` carries the methods that are allowed (RFC 3261 §21.4.6). The answer was:\n{answer}"
    );
    // The binding, observed from outside: nothing can be routed to a contact that was never stored.
    assert_eq!(
        at_sink, None,
        "a REGISTER the node refused still created a binding: the INVITE reached the contact"
    );
}

/// The other half of R3: a `registrar` is not a proxy.
///
/// Wiring is the union of the sections a role's column marks (§4 R7) and there is no other way for a
/// role to acquire behaviour — so a node given only `registrar` registers, and refuses the calls it
/// was never configured to carry. On `86e6b10` the INVITE is proxied and answered `404`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fc6_a_registrar_does_not_proxy() {
    let sink = UdpSocket::bind("127.0.0.1:0").await.expect("bind the sink");
    let sink_port = sink.local_addr().expect("the sink's address").port();
    let caller = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the caller");
    let caller_port = caller.local_addr().expect("the caller's address").port();

    let running = support::start_in_process(node_config("registrar", &[Role::Registrar])).await;
    let node = running.target();

    caller
        .send_to(register(caller_port, sink_port).as_bytes(), &node)
        .await
        .expect("send REGISTER");
    let registered = response(&caller, Duration::from_secs(2))
        .await
        .map(|datagram| status_of(&datagram));

    caller
        .send_to(invite(caller_port).as_bytes(), &node)
        .await
        .expect("send INVITE");
    let to_the_invite = response(&caller, Duration::from_secs(2))
        .await
        .map(|datagram| status_of(&datagram));
    let at_sink = anything(&sink, Duration::from_millis(500)).await;

    running.stop();

    println!("REGISTER `{registered:?}`, INVITE `{to_the_invite:?}`, sink {at_sink:?}");

    assert_eq!(
        registered.as_deref(),
        Some("SIP/2.0 200 OK"),
        "a `registrar` must still register"
    );
    assert!(
        to_the_invite
            .as_deref()
            .is_some_and(|status| status.starts_with("SIP/2.0 405")),
        "a node whose roles are `registrar` answered `{to_the_invite:?}` to an INVITE; the call \
         path is not wired on it"
    );
    assert_eq!(
        at_sink, None,
        "a node with no proxy role forwarded a call anyway"
    );
}

/// A role this build has no path for stops the node, rather than being served as something else.
///
/// The `FC-1`/`FC-3` shape: accepted means applied, or refused. `echo` is a UAS role
/// ([e2e-probe](../../../docs/specs/e2e-probe.md) §9) whose endpoint this driver does not run and
/// whose `cluster.echo` section this loader does not read, so a node started with it cannot do what
/// it was asked. Before this story it came up as a full proxy **and** registrar — the one thing §9's
/// "no proxy role ever links a UAS" forbids absolutely.
#[test]
fn fc6_a_role_this_build_cannot_serve_stops_the_node() {
    let path = support::write_document(&document(), "echo-refused");
    let identity = NodeIdentity {
        node: 1,
        zone: "a".to_owned(),
        roles: [Role::Echo].into_iter().collect(),
    };
    let Err(refusal) = startup::from_document(&path, &identity, &BTreeMap::new()) else {
        panic!("a node asked for `echo` must refuse to start; this build has no echo endpoint")
    };
    let error = refusal.to_string();
    assert!(
        error.contains("echo"),
        "the refusal must name the role that cannot be served: {error}"
    );
}
