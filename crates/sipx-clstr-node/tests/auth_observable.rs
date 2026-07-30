//! `RG-15` — an authentication outcome has to reach an operator, and nothing else may ride along.
//!
//! [registrar-auth](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md)
//! §9, vectors `RA-L-1`, `RA-L-2` and `RA-L-3`. The decision has always computed *why* it refused —
//! `ChallengeResponse::because` is documented "Why, for logs and tests" — and the driver dropped it
//! on the floor, so a `401` was indistinguishable from silence to anyone outside the process. With
//! no rate limiting and a 300-second nonce lifetime, that makes brute force against a tenant both
//! undetectable and unbounded.
//!
//! This runs the **real** driver over a **real** socket, because the audit record is the driver's
//! (AGENTS.md #2: the registrar decides, the driver is the layer allowed an effect). A unit test on
//! the decision would prove the reason is *computed*, which was never the defect.
//!
//! **One `#[test]` for three vectors, with a `covers:` comment**, because the capture is a process-
//! wide `tracing` subscriber: two of these running in parallel would each read the other's records.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::UdpSocket;

use sipx_clstr_node::driver::{self, AuthConfig, NodeConfig};
use sipx_clstr_registrar::InMemoryCredentials;

/// The node that challenges. Its own port, so a parallel suite run does not fight over one.
const AUTHENTICATED_PORT: u16 = 15091;
/// The node that does not (§3 A1) — `RA-L-3`'s *unauthenticated* is a recorded fact, not a silence.
const OPEN_PORT: u16 = 15092;

/// Distinctive on purpose: every one of these is credential material, and the assertion at the end
/// is that none of it appears in a record. A generic value like `"alice"` would match by accident.
const USERNAME: &str = "enumerate-me-9f3a";
const PASSWORD: &str = "p4ssw0rd-must-not-be-logged-7b21";
const CNONCE: &str = "cnonce-must-not-be-logged-4d5e";
const WRONG_RESPONSE: &str = "0badd16e57must0not0be0logged0aa1";

const REALM: &str = "acme.example";
const AUTHENTICATED_TENANT: &str = "challenging-tenant";
const OPEN_TENANT: &str = "open-tenant";

// ------------------------------------------------------------------------------ the capture ---

/// Everything the node logged, as an operator's terminal would have shown it.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn text(&self) -> String {
        let buffer = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        String::from_utf8_lossy(&buffer).into_owned()
    }
}

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut buffer = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

// ----------------------------------------------------------------------------- the two nodes ---

fn challenging_node() -> NodeConfig {
    let mut config = NodeConfig::new(
        format!("127.0.0.1:{AUTHENTICATED_PORT}")
            .parse()
            .expect("an address"),
    )
    .expect("a loopback node");
    AUTHENTICATED_TENANT.clone_into(&mut config.tenant);
    config.auth = Some(AuthConfig {
        realm: REALM.to_owned(),
        // Fixed rather than drawn: a scenario that cannot be replayed byte for byte is not a
        // scenario, it is a sighting.
        secret: [0x5a_u8; 32],
        credentials: InMemoryCredentials::new().with(AUTHENTICATED_TENANT, USERNAME, PASSWORD),
    });
    config
}

fn open_node() -> NodeConfig {
    let mut config = NodeConfig::new(
        format!("127.0.0.1:{OPEN_PORT}")
            .parse()
            .expect("an address"),
    )
    .expect("a loopback node");
    OPEN_TENANT.clone_into(&mut config.tenant);
    config.auth = None;
    config
}

// ------------------------------------------------------------------------------ the messages ---
//
// Written as text rather than built, in the style of `tests/admission_bound.rs`: what this test is
// about is what goes over a socket, and the real parser is what reads it.

fn register(from_port: u16, round: u32, authorization: Option<&str>) -> String {
    let credentials = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{from_port};branch=z9hG4bK-rg15-{round}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:{USERNAME}@example.test>;tag=rg15{round}\r\n\
         To: <sip:{USERNAME}@example.test>\r\n\
         Call-ID: rg15-{round}\r\n\
         CSeq: {} REGISTER\r\n\
         {credentials}\
         Contact: <sip:{USERNAME}@127.0.0.1:{from_port}>;expires=3600\r\n\
         Content-Length: 0\r\n\
         \r\n",
        round + 1,
    )
}

/// A syntactically complete answer to `nonce` whose digest is wrong — §3 A6, and the shape a
/// password-guessing run has. Nothing here is computed: a *correct* response is not needed to prove
/// that a refusal is recorded, and a wrong one is what the operator has to be able to count.
fn a_wrong_answer(nonce: &str) -> String {
    format!(
        "Digest username=\"{USERNAME}\", realm=\"{REALM}\", nonce=\"{nonce}\", \
         uri=\"sip:example.test\", response=\"{WRONG_RESPONSE}\", algorithm=SHA-256, \
         qop=auth, nc=00000001, cnonce=\"{CNONCE}\""
    )
}

/// The `nonce` a `WWW-Authenticate` offered.
fn nonce_of(challenge: &str) -> String {
    let (_, after) = challenge
        .split_once("nonce=\"")
        .expect("the challenge must carry a nonce");
    let (nonce, _) = after.split_once('"').expect("a terminated nonce");
    nonce.to_owned()
}

