//! The `PB-V`, `PB-P`, `PB-F`, `PB-R` and `PB-T` vector tables of
//! [proxy-behavior §12](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md),
//! row by row.
//!
//! `PB-C-*` (CANCEL and Timer C) is `PX-6`. `PB-S-*` (stateless) is `PX-4`, deferred to M2 because
//! nothing consumes it until mid-dialog requests carry tokens. `PB-A-*` (transaction affinity)
//! needs more than one node and is M2's.
//!
//! `PB-T-*` is `PX-14`, and it is here because rows proved one at a time do not prove their
//! composition: sequential forking (`PB-F-2`) and the terminal results (`PB-R-3`, `PB-R-7`,
//! `PB-C-2`) were each green while a `487` settling a cancellation re-originated an answered call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bytes::Bytes;
use sipx_clstr_proxy::{
    AckRefusal, AckRoute, BranchId, CookieKey, DEFAULT_TIMER_C, Effect, Input, Kind, ProxyConfig,
    ProxyTimer, Refusal, ResponseContext, TIMER_C_FLOOR, Target, TokenVerdict, route_ack,
};
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

/// A well-formed INVITE, with whatever extra headers a row needs.
fn invite(request_uri: &str, extra: Vec<(HeaderName, &str)>) -> Request {
    request(&Method::Invite, request_uri, extra)
}

fn request(method: &Method, request_uri: &str, extra: Vec<(HeaderName, &str)>) -> Request {
    // `To` is taken from `extra` when a row supplies one, rather than added twice: `Headers::value`
    // returns the *first* occurrence, so a second `To` would be silently ignored and a re-INVITE
    // test would quietly assert nothing.
    let to = extra
        .iter()
        .find(|(name, _)| name == &HeaderName::To)
        .map_or("<sip:bob@b.example>", |(_, value)| value);

    let mut builder = RequestBuilder::new(method.clone(), uri(request_uri))
        .header(HeaderName::CallId, "call-1")
        .unwrap()
        .cseq(1, method)
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
    for (name, value) in extra {
        if name == HeaderName::To {
            continue;
        }
        builder = builder.header(name, value.to_owned()).unwrap();
    }
    builder.build()
}

fn target(contact: &str, q: u16) -> Target {
    Target {
        uri: Bytes::copy_from_slice(contact.as_bytes()),
        route_set: Vec::new(),
        q,
    }
}

/// Feed a request and its resolved targets; return everything that came back.
fn run(request: Request, targets: Vec<Target>) -> (ResponseContext, Vec<Effect>) {
    let mut context = ResponseContext::new(config());
    let mut effects = context.on_input(Input::Upstream(Box::new(request)));
    if effects.iter().any(|e| e.kind() == Kind::ResolveTargets) {
        effects.extend(context.on_input(Input::TargetsResolved(targets)));
    }
    (context, effects)
}

fn statuses(effects: &[Effect]) -> Vec<u16> {
    effects.iter().filter_map(Effect::status).collect()
}

fn header_text(request: &Request, name: &HeaderName) -> Option<String> {
    request
        .headers
        .value(name)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
}

fn branches(effects: &[Effect]) -> Vec<BranchId> {
    effects
        .iter()
        .filter(|effect| effect.kind() == Kind::Forward)
        .filter_map(|effect| effect.branch().cloned())
        .collect()
}

/// A response from a branch, carrying that branch's `Via` on top the way a real one would.
fn branch_response(request: &Request, branch: &BranchId, status: u16, reason: &str) -> Response {
    let mut response = ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason.to_owned(),
    )
    .expect("a response")
    .build();
    response.headers.push_front(
        Header::build(
            HeaderName::Via,
            format!("SIP/2.0/UDP {EDGE};branch={branch}"),
        )
        .expect("a Via"),
    );
    response
}

// ------------------------------------------------------------------- PB-V ----------------------

#[test]
fn pb_v_1_an_invite_with_max_forwards_zero_is_483() {
    let (_, effects) = run(
        invite("sip:bob@b.example", vec![(HeaderName::MaxForwards, "0")]),
        vec![],
    );
    assert_eq!(statuses(&effects), [483]);
}

#[test]
fn pb_v_2_options_with_max_forwards_zero_is_also_483_never_answered_on_behalf() {
    // A proxy that answered OPTIONS on the target's behalf would leak topology and mislead whoever
    // is diagnosing the call. This platform never does.
    let (_, effects) = run(
        request(
            &Method::Options,
            "sip:bob@b.example",
            vec![(HeaderName::MaxForwards, "0")],
        ),
        vec![],
    );
    assert_eq!(statuses(&effects), [483]);
}

#[test]
fn pb_v_3_an_absent_max_forwards_is_inserted_as_70_then_decremented_to_69() {
    let (_, effects) = run(
        invite("sip:bob@b.example", vec![]),
        vec![target("sip:bob@10.0.0.1", 1_000)],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forwarded request");
    assert_eq!(
        header_text(forwarded, &HeaderName::MaxForwards).as_deref(),
        Some("69")
    );
}

#[test]
fn pb_v_4_an_unsupported_proxy_require_is_420_naming_the_offender() {
    let (_, effects) = run(
        invite(
            "sip:bob@b.example",
            vec![(HeaderName::ProxyRequire, "nothing-we-know")],
        ),
        vec![],
    );
    assert_eq!(statuses(&effects), [420]);
    let response = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond(response) => Some(response),
            _ => None,
        })
        .expect("a response");
    assert_eq!(
        response
            .headers
            .value(&HeaderName::Unsupported)
            .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
            .as_deref(),
        Some("nothing-we-know"),
        "a refusal the UAC cannot act on is not a refusal"
    );
}

#[test]
fn pb_v_5_an_unknown_request_uri_scheme_is_416() {
    let (_, effects) = run(invite("tel:+15550101", vec![]), vec![]);
    assert_eq!(statuses(&effects), [416]);
}

