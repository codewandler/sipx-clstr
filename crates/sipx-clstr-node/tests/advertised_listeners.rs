//! `DP-5` — a request received on the bound address is answerable at the advertised one.
//!
//! A node on a private address that is reached on a public one has two addresses, and only one of
//! them is an answer to "where do I reach you". Every place an address enters a message — the `Via`
//! sent-by (RFC 3261 §18.1.1), a `Contact` (§8.1.1.8), the `Record-Route` a proxy inserts (§16.6
//! step 4) — must carry the advertised one. The bound one is a fact about a socket, and a peer
//! cannot route to it.
//!
//! Which address that is, is a **decision**: a pure function of the listener's declared
//! configuration and the message that arrived (AGENTS.md #2). Nothing here binds a socket, and the
//! whole file runs in the deterministic harness.
//!
//! Why it is worth a test of its own rather than a line in a config parser: `Record-Route` is how a
//! mid-dialog request finds its way back (AGENTS.md #5 — state rides the message). An advertised
//! address that is wrong does not fail loudly at start-up; it fails on the second request of a
//! dialog, in production, as a call that cannot be transferred or hung up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_node::driver::{NodeConfig, proxy_config};
use sipx_clstr_node::listen::{Advertised, Listener, Listeners};
use sipx_clstr_proxy::{Effect, Input, Kind, ProxyConfig, ResponseContext, Target};
use sipx_sip::{HeaderName, Method, Request, RequestBuilder, Uri};
use sipx_transport::TransportKind;

/// What the node binds: a private address, reachable only from inside the host's network.
const BOUND_HOST: &str = "10.0.0.7";
/// What the node is reached on: the only address a peer can put in a packet.
const ADVERTISED_HOST: &str = "203.0.113.9";

/// The three transports the story names, with the port each is bound and advertised on.
///
/// TLS gets its own port because RFC 3261 §19.1.2 gives `sips` one (5061), and a peer connecting to
/// 5060 does not expect a handshake.
const LISTENERS: [(TransportKind, u16); 3] = [
    (TransportKind::Udp, 5060),
    (TransportKind::Tcp, 5060),
    (TransportKind::Tls, 5061),
];

/// A node that binds a private address on all three transports and advertises a public one.
fn node() -> NodeConfig {
    let listeners = LISTENERS.map(|(transport, port)| {
        Listener::new(
            transport,
            format!("{BOUND_HOST}:{port}").parse().unwrap(),
            Advertised::parse(&format!("{ADVERTISED_HOST}:{port}")).unwrap(),
        )
        .unwrap()
    });
    NodeConfig::listening(Listeners::new(listeners).unwrap())
}

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

fn invite() -> Request {
    RequestBuilder::new(Method::Invite, uri("sip:bob@b.example"))
        .header(HeaderName::CallId, "call-dp5")
        .and_then(|b| b.cseq(1, &Method::Invite))
        .and_then(|b| b.header(HeaderName::From, "<sip:alice@a.example>;tag=af"))
        .and_then(|b| b.header(HeaderName::To, "<sip:bob@b.example>"))
        .and_then(|b| {
            b.header(
                HeaderName::Via,
                "SIP/2.0/UDP phone.a.example;branch=z9hG4bK-caller",
            )
        })
        .and_then(|b| b.header(HeaderName::MaxForwards, "70"))
        .map(sipx_sip::RequestBuilder::build)
        .expect("a well-formed INVITE")
}

/// Drive one INVITE through the proxy the node runs for a request that arrived on `transport`,
/// and return the request as it leaves.
fn forwarded_over(config: &NodeConfig, transport: TransportKind) -> (ProxyConfig, Request) {
    let listener = config
        .listeners
        .receiving(transport)
        .expect("a listener for a declared transport");
    let proxy = proxy_config(config, Some(listener));
    let mut context = ResponseContext::new(proxy.clone());
    let mut effects = context.on_input(Input::Upstream(Box::new(invite())));
    if effects
        .iter()
        .any(|effect| effect.kind() == Kind::ResolveTargets)
    {
        effects.extend(context.on_input(Input::TargetsResolved(vec![Target {
            uri: Bytes::from_static(b"sip:bob@10.0.0.9"),
            route_set: Vec::new(),
            q: 1000,
        }])));
    }
    let forwarded = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Forward { request, .. } => Some(*request),
            _ => None,
        })
        .expect("the INVITE is forwarded");
    (proxy, forwarded)
}

fn header(request: &Request, name: &HeaderName) -> String {
    String::from_utf8_lossy(
        &request
            .headers
            .get(name)
            .unwrap_or_else(|| panic!("a {name:?} header"))
            .value(),
    )
    .trim()
    .to_owned()
}

