//! Where a request goes — §5 preprocessing, §5.1 target determination, and F7's next hop.
//!
//! These are the routing decisions two entry points share: the [response
//! context](crate::context::ResponseContext), which owns a server transaction, and
//! [`route_ack`](crate::ack::route_ack), which owns nothing because an `ACK` for a 2xx has no
//! transaction and no response (§7.2 K3). They live here rather than in either one so that "the
//! core's route preprocessing" means the same code on both paths — an `ACK` that took a private
//! shortcut past it is `V-03`, and a shortcut is exactly what a second copy of these rules would be.
//!
//! Nothing here reads a clock, a socket or a store. Which URI's *address* the next hop resolves to is
//! the driver's (RFC 3263, and `RT-1`'s route plan); which URI it **is** is decided here.

use bytes::Bytes;
use sipx_sip::headers::Address;
use sipx_sip::{HeaderName, Request};

use crate::config::ProxyConfig;
use crate::types::{Target, TargetQuery};

/// The `q` a predetermined target carries: the only one there is, so the most preferred.
///
/// Stated in thousandths like every other `q` here (§7 L2), rather than left to a literal at the one
/// call site, because a target that sorted below a hypothetical second one would fork in sequence
/// behind it.
const SOLE_TARGET_Q: u16 = 1_000;

/// §5's P1 and P2, in order. Mutates the request the way §16.4 says to.
///
/// P3's rejection arrives as [`crate::Input::TokenFact`] instead, because verification needs the
/// keys and the keys are the driver's — so there is nothing for this function to do about a token
/// beyond leaving the popped `Route` where the caller can see it.
pub(crate) fn preprocess(request: &mut Request, config: &ProxyConfig) {
    // P1 — a strict-routing predecessor put our Record-Route value in the Request-URI. The real
    // Request-URI is the last Route; recover it and drop that Route.
    if is_our_record_route_value(request, config) {
        let routes = routes_of(request);
        if let Some(last) = routes.last()
            && let Ok(address) = Address::parse(last, "Route")
        {
            request.uri = address.uri.clone();
            set_routes(
                request,
                routes.iter().take(routes.len().saturating_sub(1)).cloned(),
            );
        }
    }

    // P2 — the first Route resolves to this platform: pop it. Any edge pops any edge's Route,
    // which is the point of the token.
    let routes = routes_of(request);
    if let Some(first) = routes.first()
        && let Ok(address) = Address::parse(first, "Route")
        && config.is_ours(&address.uri)
    {
        set_routes(request, routes.iter().skip(1).cloned());
    }
}

/// Whether the Request-URI is a value **this platform placed in a `Record-Route`** — P1's condition,
/// as §16.4 states it rather than as an edge-identity test.
///
/// `is_ours` alone is far weaker than the rule: an edge identity is host-scoped and port-agnostic on
/// purpose (§5), so it matches every URI at the edge's host — including the contact of any device
/// deployed beside it. Read that way, P1 fires on an ordinary mid-dialog request, replaces its
/// Request-URI with our own `Record-Route` and consumes its `Route`: the dialog's remote target is
/// gone and the request is addressed to us. A loopback deployment (`scripts/two-node-call.sh`: the
/// edge on `127.0.0.1`, its phones on `127.0.0.1`) is precisely that case.
///
/// So the shape is checked too. Every value this platform places is
/// [`Listener::record_route_uri`](https://github.com/codewandler/sipx-clstr/blob/main/crates/sipx-clstr-node/src/listen.rs)'s:
/// no user part, and `lr`. A strict router copies that value verbatim into the Request-URI, so
/// requiring both loses nothing P1 is for, and excludes every contact — which always has a user, or
/// no `lr`, or both.
fn is_our_record_route_value(request: &Request, config: &ProxyConfig) -> bool {
    config.is_ours(&request.uri)
        && request.uri.user().is_none()
        && request
            .uri
            .params()
            .is_some_and(|params| params.get("lr").is_some())
}

/// §5.1 T1 — whether the request is within a dialog, and its target therefore predetermined.
///
/// A `To` tag is the definition (§12.2): the tag is chosen by the far end when the dialog is
/// established, so a request carrying one is addressed *inside* one and its Request-URI is the
/// dialog's remote target (§12.2.1.1) — a contact, never an address of record.
pub(crate) fn is_in_dialog(request: &Request) -> bool {
    request
        .headers
        .value(&HeaderName::To)
        .and_then(|value| Address::parse(&value, "To").ok())
        .is_some_and(|address| address.tag().is_some())
}

/// §5.1 T1 — the predetermined target set: the Request-URI, and nothing else.
///
/// The route set is empty rather than the surviving `Route` values: those are already *in* the
/// message and [`crate::forward`] prepends a target's route set to them, so copying them here would
/// traverse every hop twice.
pub(crate) fn predetermined_target(request: &Request) -> Target {
    Target {
        uri: request.uri.to_bytes(),
        route_set: Vec::new(),
        q: SOLE_TARGET_Q,
    }
}

/// §5.1 T2 — the URI whose registrations are wanted: the first `Route` if one survived
/// preprocessing, else the Request-URI.
pub(crate) fn aor_query(request: &Request) -> TargetQuery {
    TargetQuery {
        uri: first_route_uri(request).unwrap_or_else(|| request.uri.to_bytes()),
    }
}

