//! RG-18's authorization boundary over the real node and a real UDP socket.
//!
//! Alice has valid credentials. That proves who she is; it must not prove that she may write Bob's
//! address of record. Both the ordinary Contact path and the wildcard path are exercised because a
//! wildcard is the quiet, whole-AoR form of registration hijacking.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use sipx_clstr_node::driver::{AuthConfig, NodeConfig};
use sipx_clstr_registrar::{CanonicalAor, InMemoryCredentials, RegistrationAuthorizations};
use tokio::net::UdpSocket;

const TENANT: &str = "t1";
const REALM: &str = "example.test";
const ALICE_PASSWORD: &str = "alice-secret";
const BOB_PASSWORD: &str = "bob-secret";

#[derive(Clone, Copy)]
enum Attack {
    Explicit,
    Wildcard,
}

fn node() -> NodeConfig {
    let mut config =
        NodeConfig::advertising(support::ephemeral(), "127.0.0.1:15118").expect("a loopback node");
    TENANT.clone_into(&mut config.tenant);
    config.domains = vec![REALM.to_owned()];
    config.auth = Some(AuthConfig {
        realm: REALM.to_owned(),
        secret: [0x18; 32],
        credentials: InMemoryCredentials::new()
            .with(TENANT, "alice", ALICE_PASSWORD)
            .with(TENANT, "bob", BOB_PASSWORD),
    });
    config.registration_authorizations = RegistrationAuthorizations::restricted()
        .allow(
            bytes::Bytes::from_static(b"t1:alice"),
            CanonicalAor::parse(bytes::Bytes::from_static(b"sip:alice@example.test"))
                .expect("Alice's AoR"),
        )
        .allow(
            bytes::Bytes::from_static(b"t1:bob"),
            CanonicalAor::parse(bytes::Bytes::from_static(b"sip:bob@example.test"))
                .expect("Bob's AoR"),
        );
    config
}

fn request(
    port: u16,
    branch: &str,
    to: &str,
    call_id: &str,
    cseq: u32,
    contact: Option<&str>,
    authorization: Option<&str>,
) -> String {
    let contact = contact
        .map(|value| format!("Contact: {value}\r\n"))
        .unwrap_or_default();
    let expires = if contact == "Contact: *\r\n" {
        "Expires: 0\r\n"
    } else {
        ""
    };
    let authorization = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "REGISTER sip:{REALM} SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{port};branch=z9hG4bK-{branch}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:{to}@{REALM}>;tag={branch}\r\n\
         To: <sip:{to}@{REALM}>\r\n\
         Call-ID: {call_id}\r\n\
         CSeq: {cseq} REGISTER\r\n\
         {authorization}{contact}{expires}\
         Content-Length: 0\r\n\r\n"
    )
}

fn open_request(port: u16, branch: &str, request_uri: &str, to: &str) -> String {
    format!(
        "REGISTER {request_uri} SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{port};branch=z9hG4bK-{branch}\r\n\
         Max-Forwards: 70\r\n\
         From: <{to}>;tag={branch}\r\n\
         To: <{to}>\r\n\
         Call-ID: {branch}\r\n\
         CSeq: 1 REGISTER\r\n\
         Contact: <sip:phone@127.0.0.1:{port}>;expires=3600\r\n\
         Content-Length: 0\r\n\r\n"
    )
}

async fn exchange(phone: &UdpSocket, node: &str, message: &str) -> String {
    let mut buffer = vec![0u8; 4096];
    for _ in 0..40 {
        phone.send_to(message.as_bytes(), node).await.expect("send");
        match tokio::time::timeout(Duration::from_millis(250), phone.recv_from(&mut buffer)).await {
            Ok(Ok((len, _))) => {
                return String::from_utf8_lossy(buffer.get(..len).unwrap_or_default()).into_owned();
            }
            Ok(Err(error)) => panic!("recv failed: {error}"),
            Err(_) => {}
        }
    }
    panic!("the node never answered")
}

fn challenge_value(response: &str) -> &str {
    response
        .lines()
        .find_map(|line| line.strip_prefix("WWW-Authenticate: "))
        .expect("a digest challenge")
}

