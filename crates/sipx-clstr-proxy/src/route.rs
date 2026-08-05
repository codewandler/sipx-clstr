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
use sipx_clstr_affinity::TOKEN_PARAM;
use sipx_sip::headers::Address;
use sipx_sip::{HeaderName, Request, Uri};

use crate::config::ProxyConfig;
use crate::types::{Target, TargetQuery, TokenCarriage};

/// The `q` a predetermined target carries: the only one there is, so the most preferred.
///
/// Stated in thousandths like every other `q` here (§7 L2), rather than left to a literal at the one
/// call site, because a target that sorted below a hypothetical second one would fork in sequence
/// behind it.
const SOLE_TARGET_Q: u16 = 1_000;

/// §5's P1 and P2, in order. Mutates the request the way §16.4 says to, and **returns what the
/// popped platform `Route`s carried** — the tokens P2 hands to verification.
///
/// P3's rejection arrives as [`crate::Input::TokenFact`], because verification needs the keys and
/// the keys are the driver's. What this function owns is everything up to that: which `Route`s are
/// ours, how many of them there are, and which `aft` values they carried. It is deliberately the
/// only place that answers those questions — both entry points call it, and a private second copy of
/// the rules is what `V-03` was.
pub(crate) fn preprocess(request: &mut Request, config: &ProxyConfig) -> Option<TokenCarriage> {
    // P1 — a strict-routing predecessor put our Record-Route value in the Request-URI. The real
    // Request-URI is the last Route; recover it and drop that Route.
    if is_our_record_route_value(request, config) {
        let routes = routes_of(request);
        if let Some(last) = routes.last()
            && let Ok(address) = Address::parse(last, "Route")
        {
            // `set_uri`, never field assignment: a parsed request keeps its raw start-line bytes
            // for exact forwarding, and they stop being truthful the moment the target changes.
            request.set_uri(address.uri.clone());
            set_routes(
                request,
                routes.iter().take(routes.len().saturating_sub(1)).cloned(),
            );
        }
    }

    // P2 — the leading `Route`s that resolve to this platform are popped. Any edge pops any edge's
    // Route, which is the point of the token.
    //
    // Up to **two**, because affinity-token §7 M2 mints a pair — one entry per side — and §8 S9's
    // consistency check is defined over exactly that shape: "when the proxy popped two consecutive
    // platform Routes". A third would not be ours to pop; a single one is the heterogeneous case §8
    // allows and Path (M7) produces by nature, and it is processed on its own token alone.
    let routes = routes_of(request);
    let mut popped: Vec<Option<Bytes>> = Vec::new();
    for value in routes.iter().take(2) {
        // A `Route` we cannot parse is one we cannot recognize as ours, so P2 does not fire for it
        // and it survives to the next hop untouched — the same answer the base gave, and the one
        // non-negotiable #3 requires: a malformed `Route` arrives from the wire and must not take
        // the node down or be guessed at.
        let Ok(address) = Address::parse(value, "Route") else {
            break;
        };
        if !config.is_ours(&address.uri) {
            break;
        }
        popped.push(token_of(&address.uri));
    }
    // Nothing of ours at the front: P2 does not fire, and the `Route` set is left byte-for-byte
    // alone rather than removed and rebuilt.
    let first = popped.first().cloned()?;
    set_routes(request, routes.iter().skip(popped.len()).cloned());

    // §8 S9: "The **first-popped** token governs (its direction names the presenting side)."
    let token = first?;
    Some(TokenCarriage {
        token,
        // A partner is only a partner when it is one. A second platform `Route` that carried no
        // token is not half of a pair, and offering it as one would let an entry stripped in flight
        // weaken S9's check into silence rather than fail it.
        partner: popped.get(1).cloned().flatten(),
    })
}