/// Send `message` to `port`, and return the first response that comes back.
///
/// Retried, because the node's socket comes up asynchronously and a lost datagram on loopback is
/// cheaper to re-send than to reason about.
async fn exchange(socket: &UdpSocket, port: u16, message: &str) -> String {
    let node = format!("127.0.0.1:{port}");
    let mut buffer = vec![0u8; 4096];
    for _ in 0..40 {
        socket
            .send_to(message.as_bytes(), &node)
            .await
            .expect("send");
        match tokio::time::timeout(Duration::from_millis(250), socket.recv_from(&mut buffer)).await
        {
            Ok(Ok((len, _))) => {
                return String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            // Nothing came back inside the budget; send it again.
            Err(_) => {}
        }
    }
    panic!("the node never answered");
}

/// The records the node emitted about authentication, one per line.
fn auth_records(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|line| line.contains("authentication"))
        .collect()
}

// ---------------------------------------------------------------------------------- the test ---

/// **The failing-first test for `RG-15`.** A refusal must be observable, with its reason.
///
// covers: RA-L-2, RA-L-3
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ra_l_1_a_refusal_is_recorded_with_its_reason_and_nothing_else() {
    let capture = Capture::default();
    tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::INFO)
        // Deliberately not raising the level: an operator who has to know to turn logging up has
        // not been told. `INFO` is what `main.rs` falls back to when `RUST_LOG` says nothing.
        .with_ansi(false)
        .try_init()
        .expect("this test owns the subscriber");

    let challenging = tokio::spawn(async move { driver::run(challenging_node()).await });
    let open = tokio::spawn(async move { driver::run(open_node()).await });

    let phone = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind the phone");
    let phone_port = phone.local_addr().expect("the phone's address").port();

    // A2 — no credentials offered, so a challenge. `RA-L-2`.
    let challenge = exchange(&phone, AUTHENTICATED_PORT, &register(phone_port, 1, None)).await;
    assert!(
        challenge.starts_with("SIP/2.0 401"),
        "a tenant that requires authentication must challenge; got:\n{challenge}"
    );

    // A6 — the challenge answered wrongly. This is the record an operator counts. `RA-L-1`.
    let nonce = nonce_of(&challenge);
    let refusal = exchange(
        &phone,
        AUTHENTICATED_PORT,
        &register(phone_port, 2, Some(&a_wrong_answer(&nonce))),
    )
    .await;
    assert!(
        refusal.starts_with("SIP/2.0 401"),
        "wrong credentials are a 401 without stale (§3 A6); got:\n{refusal}"
    );

    // A1 — an open tenant proceeds with no principal, and the trail has to *say* so. `RA-L-3`.
    let accepted = exchange(&phone, OPEN_PORT, &register(phone_port, 3, None)).await;
    assert!(
        accepted.starts_with("SIP/2.0 200"),
        "an open tenant registers without credentials; got:\n{accepted}"
    );

    challenging.abort();
    open.abort();

    let log = capture.text();
    let records = auth_records(&log);
    // Printed rather than only asserted: the records are the evidence, and a run that passes for
    // the wrong reason is visible here rather than inferred.
    println!("--- authentication records ---\n{}", records.join("\n"));

    // `RA-L-1` — the refusal, and why.
    assert!(
        records
            .iter()
            .any(|line| line.contains("authentication refused")),
        "a refused REGISTER must produce a record. The whole log was:\n{log}"
    );
    assert!(
        records
            .iter()
            .any(|line| line.contains("the credentials did not match")),
        "the refusal must carry the reason the decision already computed. The log was:\n{log}"
    );

    // `RA-L-2` — the first challenge is an outcome too, and it is not a refusal.
    assert!(
        records
            .iter()
            .any(|line| line.contains("authentication challenged")),
        "an unauthenticated first REGISTER must produce a record. The log was:\n{log}"
    );

    // `RA-L-3` — *unauthenticated* is written down rather than merely not written.
    assert!(
        records
            .iter()
            .any(|line| line.contains("unauthenticated") && line.contains(OPEN_TENANT)),
        "an open tenant's REGISTER must record that nobody was authenticated. The log was:\n{log}"
    );

    // `RA-L-2`'s other half: nothing the far end sent may ride into the record with it. A log line
    // is the artefact most likely to be copied into an issue — the same argument
    // `StoreChoice::describe()` makes about a resolved DSN, applied to the material an attacker
    // controls.
    //
    // `USERNAME` is in this list because **nothing in this scenario authenticates**: the challenging
    // node refuses both attempts and the open node proceeds with `principal: None`, so no proven
    // identity exists for a record to name. §5's principal *is* nameable on a success — it is the
    // identity the digest proved, and it is already stored on the binding — and a future test that
    // authenticates should assert it appears rather than adding it here.
    for secret in [PASSWORD, CNONCE, WRONG_RESPONSE, nonce.as_str(), USERNAME] {
        assert!(
            !log.contains(secret),
            "credential material reached a log line: {secret:?}\nThe log was:\n{log}"
        );
    }
}