#[test]
fn pb_v_6_our_own_via_coming_back_is_a_loop_and_answers_482() {
    // The whole point of loop detection, end to end. Forward once to learn the branch we mint, then
    // deliver the request back to ourselves with that Via on top — which is exactly what a routing
    // cycle does.
    let original = invite("sip:bob@b.example", vec![]);
    let (_, effects) = run(original.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let mut looped = original;
    looped.headers.push_front(
        Header::build(
            HeaderName::Via,
            format!("SIP/2.0/UDP {EDGE};branch={branch}"),
        )
        .unwrap(),
    );

    let mut context = ResponseContext::new(config());
    let out = context.on_input(Input::Upstream(Box::new(looped)));
    assert_eq!(
        statuses(&out),
        [482],
        "a request carrying our own branch back to us is a loop"
    );
    assert!(
        !out.iter().any(|e| e.kind() == Kind::Forward),
        "a loop must not be forwarded round again"
    );
}

#[test]
fn pb_v_7_the_same_via_with_a_changed_request_uri_is_a_spiral_and_is_forwarded() {
    // The distinction RFC 5393 exists for: our Via is present, but the routing state moved on, so
    // this is a legitimate second visit and refusing it would break every service that re-targets.
    let original = invite("sip:bob@b.example", vec![]);
    let (_, effects) = run(original.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let mut spiralled = invite("sip:carol@b.example", vec![]);
    spiralled.headers.push_front(
        Header::build(
            HeaderName::Via,
            format!("SIP/2.0/UDP {EDGE};branch={branch}"),
        )
        .unwrap(),
    );

    let mut context = ResponseContext::new(config());
    let out = context.on_input(Input::Upstream(Box::new(spiralled)));
    assert!(
        out.iter().any(|e| e.kind() == Kind::ResolveTargets),
        "a spiral is forwarded: {:?}",
        out.iter().map(Effect::kind).collect::<Vec<_>>()
    );
    assert!(statuses(&out).is_empty(), "and it is not refused");
}

#[test]
fn a_foreign_proxys_via_is_never_mistaken_for_ours() {
    // Loop detection keys on *our* identities. Another proxy's Via, even with a branch shaped like
    // ours, is not evidence of a cycle through us.
    let original = invite("sip:bob@b.example", vec![]);
    let (_, effects) = run(original.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let mut relayed = original;
    relayed.headers.push_front(
        Header::build(
            HeaderName::Via,
            format!("SIP/2.0/UDP somebody-else.example;branch={branch}"),
        )
        .unwrap(),
    );

    let mut context = ResponseContext::new(config());
    let out = context.on_input(Input::Upstream(Box::new(relayed)));
    assert!(out.iter().any(|e| e.kind() == Kind::ResolveTargets));
}

// ------------------------------------------------------------------- PB-P ----------------------

#[test]
fn pb_p_2_our_own_route_is_popped_before_target_determination() {
    let mut context = ResponseContext::new(config());
    let request = invite(
        "sip:bob@b.example",
        vec![(HeaderName::Route, "<sip:edge-1.example;lr>")],
    );
    let effects = context.on_input(Input::Upstream(Box::new(request)));

    // With our Route popped, the URI to resolve is the Request-URI again, not our own edge.
    let query = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ResolveTargets(query) => Some(query),
            _ => None,
        })
        .expect("a resolve");
    assert_eq!(
        String::from_utf8_lossy(&query.uri),
        "sip:bob@b.example",
        "our own Route must not be the next hop"
    );
}

#[test]
fn pb_p_3_a_route_belonging_to_another_edge_is_popped_too() {
    // §5: any edge pops any edge's Route. A node that only recognized itself would drop the
    // mid-dialog requests the affinity token exists to let it handle.
    let mut config = config();
    config
        .identities
        .push(sipx_clstr_proxy::EdgeIdentity::host("edge-2.example"));
    let mut context = ResponseContext::new(config);
    let request = invite(
        "sip:bob@b.example",
        vec![(HeaderName::Route, "<sip:edge-2.example;lr>")],
    );
    let effects = context.on_input(Input::Upstream(Box::new(request)));
    let query = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ResolveTargets(query) => Some(query),
            _ => None,
        })
        .expect("a resolve");
    assert_eq!(String::from_utf8_lossy(&query.uri), "sip:bob@b.example");
}

#[test]
fn pb_p_4_a_tampered_token_is_403_with_no_forward_and_no_fallback() {
    let mut context = ResponseContext::new(config());
    let request = invite(
        "sip:bob@b.example",
        vec![(HeaderName::Route, "<sip:edge-1.example;lr;aft=tampered>")],
    );
    context.on_input(Input::Upstream(Box::new(request)));
    let effects = context.on_input(Input::TokenFact(TokenVerdict::Invalid));

    assert_eq!(statuses(&effects), [403]);
    assert!(
        !effects.iter().any(|effect| effect.kind() == Kind::Forward),
        "a forged token must not buy the routing it would have got without one"
    );
}

// ------------------------------------------------------------------- PB-F ----------------------

#[test]
fn pb_f_1_a_dialog_forming_invite_is_record_routed_with_a_branch_and_timer_c() {
    let (_, effects) = run(
        invite("sip:bob@b.example", vec![]),
        vec![target("sip:bob@10.0.0.1", 1_000)],
    );

    // Effect order is load-bearing: the Forward precedes the SetTimer that guards it.
    assert_eq!(
        effects.iter().map(Effect::kind).collect::<Vec<_>>(),
        [Kind::ResolveTargets, Kind::Forward, Kind::SetTimer]
    );

    // The row says "Timer C armed at F11's default, 240 s", and until `PX-10` this test asserted
    // only that *a* timer was set — so the row read 180 s, the code armed 180 s, and the value the
    // row exists to pin was never compared to anything. A "proved" row that proves the shape and not
    // the number is how the same defect survived being fixed once already (`DP-12`, `CC-V-9`).
    let armed = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SetTimer {
                timer: ProxyTimer::C,
                branch: Some(_),
                after,
            } => Some(*after),
            _ => None,
        })
        .expect("a Timer C, armed on the branch it guards");
    assert_eq!(armed, DEFAULT_TIMER_C, "F11's default");
    assert!(
        armed > TIMER_C_FLOOR,
        "§16.6 step 11 is a strict bound: {armed:?} must be larger than {TIMER_C_FLOOR:?}"
    );

    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    let record_route = header_text(forwarded, &HeaderName::RecordRoute).expect("a Record-Route");
    assert!(record_route.contains("edge-1.example"), "{record_route}");

    let via = header_text(forwarded, &HeaderName::Via).expect("a Via");
    assert!(via.contains("branch=z9hG4bK-"), "{via}");
    assert!(
        via.contains(EDGE),
        "the Via must be answerable back to us: {via}"
    );
}