/// The URI inside a `<...>`, with its parameters.
fn bracketed(value: &str) -> Uri {
    let inner = value
        .trim()
        .trim_start_matches('<')
        .split('>')
        .next()
        .unwrap_or(value);
    uri(inner)
}

/// The acceptance test: what arrives on the bound address is answerable at the advertised one.
///
/// "Answerable" is three claims, and each is checked for every transport the story names:
///
/// 1. the three places an address enters a message name the advertised address, and none of them
///    leaks the bound one;
/// 2. the `Record-Route` the node inserts is a URI the node itself recognizes as its own — which is
///    what makes the mid-dialog request it will come back in routable (proxy-behavior §5);
/// 3. it names the port and transport a peer must actually use, so the return trip lands on the
///    listener that made the promise rather than on whatever is behind 5060/UDP.
#[test]
fn a_request_received_on_the_bound_address_is_answerable_at_the_advertised_one() {
    let config = node();

    for (transport, port) in LISTENERS {
        let listener = config.listeners.receiving(transport).expect("a listener");
        let (proxy, forwarded) = forwarded_over(&config, transport);

        let places = [
            ("Via", header(&forwarded, &HeaderName::Via)),
            ("Record-Route", header(&forwarded, &HeaderName::RecordRoute)),
            (
                "Contact",
                String::from_utf8_lossy(&listener.contact_uri(Some("alice"))).into_owned(),
            ),
        ];

        for (place, value) in &places {
            assert!(
                value.contains(ADVERTISED_HOST),
                "{transport:?}: {place} `{value}` does not name the advertised address",
            );
            assert!(
                !value.contains(BOUND_HOST),
                "{transport:?}: {place} `{value}` leaks the bound address",
            );
        }

        // The sent-by exactly, not merely "contains": a `Via` that named the right host and the
        // wrong port is a response that arrives at whatever else is on 5060.
        //
        // The transport token before it is the proxy engine's, and it says `UDP` on every branch
        // regardless — that is the transport a forwarded request *leaves* on, which this node does
        // not yet choose per target. Out of this story's scope and recorded as such; the sent-by,
        // which is what a response is routed by, is the part that is decided here.
        let via = header(&forwarded, &HeaderName::Via);
        let sent_by = via
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.split(';').next())
            .unwrap_or_default();
        assert_eq!(
            sent_by,
            listener.sent_by(),
            "{transport:?}: the Via sent-by is not the advertised address",
        );

        // (2) The node answers to what it wrote down. A `Record-Route` naming an address this node
        // does not recognize is one it will forward back out again instead of popping — the loop
        // AGENTS.md #5 exists to prevent.
        let record_route = bracketed(&header(&forwarded, &HeaderName::RecordRoute));
        assert!(
            proxy.is_ours(&record_route),
            "{transport:?}: the node does not recognize its own Record-Route",
        );

        // (3) The return trip must land on this listener, not merely on this host.
        assert_eq!(
            record_route.port(),
            Some(port),
            "{transport:?}: the Record-Route names the wrong port",
        );
        let named = record_route.transport().and_then(TransportKind::parse);
        assert_eq!(
            named.unwrap_or(TransportKind::Udp),
            transport,
            "{transport:?}: the Record-Route does not name the transport to come back on",
        );
    }
}

/// A listener declares the two independently, and neither is derived from the other.
#[test]
fn a_listener_declares_bind_and_advertise_independently() {
    let config = node();
    for (transport, port) in LISTENERS {
        let listener = config.listeners.receiving(transport).expect("a listener");
        assert_eq!(listener.bind().to_string(), format!("{BOUND_HOST}:{port}"));
        assert_eq!(listener.advertised_host(), ADVERTISED_HOST);
        assert_eq!(listener.advertised_port(), port);
        assert_eq!(
            listener.sent_by(),
            format!("{ADVERTISED_HOST}:{port}"),
            "the Via sent-by is the advertised address (RFC 3261 §18.1.1)",
        );
    }
}

/// The endpoint the kernel is asked to bind carries both addresses, and does not confuse them.
///
/// The kernel already separates the two (`sipx_transport::Config::sent_by`); this asserts the
/// mapping onto it, which is the part that lives here.
#[test]
fn the_kernel_endpoint_binds_the_private_address_and_advertises_the_public_one() {
    let endpoint = node()
        .listeners
        .endpoint_config()
        .expect("a cleartext listener");
    assert_eq!(endpoint.bind.ip().to_string(), BOUND_HOST);
    assert_eq!(endpoint.sent_by, ADVERTISED_HOST);
    assert_eq!(endpoint.sent_by_port, Some(5060));
    assert!(endpoint.tcp, "a declared TCP listener is bound");
}