async fn authenticated_register(
    phone: &UdpSocket,
    node: &str,
    credentials: (&str, &str),
    to: &str,
    tag: &str,
    nonce_count: u32,
    contact: Option<&str>,
) -> String {
    let (user, password) = credentials;
    let port = phone.local_addr().expect("phone address").port();
    let bare = request(port, tag, to, tag, 1, contact, None);
    let challenge = exchange(phone, node, &bare).await;
    assert!(challenge.starts_with("SIP/2.0 401"), "{challenge}");
    let parsed = sipx_ua::auth::Challenge::parse(challenge_value(&challenge).as_bytes(), false)
        .expect("the challenge parses");
    let authorization = sipx_ua::auth::respond(
        &parsed,
        &sipx_ua::auth::Credentials::new(user, password),
        "REGISTER",
        &format!("sip:{REALM}"),
        nonce_count,
        tag,
    );
    let answered_branch = format!("{tag}-answer");
    let answered = request(
        port,
        &answered_branch,
        to,
        tag,
        2,
        contact,
        Some(&authorization),
    );
    exchange(phone, node, &answered).await
}

async fn denied_attack(attack: Attack) {
    let running = support::start_in_process(node()).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");

    let bob_contact = "<sip:bob@127.0.0.1:17018>;expires=3600";
    let bob = authenticated_register(
        &phone,
        &running.target(),
        ("bob", BOB_PASSWORD),
        "bob",
        "bob-fixture",
        1,
        Some(bob_contact),
    )
    .await;
    assert!(bob.starts_with("SIP/2.0 200"), "{bob}");

    let attack_contact = match attack {
        Attack::Explicit => Some("<sip:mallory@127.0.0.1:6666>;expires=3600"),
        Attack::Wildcard => Some("*"),
    };
    let attack = authenticated_register(
        &phone,
        &running.target(),
        ("alice", ALICE_PASSWORD),
        "bob",
        "alice-attacks-bob",
        2,
        attack_contact,
    )
    .await;

    // LS-A-3 / S4. At the merge base both assertions fail with `200`: valid Alice credentials are
    // treated as authorization to mutate Bob's AoR.
    assert!(
        attack.starts_with("SIP/2.0 403"),
        "Alice authenticated but is not authorized for Bob; got:\n{attack}"
    );

    let query = authenticated_register(
        &phone,
        &running.target(),
        ("bob", BOB_PASSWORD),
        "bob",
        "bob-query",
        3,
        None,
    )
    .await;
    assert!(query.starts_with("SIP/2.0 200"), "{query}");
    assert!(query.contains("sip:bob@127.0.0.1:17018"), "{query}");
    assert!(!query.contains("mallory"), "{query}");

    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_a_1_and_2_request_uri_and_to_domains_are_distinct_on_the_wire() {
    let mut config =
        NodeConfig::advertising(support::ephemeral(), "127.0.0.1:15119").expect("a loopback node");
    TENANT.clone_into(&mut config.tenant);
    config.domains = vec![REALM.to_owned()];
    let running = support::start_in_process(config).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");
    let port = phone.local_addr().expect("phone address").port();

    let unserved_request_uri = exchange(
        &phone,
        &running.target(),
        &open_request(
            port,
            "unserved-request-uri",
            "sip:biloxi.example",
            "sip:alice@example.test",
        ),
    )
    .await;
    assert!(
        unserved_request_uri.starts_with("SIP/2.0 404"),
        "S1 is about the Request-URI, not the served To:\n{unserved_request_uri}"
    );

    let unserved_to = exchange(
        &phone,
        &running.target(),
        &open_request(
            port,
            "unserved-to",
            "sip:example.test",
            "sip:alice@biloxi.example",
        ),
    )
    .await;
    assert!(
        unserved_to.starts_with("SIP/2.0 404"),
        "S5 is about To even when the Request-URI is served:\n{unserved_to}"
    );

    let case_and_port = exchange(
        &phone,
        &running.target(),
        &open_request(
            port,
            "typed-authority",
            "sip:registrar@EXAMPLE.TEST:5070",
            "sip:alice@example.test",
        ),
    )
    .await;
    assert!(
        case_and_port.starts_with("SIP/2.0 200"),
        "host case and an explicit Request-URI port do not create another domain:\n{case_and_port}"
    );

    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_a_3_alice_cannot_register_a_contact_for_bob() {
    denied_attack(Attack::Explicit).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_a_3_alice_cannot_wildcard_deregister_bob() {
    denied_attack(Attack::Wildcard).await;
}