#[test]
fn pb_f_1_a_re_invite_is_not_record_routed() {
    // RFC 6141: mid-dialog Record-Route does not alter an established route set, so adding one is
    // noise that also costs a token's worth of bytes on every re-negotiation.
    let (_, effects) = run(
        invite(
            "sip:bob@b.example",
            vec![(HeaderName::To, "<sip:bob@b.example>;tag=bt")],
        ),
        vec![target("sip:bob@10.0.0.1", 1_000)],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    assert!(header_text(forwarded, &HeaderName::RecordRoute).is_none());
}

#[test]
fn pb_f_2_three_equal_q_targets_fork_in_parallel_with_unique_branches() {
    let (_, effects) = run(
        invite("sip:bob@b.example", vec![]),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
            target("sip:bob@10.0.0.3", 1_000),
        ],
    );
    let ids = branches(&effects);
    assert_eq!(ids.len(), 3);
    let unique: std::collections::BTreeSet<&BranchId> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        3,
        "§16.6 step 8: unique per client transaction"
    );

    // Max-Breadth is divided across the parallel branches (RFC 5393), each getting at least 1.
    for effect in effects.iter().filter(|e| e.kind() == Kind::Forward) {
        let forwarded = effect.forwarded().expect("a request");
        let breadth = header_text(
            forwarded,
            &HeaderName::Other(Bytes::from_static(b"Max-Breadth")),
        )
        .expect("a Max-Breadth");
        assert_eq!(breadth, "20", "60 divided three ways");
    }
}

#[test]
fn pb_f_2_distinct_q_values_fork_in_sequence() {
    let (mut context, effects) = run(
        invite("sip:bob@b.example", vec![]),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 500),
        ],
    );
    // Only the leading q group goes out first.
    assert_eq!(branches(&effects).len(), 1, "the 500 target waits its turn");

    // When the first group concludes with a failure, the next group is tried.
    let request = invite("sip:bob@b.example", vec![]);
    let branch = branches(&effects).first().cloned().expect("a branch");
    let next = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &branch, 404, "Not Found")),
        branch,
    ));
    assert_eq!(branches(&next).len(), 1, "the second group is forked now");
}

