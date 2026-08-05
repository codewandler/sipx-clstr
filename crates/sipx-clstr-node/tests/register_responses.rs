//! `RG-19` — the registrar's complete typed outcome survives the real node wire boundary.
//!
//! The sans-IO vectors already prove the decision. These tests start the driver, send REGISTER over
//! UDP, parse the response with the kernel, and compare the ordered response facts the decision
//! produced. A status-only renderer passes the core vectors and fails every test in this file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_node::driver::NodeConfig;
use sipx_sip::headers::address::Path;
use sipx_sip::headers::{Contact, Require, Supported, Unsupported};
use sipx_sip::{HeaderName, Message, Response, parse_datagram};
use tokio::net::UdpSocket;

fn node(min_expires: u32) -> NodeConfig {
    let mut config =
        NodeConfig::advertising(support::ephemeral(), "127.0.0.1:15119").expect("a node");
    config.policy.min_expires = min_expires;
    config
}

fn register(port: u16, branch: &str, extra_headers: &str) -> String {
    format!(
        "REGISTER sip:example.test SIP/2.0\r\n\
         Via: SIP/2.0/UDP 127.0.0.1:{port};branch=z9hG4bK-{branch}\r\n\
         Max-Forwards: 70\r\n\
         From: <sip:alice@example.test>;tag={branch}\r\n\
         To: <sip:alice@example.test>\r\n\
         Call-ID: {branch}\r\n\
         CSeq: 1 REGISTER\r\n\
         {extra_headers}\
         Content-Length: 0\r\n\r\n"
    )
}

async fn exchange(phone: &UdpSocket, node: &str, message: &str) -> Response {
    let mut buffer = vec![0u8; 8_192];
    for _ in 0..40 {
        phone
            .send_to(message.as_bytes(), node)
            .await
            .expect("send REGISTER");
        match tokio::time::timeout(Duration::from_millis(250), phone.recv_from(&mut buffer)).await {
            Ok(Ok((len, _))) => {
                let bytes = Bytes::copy_from_slice(buffer.get(..len).unwrap_or_default());
                return match parse_datagram(bytes, &sipx_sip::Limits::datagram())
                    .expect("the response parses")
                {
                    Message::Response(response) => response,
                    Message::Request(_) => panic!("the node sent a request instead of a response"),
                };
            }
            Ok(Err(error)) => panic!("receive failed: {error}"),
            Err(_) => {}
        }
    }
    panic!("the node never answered the REGISTER")
}

