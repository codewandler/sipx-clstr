//! `FC-6` — a declared `cluster.security` control this build cannot apply stops the node.
//!
//! Four keys of §7's `security` row — `unknownSource`, `sanityCheck`, `userAgentDenyList` and
//! `internalZone` — were on the loader's allow-list, validated against nothing, and reached no field
//! of `NodeConfig`. So a document that said "drop what you do not recognise, refuse this User-Agent,
//! and treat only these networks as internal" loaded, produced no unapplied warning, and served the
//! opposite posture: everything admitted, from anywhere. Found by the independent review of
//! `86e6b10` (`v0.12.0`) as finding **V-06** and reproduced against the real binary.
//!
//! This drives the **binary**, for the reason `tests/startup_warns.rs` does: what is under test is
//! whether a refusal reaches an operator before anything is served, and that is a property of the
//! process rather than of a function. The green result is that the node never binds. The red result
//! is the defect itself, so the test *goes on* to send the denied User-Agent from loopback and reports
//! the status it was answered — a failure message that quotes V-06 rather than merely contradicting
//! an assertion.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// What the node says it is reached at. Advertised, never bound — see `tests/support/mod.rs`.
const ADVERTISED: &str = "127.0.0.1:15111";

/// The User-Agent the document denies. A phone that should never be admitted, by name.
const DENIED_USER_AGENT: &str = "evil-phone";

/// The three controls this document declares, by the path a refusal must name.
const DECLARED: [&str; 3] = [
    "cluster.security.internalZone",
    "cluster.security.unknownSource",
    "cluster.security.userAgentDenyList",
];

/// A document that is otherwise startable and asks for three ingress controls.
///
/// `unknownSource: drop` and a deny list naming [`DENIED_USER_AGENT`], with an `internalZone` that
/// covers private space and therefore **excludes loopback** — so a REGISTER from `127.0.0.1` carrying
/// that User-Agent is a request all three controls would have something to say about, and every one of
/// them says "not this one".
///
/// It binds `127.0.0.1:0` (`CF-13`): the kernel picks the port, so no run of this suite can collide
/// with another.
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
    - roles: [edge, registrar]
      transport: udp
      bind: {bind}
      advertise: {ADVERTISED}
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      # cluster-membership MB5: required of a member on the call path, dialled by nobody in this
      # build, and therefore reported as unapplied configuration.
      rpc: 127.0.0.1:7223
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [example.test]
  security:
    unknownSource: drop
    userAgentDenyList: [{DENIED_USER_AGENT}]
    internalZone:
      networks: [\"10.0.0.0/8\", \"172.16.0.0/12\", \"192.168.0.0/16\"]
",
        bind = support::EPHEMERAL,
    )
}

/// Send a REGISTER from loopback carrying the denied User-Agent, and return the status line.
///
/// Only reached when the node started, which is the defect. It exists so the failure says what the
/// node *did* — `200 OK` to a phone the document names as denied, from a source the document does not
/// count as internal — rather than only that a refusal was expected.
fn register_as_the_denied_phone(node: SocketAddr) -> String {
    let caller = UdpSocket::bind("127.0.0.1:0").expect("bind the caller");
    caller
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("a read timeout");
    let port = caller.local_addr().expect("the caller's address").port();
    let register = format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{port};branch=z9hG4bK-fc6-v06\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:stranger@example.test>;tag=fc6v06\r\n\
         To: <sip:stranger@example.test>\r\n\
         Call-ID: fc6-v06-register\r\n\
         CSeq: 1 REGISTER\r\n\
         User-Agent: {DENIED_USER_AGENT}\r\n\
         Contact: <sip:stranger@127.0.0.1:{port}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n"
    );
    caller
        .send_to(register.as_bytes(), node)
        .expect("send REGISTER");

    let mut buffer = vec![0u8; 4096];
    match caller.recv_from(&mut buffer) {
        Ok((len, _from)) => String::from_utf8_lossy(buffer.get(..len).unwrap_or_default())
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned(),
        Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            "no answer".to_owned()
        }
        Err(error) => panic!("recv failed: {error}"),
    }
}

/// **The failing-first test for `FC-6`.** The refusal arrives before any socket is bound.
///
/// On `e6fffd6` the node starts, warns about nothing, and answers the denied phone `200 OK`.
#[test]
fn fc6_a_declared_security_control_stops_the_node_before_it_binds() {
    let path = support::write_document(&document(), "security-refused");

    let refusal = match support::BinaryNode::try_start(&path) {
        Err(refusal) => refusal,
        Ok(node) => {
            // The defect, reproduced rather than asserted away.
            let status = register_as_the_denied_phone(node.addr());
            let stderr = node.stop();
            panic!(
                "the node started with three unappliable `cluster.security` controls declared, and \
                 the REGISTER from loopback carrying `User-Agent: {DENIED_USER_AGENT}` was answered \
                 `{status}`. A declared control that changes no decision must stop the node. stderr \
                 was:\n{stderr}"
            );
        }
    };

    assert!(
        refusal.exited,
        "the node neither bound nor stopped; a refusal has to be reached, not waited out. stderr \
         was:\n{}",
        refusal.stderr
    );
    assert!(
        refusal.stderr.contains("was refused"),
        "the document must be refused by the loader, so every problem is reported at once. stderr \
         was:\n{}",
        refusal.stderr
    );
    // Every declared path, not the first: an operator who removes one key and restarts into the next
    // refusal has been told about one of three problems (§8 V1).
    for path in DECLARED {
        assert!(
            refusal.stderr.contains(path),
            "the refusal must name `{path}`. stderr was:\n{}",
            refusal.stderr
        );
    }
    // Before any socket: the bind announcement is the node's own readiness signal, and its absence is
    // how "nothing was served" is observed from outside the process.
    assert!(
        !refusal.stdout.contains("listening on"),
        "the node bound a socket before refusing. stdout was:\n{}",
        refusal.stdout
    );
    // `FC-8`'s rule, honoured here rather than left for it: a refusal describes what was declared and
    // never echoes it, so a document whose value happened to be a secret does not publish it.
    assert!(
        !refusal.stderr.contains(DENIED_USER_AGENT),
        "the refusal echoed a configured value back into the log. stderr was:\n{}",
        refusal.stderr
    );
}