#[test]
fn pb_f_3_an_unknown_header_survives_byte_identical_in_every_branch() {
    let (_, effects) = run(
        invite(
            "sip:bob@b.example",
            vec![(HeaderName::Other(Bytes::from_static(b"X-Vendor")), "a, b")],
        ),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    for effect in effects.iter().filter(|e| e.kind() == Kind::Forward) {
        let forwarded = effect.forwarded().expect("a request");
        assert_eq!(
            header_text(
                forwarded,
                &HeaderName::Other(Bytes::from_static(b"X-Vendor"))
            )
            .as_deref(),
            Some("a, b"),
            "a proxy must forward what it cannot itself interpret, unchanged"
        );
    }
}

#[test]
fn pb_f_4_a_strict_routing_next_hop_gets_the_f6_swap() {
    // The next hop advertises no `;lr`, so RFC 3261 §16.6 step 12 applies: the Request-URI goes to
    // the end of the Route set and the first Route becomes the Request-URI.
    let (_, effects) = run(
        invite(
            "sip:bob@b.example",
            vec![(HeaderName::Route, "<sip:strict.example>")],
        ),
        vec![target("sip:strict.example", 1_000)],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    assert_eq!(
        String::from_utf8_lossy(&forwarded.uri.to_bytes()),
        "sip:strict.example",
        "the strict router's URI is now the Request-URI"
    );
    let routes: Vec<String> = forwarded
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect();
    assert_eq!(
        routes.last().map(String::as_str),
        Some("<sip:strict.example>"),
        "the original Request-URI moved to the end of the Route set: {routes:?}"
    );
    // F7 over the swap: the first `Route` no longer carries `lr`, so the next hop is the
    // Request-URI. Following the `Route` instead would skip the very router the swap exists to
    // traverse.
    assert_eq!(
        effects
            .iter()
            .find_map(Effect::next_hop)
            .map(|uri| String::from_utf8_lossy(uri).into_owned()),
        Some("sip:strict.example".to_owned())
    );
}

#[test]
fn pb_f_6_a_surviving_route_is_the_next_hop_and_the_remote_target_stays_the_request_uri() {
    // The mid-dialog case with a second element still in the route set. F2 must not move, and F7
    // must not follow the Request-URI: the copy is *addressed* to the far end and *sent* to the next
    // hop, and a driver given only one of the two cannot do both.
    let (_, effects) = run(
        request(
            &Method::Bye,
            "sip:bob@10.0.0.1:5062",
            vec![
                (HeaderName::To, "<sip:bob@b.example>;tag=bt"),
                (HeaderName::Route, "<sip:edge-1.example;lr>"),
                (HeaderName::Route, "<sip:p2.example;lr>"),
            ],
        ),
        vec![],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    assert_eq!(
        String::from_utf8_lossy(&forwarded.uri.to_bytes()),
        "sip:bob@10.0.0.1:5062",
        "the dialog's remote target stays in the Request-URI"
    );
    assert_eq!(
        effects
            .iter()
            .find_map(Effect::next_hop)
            .map(|uri| String::from_utf8_lossy(uri).into_owned()),
        Some("sip:p2.example;lr".to_owned()),
        "our own Route was popped by P2; the next one is the hop"
    );
}

#[test]
fn pb_f_7_an_ack_for_a_2xx_is_routed_by_its_route_set_and_never_answered() {
    // K3. The `ACK` takes the same validation, preprocessing and F1–F9 edits as any other forwarded
    // request — through the same code — and it comes back as a request plus a hop, with no status
    // anywhere in the outcome type to answer it with.
    let ack = request(
        &Method::Ack,
        "sip:bob@10.0.0.1:5062",
        vec![
            (HeaderName::To, "<sip:bob@b.example>;tag=bt"),
            (HeaderName::Route, "<sip:edge-1.example;lr>"),
            (HeaderName::MaxForwards, "70"),
        ],
    );
    let outcome = route_ack(ack, &config());
    let AckRoute::Forward { request, next_hop } = outcome else {
        panic!("an ACK for a 2xx is a separately routed request: {outcome:?}");
    };
    assert_eq!(
        String::from_utf8_lossy(&next_hop),
        "sip:bob@10.0.0.1:5062",
        "the dialog's remote target, reached after our own Route was popped"
    );
    assert_eq!(
        header_text(&request, &HeaderName::Route),
        None,
        "P2 popped it"
    );
    assert_eq!(
        header_text(&request, &HeaderName::MaxForwards).as_deref(),
        Some("69"),
        "F3 applies to an ACK like any other forwarded request"
    );
    assert_eq!(
        header_text(&request, &HeaderName::RecordRoute),
        None,
        "RFC 6141: a mid-dialog Record-Route alters no established route set"
    );
    assert!(
        header_text(&request, &HeaderName::Via).is_some_and(|via| via.contains(EDGE)),
        "F8 pushes our Via even on a message nothing will answer"
    );
}

#[test]
fn pb_f_8_an_ack_that_cannot_be_forwarded_is_an_explicit_outcome_with_no_response() {
    // The half of K3 that the merge base got wrong twice over: it dropped the ACK, and it said
    // nothing. There is no status to send — `cluster-config` §8 V11 leans on the same fact — so the
    // outcome carries a reason instead, and the type has nowhere to put a response.
    let ack = request(
        &Method::Ack,
        "sip:bob@10.0.0.1:5062",
        vec![
            (HeaderName::To, "<sip:bob@b.example>;tag=bt"),
            (HeaderName::MaxForwards, "0"),
        ],
    );
    let outcome = route_ack(ack, &config());
    assert!(
        matches!(
            outcome,
            AckRoute::Unroutable(AckRefusal::Refused(Refusal::TooManyHops))
        ),
        "an unroutable ACK settles as an explicit outcome, never as a silent drop: {outcome:?}"
    );
}

#[test]
fn pb_f_5_an_empty_target_set_is_480() {
    let (_, effects) = run(invite("sip:bob@b.example", vec![]), vec![]);
    assert_eq!(statuses(&effects), [480]);
}

#[test]
fn f4_a_token_over_the_parameter_budget_is_refused_rather_than_truncated() {
    use sipx_clstr_proxy::{ForwardError, ForwardPlan, TOKEN_PARAM_BUDGET, forward, validate};

    let request = invite("sip:bob@b.example", vec![]);
    let config = config();
    let validated = validate(&request, &config).expect("valid");
    let oversized = Bytes::from("t".repeat(TOKEN_PARAM_BUDGET + 1));
    let target = target("sip:bob@10.0.0.1", 1_000);

    let plan = ForwardPlan {
        original: &request,
        validated: &validated,
        target: &target,
        index: 0,
        parallel_branches: 1,
        record_route: true,
        token: Some(oversized),
    };
    // Truncating a token would produce one that fails verification at the next hop — a call that
    // fails later and more confusingly than one refused here.
    assert!(matches!(
        forward(&plan, &config),
        Err(ForwardError::TokenTooLarge { .. })
    ));
}

// ------------------------------------------------------------------- PB-R ----------------------

#[test]
fn pb_r_1_a_100_from_a_branch_is_absorbed() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &branch, 100, "Trying")),
        branch,
    ));
    assert!(
        out.is_empty(),
        "a hop-by-hop 100 must not travel end to end"
    );
}

#[test]
fn pb_r_2_a_180_is_forwarded_and_resets_timer_c() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &branch, 180, "Ringing")),
        branch,
    ));
    assert_eq!(statuses(&out), [180]);
    assert!(
        out.iter().any(|e| e.kind() == Kind::SetTimer),
        "§16.8: every 101–199 resets Timer C"
    );

    // R2 — our Via is popped, so the response goes to whoever sent us the request.
    let response = out
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond(response) => Some(response),
            _ => None,
        })
        .expect("a response");
    let top_via = response
        .headers
        .value(&HeaderName::Via)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
        .expect("a Via");
    assert!(
        !top_via.contains(EDGE),
        "our own Via must be gone: {top_via}"
    );
}

#[test]
fn pb_r_3_and_r_4_a_2xx_is_forwarded_and_cancels_the_others_and_a_late_2xx_is_forwarded_too() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (first, second) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &first, 200, "OK")),
        first,
    ));
    assert_eq!(statuses(&out), [200]);
    assert!(
        out.iter().any(|e| e.kind() == Kind::CancelBranch),
        "R3: the pending branch is cancelled"
    );

    // R4/RFC 6026 — a second 2xx for the same INVITE is a fork, not a bug, and is forwarded too.
    let late = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &second, 200, "OK")),
        second,
    ));
    assert_eq!(
        statuses(&late),
        [200],
        "a late 2xx must reach the caller: RFC 6026"
    );
}

#[test]
fn pb_r_5_the_best_of_two_failures_prefers_the_branch_that_reached_the_user() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 486, "Busy Here")),
        a,
    ));
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 404, "Not Found")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [486],
        "R7 rank 2 over rank 3: the branch that reached the user outranks the one reporting absence"
    );
}