/// Just the registrar facts, in the order they appeared on the wire.
fn fact_headers(response: &Response) -> Vec<(String, String)> {
    response
        .headers
        .iter()
        .filter_map(|header| {
            let name = match header.name() {
                HeaderName::Contact
                | HeaderName::Path
                | HeaderName::Supported
                | HeaderName::Unsupported
                | HeaderName::Require
                | HeaderName::MinExpires => header.name().canonical(),
                _ => return None,
            };
            Some((
                String::from_utf8_lossy(name).into_owned(),
                String::from_utf8_lossy(&header.value()).into_owned(),
            ))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_r_11_interval_too_brief_names_the_minimum_on_the_wire() {
    let running = support::start_in_process(node(300)).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");
    let port = phone.local_addr().expect("phone address").port();
    let response = exchange(
        &phone,
        &running.target(),
        &register(
            port,
            "ls-r-11",
            "Contact: <sip:alice@127.0.0.1:17011>;expires=60\r\n",
        ),
    )
    .await;

    assert_eq!(response.status.code(), 423);
    assert_eq!(
        fact_headers(&response),
        vec![("Min-Expires".to_owned(), "300".to_owned())]
    );

    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_r_17_success_round_trips_every_contact_and_path_fact_in_order() {
    let running = support::start_in_process(node(60)).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");
    let port = phone.local_addr().expect("phone address").port();
    let response = exchange(
        &phone,
        &running.target(),
        &register(
            port,
            "ls-r-17",
            "Contact: <sip:alice@127.0.0.1:17017>;expires=3600;q=0.75, \
             <sip:alice@127.0.0.1:17018>;expires=7200;q=0.25\r\n\
             Path: <sip:p2.example;lr>, <sip:p1.example;lr>\r\n\
             Supported: path\r\n",
        ),
    )
    .await;

    assert_eq!(response.status.code(), 200);
    assert_eq!(
        fact_headers(&response),
        vec![
            (
                "Contact".to_owned(),
                "<sip:alice@127.0.0.1:17017>;expires=3600;q=0.750".to_owned(),
            ),
            (
                "Contact".to_owned(),
                "<sip:alice@127.0.0.1:17018>;expires=7200;q=0.250".to_owned(),
            ),
            ("Path".to_owned(), "<sip:p2.example;lr>".to_owned()),
            ("Path".to_owned(), "<sip:p1.example;lr>".to_owned()),
            ("Supported".to_owned(), "path".to_owned()),
        ]
    );

    let contacts = response
        .headers
        .typed_all::<Contact>()
        .collect::<Result<Vec<_>, _>>()
        .expect("typed Contact values");
    let contact_facts: Vec<_> = contacts
        .iter()
        .map(|contact| {
            (
                contact.uri.to_bytes(),
                contact.param("expires").map(Bytes::copy_from_slice),
                contact.param("q").map(Bytes::copy_from_slice),
            )
        })
        .collect();
    assert_eq!(
        contact_facts,
        vec![
            (
                Bytes::from_static(b"sip:alice@127.0.0.1:17017"),
                Some(Bytes::from_static(b"3600")),
                Some(Bytes::from_static(b"0.750")),
            ),
            (
                Bytes::from_static(b"sip:alice@127.0.0.1:17018"),
                Some(Bytes::from_static(b"7200")),
                Some(Bytes::from_static(b"0.250")),
            ),
        ]
    );

    let paths = response
        .headers
        .typed_all::<Path>()
        .collect::<Result<Vec<_>, _>>()
        .expect("typed Path values");
    assert_eq!(
        paths
            .iter()
            .map(|path| path.uri.to_bytes())
            .collect::<Vec<_>>(),
        [
            Bytes::from_static(b"sip:p2.example;lr"),
            Bytes::from_static(b"sip:p1.example;lr"),
        ]
    );
    let supported = response
        .headers
        .typed::<Supported>()
        .expect("Supported is present")
        .expect("Supported is typed");
    assert_eq!(supported.0.0, vec![b"path".to_vec()]);

    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_r_18_path_without_support_names_the_required_extension() {
    let running = support::start_in_process(node(60)).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");
    let port = phone.local_addr().expect("phone address").port();
    let response = exchange(
        &phone,
        &running.target(),
        &register(
            port,
            "ls-r-18",
            "Contact: <sip:alice@127.0.0.1:17018>;expires=3600\r\n\
             Path: <sip:p2.example;lr>\r\n",
        ),
    )
    .await;

    assert_eq!(response.status.code(), 421);
    assert_eq!(
        fact_headers(&response),
        vec![("Require".to_owned(), "path".to_owned())]
    );
    let required = response
        .headers
        .typed::<Require>()
        .expect("Require is present")
        .expect("Require is typed");
    assert_eq!(required.0.0, vec![b"path".to_vec()]);

    running.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ls_r_20_bad_extension_lists_every_offender_in_request_order() {
    let running = support::start_in_process(node(60)).await;
    let phone = UdpSocket::bind("127.0.0.1:0").await.expect("bind phone");
    let port = phone.local_addr().expect("phone address").port();
    let response = exchange(
        &phone,
        &running.target(),
        &register(
            port,
            "ls-r-20",
            "Require: first-unknown, path, second-unknown\r\n",
        ),
    )
    .await;

    assert_eq!(response.status.code(), 420);
    assert_eq!(
        fact_headers(&response),
        vec![(
            "Unsupported".to_owned(),
            "first-unknown, second-unknown".to_owned(),
        )]
    );
    let unsupported = response
        .headers
        .typed::<Unsupported>()
        .expect("Unsupported is present")
        .expect("Unsupported is typed");
    assert_eq!(
        unsupported.0.0,
        vec![b"first-unknown".to_vec(), b"second-unknown".to_vec()]
    );

    running.stop();
}