/// F7 — the next hop of a **forwarded copy**, read off the copy after F6.
///
/// The first `Route` when it carries `lr`, else the Request-URI. The `lr` test is what makes one rule
/// cover both routing styles: F6 has already moved a strict router's URI into the Request-URI and
/// pushed the real destination to the end of the `Route` set, so a first `Route` without `lr` is the
/// far end rather than the next hop, and following it would skip the router the swap exists to
/// traverse.
pub(crate) fn next_hop_of(request: &Request) -> Bytes {
    match first_route(request) {
        Some(address)
            if address
                .uri
                .params()
                .is_some_and(|params| params.get("lr").is_some()) =>
        {
            address.uri.to_bytes()
        }
        _ => request.uri.to_bytes(),
    }
}

fn first_route(request: &Request) -> Option<Address> {
    let value = request.headers.value(&HeaderName::Route)?;
    Address::parse(&value, "Route").ok()
}

fn first_route_uri(request: &Request) -> Option<Bytes> {
    first_route(request).map(|address| address.uri.to_bytes())
}

fn routes_of(request: &Request) -> Vec<Bytes> {
    request
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
        .collect()
}

/// Replace the whole `Route` set with `values`, in order.
fn set_routes(request: &mut Request, values: impl Iterator<Item = Bytes>) {
    request.headers.remove_all(&HeaderName::Route);
    for value in values {
        if let Ok(header) = sipx_sip::Header::build(HeaderName::Route, value) {
            request.headers.push(header);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::config::CookieKey;
    use sipx_sip::{Method, RequestBuilder, Uri};

    fn config() -> ProxyConfig {
        ProxyConfig::new(
            "edge-1.example",
            Bytes::from_static(b"<sip:edge-1.example:5060;lr>"),
            CookieKey::new(Bytes::from_static(b"key")),
        )
    }

    fn request(uri: &str, to: &str, routes: &[&str]) -> Request {
        let mut builder = RequestBuilder::new(
            Method::Bye,
            Uri::parse(Bytes::copy_from_slice(uri.as_bytes())).expect("a URI"),
        )
        .header(HeaderName::CallId, "route-tests")
        .unwrap()
        .cseq(2, &Method::Bye)
        .unwrap()
        .header(HeaderName::From, "<sip:alice@a.example>;tag=af")
        .unwrap()
        .header(HeaderName::To, to.to_owned())
        .unwrap()
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP alice.example;branch=z9hG4bK-in",
        )
        .unwrap();
        for route in routes {
            builder = builder
                .header(HeaderName::Route, (*route).to_owned())
                .unwrap();
        }
        builder.build()
    }

    fn route_values(request: &Request) -> Vec<String> {
        routes_of(request)
            .iter()
            .map(|value| String::from_utf8_lossy(value).trim().to_owned())
            .collect()
    }

    #[test]
    fn a_to_tag_is_what_makes_a_request_in_dialog() {
        assert!(is_in_dialog(&request(
            "sip:bob@10.0.0.1:5062",
            "<sip:bob@b.example>;tag=bt",
            &[]
        )));
        assert!(!is_in_dialog(&request(
            "sip:bob@b.example",
            "<sip:bob@b.example>",
            &[]
        )));
    }

    #[test]
    fn our_own_route_is_popped_and_the_remote_target_is_left_alone() {
        // The ordinary mid-dialog case, and the one that mattered: the edge and the device share a
        // host, so P1's identity test alone would have consumed the Route and lost the target.
        let mut request = request(
            "sip:bob@edge-1.example:5062",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:edge-1.example:5060;lr>"],
        );
        preprocess(&mut request, &config());
        assert_eq!(
            String::from_utf8_lossy(&request.uri.to_bytes()),
            "sip:bob@edge-1.example:5062"
        );
        assert!(route_values(&request).is_empty(), "P2 pops our own Route");
    }

    #[test]
    fn a_strict_routing_predecessor_is_still_recovered() {
        // P1 proper: the Request-URI really is a value we placed — no user, `lr` — so the last Route
        // is the real destination.
        let mut request = request(
            "sip:edge-1.example:5060;lr",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:bob@10.0.0.1:5062>"],
        );
        preprocess(&mut request, &config());
        assert_eq!(
            String::from_utf8_lossy(&request.uri.to_bytes()),
            "sip:bob@10.0.0.1:5062"
        );
        assert!(route_values(&request).is_empty());
    }

    #[test]
    fn the_next_hop_is_a_surviving_loose_route_and_otherwise_the_request_uri() {
        let with_route = request(
            "sip:bob@10.0.0.1:5062",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:p2.example;lr>"],
        );
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&with_route)),
            "sip:p2.example;lr"
        );

        let bare = request("sip:bob@10.0.0.1:5062", "<sip:bob@b.example>;tag=bt", &[]);
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&bare)),
            "sip:bob@10.0.0.1:5062"
        );
    }

    #[test]
    fn a_strict_first_route_is_not_the_next_hop() {
        // After F6 the strict router is in the Request-URI and the real destination is the last
        // Route. Following the Route would skip the very hop the swap exists to traverse.
        let swapped = request(
            "sip:strict.example",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:bob@10.0.0.1:5062>"],
        );
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&swapped)),
            "sip:strict.example"
        );
    }

    #[test]
    fn the_predetermined_target_is_the_request_uri_and_carries_no_route_set() {
        let request = request("sip:bob@10.0.0.1:5062", "<sip:bob@b.example>;tag=bt", &[]);
        let target = predetermined_target(&request);
        assert_eq!(
            String::from_utf8_lossy(&target.uri),
            "sip:bob@10.0.0.1:5062"
        );
        assert!(target.route_set.is_empty());
    }
}