#[test]
fn pb_r_6_two_challenges_aggregate_into_one_response() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    let mut first = branch_response(&request, &a, 407, "Proxy Authentication Required");
    first.headers.push(
        Header::build(
            HeaderName::ProxyAuthenticate,
            "Digest realm=\"one\", nonce=\"n1\"",
        )
        .unwrap(),
    );
    let mut second = branch_response(&request, &b, 407, "Proxy Authentication Required");
    second.headers.push(
        Header::build(
            HeaderName::ProxyAuthenticate,
            "Digest realm=\"two\", nonce=\"n2\"",
        )
        .unwrap(),
    );

    context.on_input(Input::BranchResponse(Box::new(first), a));
    let out = context.on_input(Input::BranchResponse(Box::new(second), b));

    assert_eq!(statuses(&out), [407]);
    let response = out
        .iter()
        .find_map(|effect| match effect {
            Effect::Respond(response) => Some(response),
            _ => None,
        })
        .expect("a response");
    let challenges: Vec<String> = response
        .headers
        .get_all(&HeaderName::ProxyAuthenticate)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect();
    assert_eq!(
        challenges.len(),
        2,
        "a UAC that only saw one realm cannot satisfy the other: {challenges:?}"
    );
}

#[test]
fn pb_r_7_a_6xx_is_forwarded_at_once_and_cancels_everything() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let a = branches(&effects).first().cloned().expect("A");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 600, "Busy Everywhere")),
        a,
    ));
    assert_eq!(statuses(&out), [600]);
    assert!(out.iter().any(|e| e.kind() == Kind::CancelBranch));
}

#[test]
fn pb_r_8_a_sole_branch_503_reaches_the_caller_as_500() {
    // §16.7: the caller must not be told the destination is unavailable when what happened is that
    // we could not get there. Forwarding the 503 would also invite a retry-after the caller has no
    // basis for.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(
            &request,
            &branch,
            503,
            "Service Unavailable",
        )),
        branch,
    ));
    assert_eq!(statuses(&out), [500]);
}

#[test]
fn pb_r_9_a_transport_error_behaves_as_a_branch_503_and_therefore_becomes_500() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request, vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let out = context.on_input(Input::BranchTransportError(branch));
    assert_eq!(statuses(&out), [500], "§16.9 → R10 → R8");
}

#[test]
fn a_response_for_a_branch_we_never_forwarded_is_not_forwarded() {
    // R1. A stateful proxy has a context; a response outside it is not its business, and forwarding
    // it would put a message upstream that nothing upstream asked for.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, _) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let stranger = BranchId("z9hG4bK-not-ours".to_owned());
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &stranger, 200, "OK")),
        stranger,
    ));
    assert!(out.is_empty());
}

#[test]
fn the_context_terminates_once_it_has_answered_and_nothing_is_pending() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let branch = branches(&effects).first().cloned().expect("a branch");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &branch, 404, "Not Found")),
        branch,
    ));
    assert!(out.iter().any(|e| e.kind() == Kind::Terminate));
    assert!(context.is_finished());

    // And a finished context stays quiet rather than acting on late input.
    assert!(context.on_input(Input::UpstreamCancelled).is_empty());
}

// ------------------------------------------------------------------- PB-C ----------------------

/// Two equal-`q` branches, forked in parallel: the fixture every PB-C row starts from.
fn two_branches() -> (Request, ResponseContext, BranchId, BranchId) {
    let request = invite("sip:bob@b.example", vec![]);
    let (context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let a = ids.first().cloned().expect("branch A");
    let b = ids.get(1).cloned().expect("branch B");
    (request, context, a, b)
}

#[test]
fn pb_c_1_a_cancel_answers_200_cancels_the_answering_branch_and_queues_the_silent_one() {
    let (request, mut context, a, b) = two_branches();

    // A has answered once; B has said nothing at all.
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 180, "Ringing")),
        a.clone(),
    ));

    let out = context.on_input(Input::UpstreamCancelled);

    // C1 — the CANCEL's own 200 comes first, and unconditionally.
    assert_eq!(
        out.first().map(Effect::kind),
        Some(Kind::AnswerCancel),
        "§16.10: the CANCEL is acknowledged before anything else is decided"
    );

    // C2 — A is cancellable, B is not yet.
    let cancelled: Vec<&BranchId> = out
        .iter()
        .filter(|effect| effect.kind() == Kind::CancelBranch)
        .filter_map(Effect::branch)
        .collect();
    assert_eq!(cancelled, [&a], "only the branch that has answered");
    assert!(
        !cancelled.contains(&&b),
        "§9.1: a CANCEL must not overtake the INVITE it means to stop"
    );

    // …and when B finally answers, the queued CANCEL goes out.
    let later = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 180, "Ringing")),
        b.clone(),
    ));
    let released: Vec<&BranchId> = later
        .iter()
        .filter(|effect| effect.kind() == Kind::CancelBranch)
        .filter_map(Effect::branch)
        .collect();
    assert_eq!(released, [&b], "the queued CANCEL is released: {later:?}");
}

#[test]
fn pb_c_2_when_every_cancelled_branch_answers_487_the_caller_gets_487() {
    let (request, mut context, a, b) = two_branches();
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 180, "Ringing")),
        a.clone(),
    ));
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 180, "Ringing")),
        b.clone(),
    ));
    context.on_input(Input::UpstreamCancelled);

    let first = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 487, "Request Terminated")),
        a,
    ));
    assert!(
        statuses(&first).is_empty(),
        "selection waits for every branch"
    );

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 487, "Request Terminated")),
        b,
    ));
    assert_eq!(statuses(&out), [487]);
}

#[test]
fn pb_c_3_a_200_that_races_a_cancel_wins_and_the_cancel_is_still_answered() {
    let (request, mut context, a, b) = two_branches();
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 180, "Ringing")),
        a.clone(),
    ));

    let cancel = context.on_input(Input::UpstreamCancelled);
    assert_eq!(cancel.first().map(Effect::kind), Some(Kind::AnswerCancel));

    // A answers 200 anyway — the race §9's own text warns about. R5 is unconditional.
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 200, "OK")),
        a,
    ));
    assert_eq!(
        statuses(&out),
        [200],
        "a 2xx is forwarded always, cancellation notwithstanding"
    );
    let _ = b;
}