/// The `aft` parameter's value on a platform URI (§5).
///
/// §5 requires **exactly one** `aft` per platform URI, and this reads one because there can only be
/// one: RFC 3261 §19.1.1 forbids a repeated `uri-parameter` name outright, and the kernel enforces
/// it at parse time — including spelling variants, since §19.1.4 makes `%61ft` the name `aft`. A
/// URI carrying two of them never becomes an `Address` at all, so it is refused a layer below this
/// one rather than tolerated here.
fn token_of(uri: &Uri) -> Option<Bytes> {
    // §19.1.4 for the name: case-insensitive, with escapes of unreserved characters folded.
    // `Params::value` compares through `has_name`, which is the kernel's implementation of exactly
    // that rule — a hand-rolled `== "aft"` would miss `Aft` and `%61ft`.
    //
    // The *value* is case-**sensitive** (§5, base64url) and is never compared as a string anywhere:
    // every decision downstream runs on the authenticated bytes it decodes to. Non-canonical
    // base64url is accepted by design — §5 names exactly two rejections, padding and
    // out-of-alphabet bytes — so two distinct parameter strings can carry one token, and comparing
    // the strings for equality anywhere is what would turn that from harmless into a hazard.
    uri.params()?.value(TOKEN_PARAM).map(Bytes::copy_from_slice)
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

/// §5.1 T3 — whether a `Route` set survived preprocessing, which predetermines the target set for
/// an out-of-dialog request exactly as a `To` tag does for an in-dialog one.
///
/// A surviving `Route` means an upstream element already routed this request: this node is a hop on
/// a pre-established path, not the element that retargets. Retargeting is done by whatever the path
/// ends at, when the request reaches it with no `Route` left — at which point T2 applies there.
pub(crate) fn route_set_survived(request: &Request) -> bool {
    request.headers.value(&HeaderName::Route).is_some()
}

/// §5.1 T2 — the URI whose registrations are wanted: the Request-URI, always.
///
/// It used to be the first `Route` when one survived preprocessing, and that was `PX-16`'s defect:
/// a `Route` names a proxy, an address of record names a user, and asking the location service the
/// former as the latter produced an answer F2 then wrote over the callee's URI. T2 is only reached
/// when nothing survived ([`route_set_survived`] gates it), so the Request-URI is the only URI
/// there is a question about.
pub(crate) fn aor_query(request: &Request) -> TargetQuery {
    TargetQuery {
        uri: request.uri.to_bytes(),
    }
}

/// F7 — the next hop of a **forwarded copy**, read off the copy after F6 *and off F6's answer*.
///
/// RFC 3261 §16.6 step 7 conditions the choice on whether step 6 reformatted the copy for a strict
/// router, not on what the first `Route` looks like afterwards: when it did, the next hop is the
/// Request-URI; otherwise the first `Route` if present, else the Request-URI. The `lr` test this
/// rule used to lean on is not total across F6 — `[strict, p2;lr]` swaps to a first `Route` that
/// carries `lr` while the strict router sits in the Request-URI, so reading `lr` there follows `p2`
/// and skips the very router the swap exists to traverse (`PX-16`).
pub(crate) fn next_hop_of(request: &Request, reformatted_for_strict_router: bool) -> Bytes {
    if reformatted_for_strict_router {
        return request.uri.to_bytes();
    }
    match first_route(request) {
        Some(address) => address.uri.to_bytes(),
        // Also the fallback for a first `Route` that does not parse: a hop this node cannot name
        // as a URI cannot be handed to a driver, and the Request-URI is the honest remainder of
        // step 7's rule — the same answer `preprocess` gives a `Route` it cannot read.
        None => request.uri.to_bytes(),
    }
}

fn first_route(request: &Request) -> Option<Address> {
    let value = request.headers.value(&HeaderName::Route)?;
    Address::parse(&value, "Route").ok()
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
    fn the_next_hop_is_a_surviving_route_and_otherwise_the_request_uri() {
        let with_route = request(
            "sip:bob@10.0.0.1:5062",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:p2.example;lr>"],
        );
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&with_route, false)),
            "sip:p2.example;lr"
        );

        let bare = request("sip:bob@10.0.0.1:5062", "<sip:bob@b.example>;tag=bt", &[]);
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&bare, false)),
            "sip:bob@10.0.0.1:5062"
        );
    }

    #[test]
    fn f6s_answer_is_what_makes_the_request_uri_the_next_hop_not_the_first_routes_shape() {
        // The post-swap copy `[strict, p2;lr]` produces: the strict router in the Request-URI and
        // `p2;lr` as the first Route. The old `lr` reading followed `p2` here and skipped the
        // router the swap exists to traverse — RFC 3261 §16.6 step 7 asks whether step 6
        // reformatted the copy, not what the first Route looks like afterwards.
        let swapped = request(
            "sip:strict.example",
            "<sip:bob@b.example>;tag=bt",
            &["<sip:p2.example;lr>", "<sip:bob@10.0.0.1:5062>"],
        );
        assert_eq!(
            String::from_utf8_lossy(&next_hop_of(&swapped, true)),
            "sip:strict.example"
        );
    }

    #[test]
    fn a_surviving_route_set_predetermines_the_target_and_a_consumed_one_does_not() {
        let mut routed = request(
            "sip:bob@b.example",
            "<sip:bob@b.example>",
            &["<sip:edge-1.example:5060;lr>", "<sip:p2.example;lr>"],
        );
        preprocess(&mut routed, &config());
        assert!(
            route_set_survived(&routed),
            "P2 pops only our own entry; the rest of the path survives"
        );

        let mut consumed = request(
            "sip:bob@b.example",
            "<sip:bob@b.example>",
            &["<sip:edge-1.example:5060;lr>"],
        );
        preprocess(&mut consumed, &config());
        assert!(
            !route_set_survived(&consumed),
            "the whole route set was ours: T2 asks the location service about the Request-URI"
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
