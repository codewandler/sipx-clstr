//! Building the forwarded request — §7 (RFC 3261 §16.6), steps F1–F11.
//!
//! Everything untouched re-serializes byte-exact; that is the kernel's lossless guarantee and the
//! passthrough vectors assert it. This module only makes the edits §16.6 lists, in the order it
//! lists them.

use bytes::Bytes;
use sipx_sip::headers::Address;
use sipx_sip::{Header, HeaderName, Request, Uri};

use crate::config::ProxyConfig;
use crate::cookie::branch_for;
use crate::types::{BranchId, Target};
use crate::validate::Validated;

/// The affinity-token URI parameter (affinity-token §6).
///
/// Exactly one per platform URI: a `Route` that resolves here carrying zero or several fails
/// verification, because "which token did you mean" has no safe answer.
pub const TOKEN_PARAM: &str = "aft";

/// The normative size budget for the token **parameter** — not the header line.
///
/// The distinction is the affinity-token spec's own correction to `PB-F-1`'s shorthand: at the
/// module-facts ceiling the whole `Record-Route` line exceeds 200 B for any realistic host, while
/// the parameter stays inside it. Asserting on the line would fail a compliant token.
pub const TOKEN_PARAM_BUDGET: usize = 200;

/// What the engine needs in order to forward one copy.
#[derive(Debug, Clone)]
pub struct ForwardPlan<'a> {
    /// The request as it arrived.
    pub original: &'a Request,
    /// What validation established.
    pub validated: &'a Validated,
    /// Where this copy goes.
    pub target: &'a Target,
    /// Which fork this is, for the branch's unique part.
    pub index: usize,
    /// How many branches share this request's `Max-Breadth`.
    pub parallel_branches: u32,
    /// Whether to Record-Route (dialog-forming requests the platform must stay in the path for).
    pub record_route: bool,
    /// The token to carry, opaque here.
    ///
    /// `AF-4` mints the real one. Until then the driver supplies a placeholder, which is what lets
    /// F4 and its byte budget be implemented and tested before the crypto exists.
    pub token: Option<Bytes>,
}

/// Why a copy could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForwardError {
    /// The target is not a URI the kernel will parse, so there is nothing to forward to.
    #[error("the target URI does not parse")]
    BadTarget,
    /// F4's budget: the token parameter is over [`TOKEN_PARAM_BUDGET`].
    #[error("the token parameter is {size} bytes, over the {TOKEN_PARAM_BUDGET} byte budget")]
    TokenTooLarge {
        /// How large it was.
        size: usize,
    },
    /// The `Record-Route` this proxy is configured with does not parse.
    #[error("the configured Record-Route URI does not parse")]
    BadRecordRoute,
}

/// Build one forwarded copy, and the branch it goes out on.
pub fn forward(
    plan: &ForwardPlan<'_>,
    config: &ProxyConfig,
) -> Result<(BranchId, Request), ForwardError> {
    // F1 — copy. Unknown headers and the body come along untouched.
    let mut request = plan.original.clone();

    // F2 — the Request-URI becomes the target, verbatim. A contact from the location service keeps
    // its parameters: re-serializing a parsed URI here would change what the UA registered.
    request.uri = Uri::parse(plan.target.uri.clone()).map_err(|_| ForwardError::BadTarget)?;

    // F3 — decrement, after V3's insert rule. An absent header became the configured default
    // during validation, so this is always the decrement of a value we know.
    let hops = plan.validated.max_forwards.saturating_sub(1);
    set_header(&mut request, &HeaderName::MaxForwards, &hops.to_string());

    // Max-Breadth (§6): the incoming value is divided across the parallel branches, each getting
    // at least 1 — a branch that cannot get 1 is not forwarded at all (V5), which is the caller's
    // check, not this function's.
    let breadth = plan
        .validated
        .max_breadth
        .checked_div(plan.parallel_branches.max(1))
        .unwrap_or(1)
        .max(1);
    set_header(
        &mut request,
        &HeaderName::Other(Bytes::from_static(b"Max-Breadth")),
        &breadth.to_string(),
    );

    // F4 — Record-Route, carrying the token as a URI parameter.
    if plan.record_route {
        let value = record_route_value(config, plan.token.as_ref())?;
        request.headers.push_front(
            Header::build(HeaderName::RecordRoute, value)
                .map_err(|_| ForwardError::BadRecordRoute)?,
        );
    }

    // §16.6 step 6 — apply the target's route set. For a contact from the location service that is
    // the stored `Path` (RFC 3327 §5.3): the route the registration says must be traversed to reach
    // this UA. Inserted in stored order at the front, ahead of any `Route` that survived
    // preprocessing, because the path is the *nearer* part of the journey.
    //
    // Pushed in reverse because `push_front` prepends one at a time.
    for route in plan.target.route_set.iter().rev() {
        let value = if route.starts_with(b"<") {
            String::from_utf8_lossy(route).into_owned()
        } else {
            // A bare URI needs brackets, or its parameters would read as header parameters.
            format!("<{}>", String::from_utf8_lossy(route))
        };
        if let Ok(header) = Header::build(HeaderName::Route, value) {
            request.headers.push_front(header);
        }
    }

    // F6 — route postprocessing for a strict-routing next hop: the Request-URI moves to the end of
    // the Route set and the first Route becomes the Request-URI. RFC 3261 §16.6 step 12. Runs after
    // the route set is applied, since the first hop is now the path's topmost value.
    apply_strict_route_swap(&mut request);

    // F8 — push our Via, with §6's branch. Last, so that the branch covers the request as it will
    // actually leave: a Via computed before F2 would attest to a Request-URI we then changed.
    let branch = branch_for(config, &plan.validated.cookie_input, plan.index);
    let via = format!(
        "SIP/2.0/UDP {};branch={branch}",
        String::from_utf8_lossy(&config.record_route_host())
    );
    request
        .headers
        .push_front(Header::build(HeaderName::Via, via).map_err(|_| ForwardError::BadRecordRoute)?);

    // F9 — Content-Length is the kernel's framing rule, applied at serialization.
    // F10/F11 — forwarding and Timer C are effects, which the engine emits.

    Ok((BranchId(branch), request))
}