#[test]
fn pb_c_5_timer_c_with_a_provisional_seen_cancels_the_branch() {
    let (request, mut context, a, _b) = two_branches();
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 180, "Ringing")),
        a.clone(),
    ));

    // Provisionals were arriving and then stopped: the far end is alive but stuck, so stop it and
    // let C3 produce the 487 rather than fabricating a timeout it never reported.
    let out = context.on_input(Input::TimerFired(
        sipx_clstr_proxy::ProxyTimer::C,
        Some(a.clone()),
    ));
    assert_eq!(
        out.iter().map(Effect::kind).collect::<Vec<_>>(),
        [Kind::CancelBranch]
    );
    assert_eq!(out.first().and_then(Effect::branch), Some(&a));
}

#[test]
fn pb_c_6_timer_c_with_total_silence_concludes_the_branch_as_408() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request, vec![target("sip:bob@10.0.0.1", 1_000)]);
    let a = branches(&effects).first().cloned().expect("a branch");

    // Nothing was ever heard, so there is no transaction at the far end worth cancelling.
    let out = context.on_input(Input::TimerFired(sipx_clstr_proxy::ProxyTimer::C, Some(a)));
    assert_eq!(statuses(&out), [408], "R9: it behaves as a branch timeout");
}

#[test]
fn a_cancel_after_the_call_was_answered_is_a_no_op_beyond_its_own_200() {
    // §9.2. Re-answering upstream would contradict the 200 the caller already has.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(request.clone(), vec![target("sip:bob@10.0.0.1", 1_000)]);
    let a = branches(&effects).first().cloned().expect("a branch");
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 200, "OK")),
        a,
    ));

    let out = context.on_input(Input::UpstreamCancelled);
    // The context finished when the 200 concluded it, so the driver answers the CANCEL — which is
    // exactly the C4-adjacent case the driver owns.
    assert!(
        out.is_empty() || out.iter().all(|e| e.kind() == Kind::AnswerCancel),
        "nothing may go upstream a second time: {:?}",
        out.iter().map(Effect::kind).collect::<Vec<_>>()
    );
}

#[test]
fn a_timer_c_for_a_branch_that_already_concluded_is_ignored() {
    // A late timer is stale, not an event. Acting on it would cancel a branch that is gone or
    // conclude one twice.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let a = ids.first().cloned().expect("A");
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 404, "Not Found")),
        a.clone(),
    ));

    let out = context.on_input(Input::TimerFired(sipx_clstr_proxy::ProxyTimer::C, Some(a)));
    assert!(out.is_empty());
}

#[test]
fn a_timer_c_with_no_branch_is_ignored_rather_than_guessed_at() {
    // Timer C is per-branch by definition (§16.8); a firing without one is a driver bug, and
    // picking a branch to cancel would turn that bug into a dropped call.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, _) = run(request, vec![target("sip:bob@10.0.0.1", 1_000)]);
    let out = context.on_input(Input::TimerFired(sipx_clstr_proxy::ProxyTimer::C, None));
    assert!(out.is_empty());
}

// ------------------------------------------------------------------- PB-T ----------------------

/// Two branches at `q=1.0` and one queued group at `q=0.5`: the fixture every PB-T row starts from.
///
/// The composition is the point. `PB-F-2` proves sequential forking and `PB-R-3`, `PB-R-7` and
/// `PB-C-2` prove the terminal results, and none of them puts a *queue* behind a terminal result —
/// which is how a `487` settling a cancellation came to originate a new INVITE (`PX-14`).
fn two_branches_and_a_queued_group() -> (Request, ResponseContext, BranchId, BranchId) {
    let request = invite("sip:bob@b.example", vec![]);
    let (context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
            target("sip:bob@10.0.0.3", 500),
        ],
    );
    let ids = branches(&effects);
    assert_eq!(
        ids.len(),
        2,
        "only the leading `q` group forks: {effects:?}"
    );
    let a = ids.first().cloned().expect("branch A");
    let b = ids.get(1).cloned().expect("branch B");
    (request, context, a, b)
}

/// Every `Forward` in `effects` — the effect that would originate a new INVITE.
fn forwards(effects: &[Effect]) -> Vec<&Effect> {
    effects
        .iter()
        .filter(|effect| effect.kind() == Kind::Forward)
        .collect()
}

#[test]
fn pb_t_1_an_accepted_call_discards_the_queued_lower_q_group() {
    let (request, mut context, a, b) = two_branches_and_a_queued_group();

    // A answers: the call is up (R5) and B is cancelled (R11).
    let answered = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 200, "OK")),
        a,
    ));
    assert_eq!(statuses(&answered), [200]);

    // B's `487` is that cancellation settling, not a branch failure to route around. C was never
    // launched and must stay that way: a `Forward` here rings a phone after the caller is already
    // talking to A, and nothing will ever answer its response.
    let settled = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 487, "Request Terminated")),
        b,
    ));
    assert_eq!(
        forwards(&settled).len(),
        0,
        "an accepted call must not originate a new request: {settled:?}"
    );
}

#[test]
fn pb_t_2_a_global_rejection_discards_the_queued_lower_q_group() {
    let (request, mut context, a, b) = two_branches_and_a_queued_group();

    // R6 — a 6xx speaks for the address of record, not for one contact, so trying another contact
    // afterwards contradicts the answer already sent upstream.
    let rejected = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 600, "Busy Everywhere")),
        a,
    ));
    assert_eq!(statuses(&rejected), [600]);

    let settled = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 487, "Request Terminated")),
        b,
    ));
    assert_eq!(
        forwards(&settled).len(),
        0,
        "a globally rejected request must not be tried elsewhere: {settled:?}"
    );
}

#[test]
fn pb_t_3_an_upstream_cancel_discards_the_queued_lower_q_group() {
    let (request, mut context, a, b) = two_branches_and_a_queued_group();

    // Both branches have answered once, so C2 lets both CANCELs go immediately.
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 180, "Ringing")),
        a.clone(),
    ));
    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 180, "Ringing")),
        b.clone(),
    ));

    let cancelled = context.on_input(Input::UpstreamCancelled);
    assert_eq!(
        cancelled.first().map(Effect::kind),
        Some(Kind::AnswerCancel),
        "C1 first, whatever else follows"
    );
    assert_eq!(
        forwards(&cancelled).len(),
        0,
        "the CANCEL itself launches nothing: {cancelled:?}"
    );

    // The branches settle, and the last one to do so is where the queue used to come back.
    let first = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 487, "Request Terminated")),
        a,
    ));
    assert_eq!(forwards(&first).len(), 0, "{first:?}");

    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 487, "Request Terminated")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [487],
        "C3: the withdrawn request is answered, not re-tried elsewhere"
    );
    assert_eq!(
        forwards(&out).len(),
        0,
        "a caller who hung up must not make us ring a third phone: {out:?}"
    );
}

