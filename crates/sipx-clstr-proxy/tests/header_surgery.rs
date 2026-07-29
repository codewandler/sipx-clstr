//! Via surgery on the response path: what `PX-3` adopted, and what it must not have changed.
//!
//! RFC 3261 §16.7 step 2 has a proxy remove **the topmost** `Via` from a response and forward what
//! is left. Order is the whole meaning of that header — the remaining stack *is* the return path —
//! so the operation is "remove at a position", not "remove by name".
//!
//! Until sipx `S-15` the kernel offered only `remove_all`, so this repo rebuilt the header
//! collection around the one header it wanted gone. The rebuild was correct and it was the only
//! thing keeping the order intact; swapping it for `Headers::remove_first` moves that guarantee
//! into the kernel. These tests assert the guarantee directly rather than trusting either
//! implementation, because "our own Via is gone" — which is all the existing `PB-R` rows check —
//! is equally true of a response whose remaining Vias came back shuffled.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_proxy::{CookieKey, Effect, Input, Kind, ProxyConfig, ResponseContext, Target};
use sipx_sip::{
    Header, HeaderName, Method, Request, RequestBuilder, Response, ResponseBuilder, StatusCode, Uri,
};

const EDGE: &str = "edge-1.example";

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

fn config() -> ProxyConfig {
    ProxyConfig::new(
        EDGE,
        Bytes::from_static(b"<sip:edge-1.example;lr>"),
        CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
    )
}

/// An INVITE that already carries **three** `Via` headers, as one two hops downstream would.
///
/// Three rather than one: with a single upstream `Via` the proxy's own is the only thing above it
/// and any removal strategy looks correct. The bug this guards against — a rebuild that reverses,
/// drops or reorders what it copies — is only visible when there is more than one survivor.
fn invite_with_a_via_stack() -> Request {
    RequestBuilder::new(Method::Invite, uri("sip:bob@b.example"))
        .header(HeaderName::CallId, "call-surgery")
        .unwrap()
        .cseq(1, &Method::Invite)
        .unwrap()
        .header(HeaderName::From, "<sip:alice@a.example>;tag=af")
        .unwrap()
        .header(HeaderName::To, "<sip:bob@b.example>")
        .unwrap()
        // Topmost first, as they are stored and as they will come back.
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP hop-a.example;branch=z9hG4bK-a",
        )
        .unwrap()
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP hop-b.example;branch=z9hG4bK-b",
        )
        .unwrap()
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP hop-c.example;branch=z9hG4bK-c",
        )
        .unwrap()
        .header(HeaderName::MaxForwards, "70")
        .unwrap()
        .build()
}

fn header_texts(response: &Response, name: &HeaderName) -> Vec<String> {
    response
        .headers
        .get_all(name)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect()
}

/// Drive one INVITE to one target and answer its branch, returning what the proxy sends upstream.
fn forwarded_response_for(extra_response_headers: Vec<(HeaderName, &str)>) -> Response {
    let request = invite_with_a_via_stack();
    let mut context = ResponseContext::new(config());
    let mut effects = context.on_input(Input::Upstream(Box::new(request.clone())));
    if effects.iter().any(|e| e.kind() == Kind::ResolveTargets) {
        effects.extend(context.on_input(Input::TargetsResolved(vec![Target {
            uri: Bytes::from_static(b"sip:bob@10.0.0.9"),
            route_set: Vec::new(),
            q: 1000,
        }])));
    }

    // The request as it left us, which is what the far end echoes back: our own Via on top of the
    // three it arrived with.
    let (branch, forwarded) = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Forward {
                branch, request, ..
            } => Some((branch.clone(), (**request).clone())),
            _ => None,
        })
        .expect("a forwarded request");

    let mut response = ResponseBuilder::to_request(
        &forwarded,
        StatusCode::new(200).expect("200"),
        "OK".to_owned(),
    )
    .expect("a response")
    .build();
    for (name, value) in extra_response_headers {
        response
            .headers
            .push(Header::build(name, value.to_owned()).expect("a well-formed header"));
    }

    let out = context.on_input(Input::BranchResponse(Box::new(response), branch));
    out.iter()
        .find_map(|effect| match effect {
            Effect::Respond(response) => Some((**response).clone()),
            _ => None,
        })
        .expect("a response forwarded upstream")
}

#[test]
fn popping_our_via_leaves_the_rest_of_the_stack_in_order() {
    let response = forwarded_response_for(Vec::new());
    let vias = header_texts(&response, &HeaderName::Via);

    assert_eq!(
        vias.len(),
        3,
        "exactly one Via — ours — comes off: {vias:?}"
    );
    assert!(
        !vias.iter().any(|via| via.contains(EDGE)),
        "our own Via must be the one that went: {vias:?}"
    );
    // The assertion the PB-R rows do not make: the survivors are in the order they arrived, so the
    // response walks back the path it came in on. Compared as a whole sequence rather than
    // position by position, so a reordering is one failure showing the shuffle, not three.
    let hosts: Vec<&str> = vias
        .iter()
        .filter_map(|via| via.split_whitespace().nth(1))
        .filter_map(|sent_by| sent_by.split(';').next())
        .collect();
    assert_eq!(
        hosts,
        vec!["hop-a.example", "hop-b.example", "hop-c.example"],
        "the remaining stack must keep its arrival order: {vias:?}"
    );
}

#[test]
fn popping_a_via_disturbs_no_other_header() {
    // Headers that sit *around* the Via stack. A rebuild that copied selectively, or an
    // `insert`-based replacement that guessed an index, would show up here rather than in the Via
    // assertions above.
    let response = forwarded_response_for(vec![
        (HeaderName::Other(Bytes::from_static(b"X-First")), "one"),
        (HeaderName::Other(Bytes::from_static(b"X-Second")), "two"),
    ]);

    assert_eq!(
        header_texts(
            &response,
            &HeaderName::Other(Bytes::from_static(b"X-First"))
        ),
        vec!["one".to_owned()],
    );
    assert_eq!(
        header_texts(
            &response,
            &HeaderName::Other(Bytes::from_static(b"X-Second"))
        ),
        vec!["two".to_owned()],
    );
    assert_eq!(
        header_texts(&response, &HeaderName::CallId),
        vec!["call-surgery".to_owned()],
        "the dialog identifiers survive the surgery untouched"
    );
}

/// The acceptance criterion, asserted against the source rather than inferred from behaviour.
///
/// `PX-3` says the mutations use the upstream API *exclusively*. Behaviour cannot show that — a
/// reintroduced rebuild would pass every test above. This reads the crate's own source for the
/// shape of the thing that was removed: constructing a fresh `Headers` in order to drop one
/// header out of an existing collection.
#[test]
fn no_site_rebuilds_the_header_collection_to_remove_one_header() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();

    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the crate has a src directory") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            if text.contains("Headers::new()") {
                offenders.push(path.display().to_string());
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these files build a fresh `Headers`, which is how header removal was written before sipx \
         S-15 shipped `remove_first`/`retain`: {offenders:?}"
    );
}