/// The `Record-Route` value: the configured URI with the token parameter added.
fn record_route_value(config: &ProxyConfig, token: Option<&Bytes>) -> Result<String, ForwardError> {
    let base = String::from_utf8_lossy(&config.record_route_uri).into_owned();
    let Some(token) = token else {
        return Ok(base);
    };

    let parameter = format!(";{TOKEN_PARAM}={}", String::from_utf8_lossy(token));
    if parameter.len() > TOKEN_PARAM_BUDGET {
        return Err(ForwardError::TokenTooLarge {
            size: parameter.len(),
        });
    }

    // The parameter goes inside the angle brackets, on the URI — a header parameter would not
    // survive into the `Route` an endpoint derives from this (§12.1.1 copies the URI).
    match base.rfind('>') {
        Some(close) => {
            let mut out = base.clone();
            out.insert_str(close, &parameter);
            Ok(out)
        }
        // A URI with no angle brackets cannot carry parameters unambiguously — `;` would read as a
        // header parameter — so bracket it first.
        None => Ok(format!("<{base}{parameter}>")),
    }
}

/// §16.6 step 12: if the next hop is a strict router, swap the Request-URI and the last Route.
fn apply_strict_route_swap(request: &mut Request) {
    let routes: Vec<Bytes> = request
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
        .collect();

    let Some(first) = routes.first() else {
        return;
    };
    let Ok(address) = Address::parse(first, "Route") else {
        return;
    };
    // A loose router advertises `;lr`. Its absence is what identifies a strict router, and RFC 3261
    // §16.6 step 12 is explicit that the swap is only for that case.
    if address
        .uri
        .params()
        .is_some_and(|params| params.get("lr").is_some())
    {
        return;
    }

    // The Request-URI goes to the end of the Route set; the first Route becomes the Request-URI.
    let request_uri = request.uri.to_bytes();
    let new_uri = address.uri.clone();

    request.headers.remove_all(&HeaderName::Route);
    for route in routes.iter().skip(1) {
        if let Ok(header) = Header::build(HeaderName::Route, route.clone()) {
            request.headers.push(header);
        }
    }
    if let Ok(header) = Header::build(
        HeaderName::Route,
        format!("<{}>", String::from_utf8_lossy(&request_uri)),
    ) {
        request.headers.push(header);
    }
    request.uri = new_uri;
}

/// Replace a single-valued header, or add it if absent.
fn set_header(request: &mut Request, name: &HeaderName, value: &str) {
    request.headers.remove_all(name);
    if let Ok(header) = Header::build(name.clone(), value.to_owned()) {
        request.headers.push_front(header);
    }
}

impl ProxyConfig {
    /// The host part of the configured `Record-Route`, for the `Via` sent-by.
    ///
    /// The two must agree: a response finds its way back by the `Via`, and a mid-dialog request
    /// finds its way back by the `Record-Route`. A node whose two identities differed would be
    /// reachable for one and not the other.
    pub(crate) fn record_route_host(&self) -> Bytes {
        let text = String::from_utf8_lossy(&self.record_route_uri);
        let inner = text
            .trim_start_matches('<')
            .trim_end_matches('>')
            .split(';')
            .next()
            .unwrap_or(&text);
        let host = inner.split_once(':').map_or(inner, |(_, rest)| rest);
        Bytes::from(host.to_owned())
    }
}