// ------------------------------------------------- §16.6 step 6: the target's route set ---------

#[test]
fn a_targets_route_set_is_applied_as_route_headers_in_stored_order() {
    // RFC 3327 §5.3: a registration's stored `Path` is the route set toward that contact, and
    // §16.6 step 6 says the proxy applies it. Missed by every other vector in this file because
    // they all use an empty route set — the register-then-call harness scenario is what found it.
    let mut target = target("sip:bob@10.0.0.1", 1_000);
    target.route_set = vec![
        Bytes::from_static(b"<sip:p2.example;lr>"),
        Bytes::from_static(b"<sip:p1.example;lr>"),
    ];

    let (_, effects) = run(invite("sip:bob@b.example", vec![]), vec![target]);
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forwarded request");

    let routes: Vec<String> = forwarded
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect();
    assert_eq!(
        routes,
        ["<sip:p2.example;lr>", "<sip:p1.example;lr>"],
        "topmost first, in stored order"
    );
}

#[test]
fn a_targets_route_set_goes_ahead_of_a_route_that_survived_preprocessing() {
    // The path is the nearer part of the journey, so it is traversed first.
    let mut target = target("sip:bob@10.0.0.1", 1_000);
    target.route_set = vec![Bytes::from_static(b"<sip:path.example;lr>")];

    let (_, effects) = run(
        invite(
            "sip:bob@b.example",
            vec![(HeaderName::Route, "<sip:downstream.example;lr>")],
        ),
        vec![target],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forwarded request");
    let routes: Vec<String> = forwarded
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
        .collect();
    assert_eq!(
        routes,
        ["<sip:path.example;lr>", "<sip:downstream.example;lr>"]
    );
}

#[test]
fn a_bare_route_uri_is_bracketed_so_its_parameters_stay_uri_parameters() {
    // `<sip:p;lr>` and `sip:p;lr` mean different things in a Route header: unbracketed, the `;lr`
    // is a *header* parameter and the loose-routing flag is lost.
    let mut target = target("sip:bob@10.0.0.1", 1_000);
    target.route_set = vec![Bytes::from_static(b"sip:p1.example;lr")];

    let (_, effects) = run(invite("sip:bob@b.example", vec![]), vec![target]);
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forwarded request");
    assert_eq!(
        header_text(forwarded, &HeaderName::Route).as_deref(),
        Some("<sip:p1.example;lr>")
    );
}

// --------------------------------------------------- the rows the coverage check found missing ---

#[test]
fn pb_p_1_a_strict_routing_predecessor_put_our_record_route_in_the_request_uri() {
    // §16.4 P1: the real Request-URI is the last `Route`. Recover it and drop that `Route`, or the
    // request is addressed to us forever and never reaches the callee.
    let mut context = ResponseContext::new(config());
    let mut request = invite("sip:edge-1.example;lr", vec![]);
    request
        .headers
        .push(Header::build(HeaderName::Route, "<sip:bob@b.example>").unwrap());

    let effects = context.on_input(Input::Upstream(Box::new(request)));
    let query = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ResolveTargets(query) => Some(query),
            _ => None,
        })
        .expect("a resolve");
    assert_eq!(
        String::from_utf8_lossy(&query.uri),
        "sip:bob@b.example",
        "the last Route becomes the Request-URI"
    );
}

#[test]
fn pb_p_6_an_in_dialog_request_asks_nobody_where_to_go() {
    // T1, and `V-03`'s whole subject. A remote contact is not an address of record, so asking the
    // location service about one answers the empty set for every ordinary call — a `BYE` concluded
    // `480` and an `ACK` (which has no response at all) simply lost.
    let (_, effects) = run(
        request(
            &Method::Bye,
            "sip:bob@10.0.0.1:5062",
            vec![(HeaderName::To, "<sip:bob@b.example>;tag=bt")],
        ),
        vec![target("sip:bob@10.9.9.9", 1_000)],
    );
    assert!(
        !effects.iter().any(|e| e.kind() == Kind::ResolveTargets),
        "the target set is predetermined: there is nothing to resolve"
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("the request is forwarded, not resolved");
    assert_eq!(
        String::from_utf8_lossy(&forwarded.uri.to_bytes()),
        "sip:bob@10.0.0.1:5062",
        "the Request-URI is the only target — the resolved contact above is never consulted"
    );
}

#[test]
fn pb_p_7_a_contact_at_our_own_host_is_not_a_record_route_value() {
    // P1's condition is that the Request-URI *is a value this platform placed in a Record-Route*, not
    // that it names an edge. An edge identity is port-agnostic by design (§5), so the weaker reading
    // fires on every mid-dialog request whose remote target shares a host with the edge — a loopback
    // deployment, or an edge beside a gateway — and consumes the route set while replacing the
    // remote target with our own address. The request is then addressed to us, forever.
    let (_, effects) = run(
        request(
            &Method::Bye,
            "sip:bob@edge-1.example:5062",
            vec![
                (HeaderName::To, "<sip:bob@b.example>;tag=bt"),
                (HeaderName::Route, "<sip:edge-1.example;lr>"),
            ],
        ),
        vec![],
    );
    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forward");
    assert_eq!(
        String::from_utf8_lossy(&forwarded.uri.to_bytes()),
        "sip:bob@edge-1.example:5062",
        "the contact has a user part and no `lr`: it is not a value we ever placed"
    );
    assert_eq!(
        header_text(forwarded, &HeaderName::Route),
        None,
        "P2 popped our own Route, and P1 did not consume it as a strict-routing recovery"
    );
}

#[test]
fn pb_p_5_an_expired_token_is_403_exactly_like_a_tampered_one() {
    // P3 makes no distinction: both are verification failures, and both are hard rejects. Treating
    // "expired" as softer would make the expiry the attacker's choice.
    let mut context = ResponseContext::new(config());
    let request = invite(
        "sip:bob@b.example",
        vec![(HeaderName::Route, "<sip:edge-1.example;lr;aft=expired>")],
    );
    context.on_input(Input::Upstream(Box::new(request)));
    let effects = context.on_input(Input::TokenFact(TokenVerdict::Invalid));
    assert_eq!(statuses(&effects), [403]);
    assert!(!effects.iter().any(|e| e.kind() == Kind::Forward));
}

#[test]
fn pb_r_4_a_late_2xx_after_a_final_was_chosen_is_still_forwarded() {
    // RFC 6026 in its own right, not as a corollary of PB-R-3: once one branch answered 200 and the
    // context concluded, a second 200 from another branch must *still* reach the caller — otherwise
    // a UAS sits in a call the caller does not know exists.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 200, "OK")),
        a,
    ));
    let late = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 200, "OK")),
        b,
    ));
    assert_eq!(statuses(&late), [200]);
}

#[test]
fn pb_r_10_a_branch_timeout_behaves_as_a_408_from_that_branch() {
    // The driver maps the kernel's `TuEvent::Timeout` to a synthesized `408` for the branch, per the
    // driver design's input table. From the engine's side it is an ordinary 4xx final.
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 408, "Request Timeout")),
        a,
    ));
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 500, "Server Internal Error")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [408],
        "the timeout is an ordinary 4xx final from A, so R7's class order picks it over B's 5xx"
    );
}

/// The four rows below pin §8.1, the within-class rule `PX-11` settled. RFC 3261 §16.7 step 6
/// fixes the class and leaves the response inside it to the proxy, so each of these is a choice
/// this specification makes rather than one the RFC forces — except `PB-R-14`, which is the one
/// the RFC does force.
#[test]
fn pb_r_11_a_resubmission_preference_outranks_a_lower_code() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 404, "Not Found")),
        a,
    ));
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 484, "Address Incomplete")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [484],
        "§16.7 step 6 names 484 as resubmission-affecting; the caller can complete the address"
    );
}

#[test]
fn pb_r_12_an_answer_outranks_a_branch_that_never_answered() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 486, "Busy Here")),
        a,
    ));
    // R9 — B's timeout reaches the engine as a synthesized `408` final for that branch.
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 408, "Request Timeout")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [486],
        "silence must not outrank an answer: A reached the user, B did not"
    );
}

#[test]
fn pb_r_13_two_server_failures_fall_to_the_lowest_code() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 500, "Server Internal Error")),
        a,
    ));
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 502, "Bad Gateway")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [500],
        "§8.1 ranks no 5xx code, so the tie-break decides and nothing more is claimed"
    );
}

#[test]
fn pb_r_14_a_6xx_outranks_a_4xx_already_stored_in_the_context() {
    let request = invite("sip:bob@b.example", vec![]);
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let ids = branches(&effects);
    let (a, b) = (
        ids.first().cloned().expect("A"),
        ids.get(1).cloned().expect("B"),
    );

    let early = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &a, 404, "Not Found")),
        a,
    ));
    assert!(
        statuses(&early).is_empty(),
        "B is still pending, so nothing is chosen yet"
    );
    let out = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &b, 600, "Busy Everywhere")),
        b,
    ));
    assert_eq!(
        statuses(&out),
        [600],
        "the one part of the choice §16.7 step 6 makes a MUST: 6xx if any exist in the context"
    );
}

#[test]
fn pb_v_8_a_max_breadth_of_one_serializes_the_second_target_rather_than_dropping_it() {
    // RFC 5393 §5.2. `Max-Breadth` bounds *parallel* fan-out; the surplus target is queued and tried
    // when the first concludes. Truncating instead would silently lose a device the user registered,
    // which is the failure nobody notices until a call does not ring on one phone.
    let request = invite(
        "sip:bob@b.example",
        vec![(HeaderName::Other(Bytes::from_static(b"Max-Breadth")), "1")],
    );
    let (mut context, effects) = run(
        request.clone(),
        vec![
            target("sip:bob@10.0.0.1", 1_000),
            target("sip:bob@10.0.0.2", 1_000),
        ],
    );
    let first = branches(&effects);
    assert_eq!(first.len(), 1, "only one branch may go out in parallel");

    let forwarded = effects
        .iter()
        .find_map(Effect::forwarded)
        .expect("a forwarded request");
    assert_eq!(
        header_text(
            forwarded,
            &HeaderName::Other(Bytes::from_static(b"Max-Breadth"))
        )
        .as_deref(),
        Some("1")
    );

    // The second target is not lost: it is forked once the first concludes.
    let branch = first.first().cloned().expect("a branch");
    let next = context.on_input(Input::BranchResponse(
        Box::new(branch_response(&request, &branch, 404, "Not Found")),
        branch,
    ));
    assert_eq!(
        branches(&next).len(),
        1,
        "the surplus target is serialized behind, not dropped: {:?}",
        next.iter().map(Effect::kind).collect::<Vec<_>>()
    );
}

#[test]
fn pb_v_9_an_unanswerable_request_is_dropped_with_nothing_sent() {
    // V1. No Via, no Call-ID, no CSeq: there is nothing to answer *to*, and a response built from
    // this would carry a fabricated Call-ID on the wire. Dropped, never guessed at.
    //
    // (This test existed, was deleted by accident while rewriting the loop-detection rows, and was
    // restored by `scripts/check-vectors.py` noticing the row had lost its proof.)
    let bare = RequestBuilder::new(Method::Options, uri("sip:bob@b.example")).build();
    let mut context = ResponseContext::new(config());
    let effects = context.on_input(Input::Upstream(Box::new(bare)));

    assert!(statuses(&effects).is_empty(), "nothing may be sent");
    assert_eq!(
        effects.iter().map(Effect::kind).collect::<Vec<_>>(),
        [Kind::Terminate]
    );
}
