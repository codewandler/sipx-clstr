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
use crate::route;
use crate::types::{BranchId, RecordRouteTokens, Target};
use crate::validate::Validated;

/// The affinity-token URI parameter, and the size budget for it — affinity-token §5.
///
/// Re-exported rather than restated. Both used to be literals here, with nothing relating them to
/// the layout that has to fit inside them: `AT-6` asserted `≤ 200` against a third literal of its
/// own, so widening the token — a larger module-fact sub-budget, a longer tag — would have spent
/// §5's measured headroom without anything going red. They now live beside the encoding that
/// produces the value, where a `const` assertion compares them to `MAX_TOKEN_LEN` at build time.
pub use sipx_clstr_affinity::{TOKEN_PARAM, TOKEN_PARAM_BUDGET};

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
    /// The `Record-Route` pair to carry, opaque here — affinity-token §7 M1/M2.
    ///
    /// `None` is a platform with no key set: it stamps the single, tokenless `Record-Route` it
    /// always has. The pair exists to carry the two *directions*, so without tokens there is
    /// nothing for a second entry to say and it would be bytes off the MTU budget for nothing.
    pub tokens: Option<&'a RecordRouteTokens>,
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

/// Build one forwarded copy, the branch it goes out on, and F7's next hop.
///
/// The next hop is computed here rather than by the caller because F7 is not a function of the
/// finished copy alone: RFC 3261 §16.6 step 7 conditions it on whether step 6 reformatted the copy
/// for a strict router, and only this function knows F6's answer. A caller reading the copy after
/// the fact has to infer the swap from the first `Route`'s shape, and that inference has a
/// counterexample (`PX-16`): `[strict, p2;lr]` swaps to a first `Route` that carries `lr`.
pub fn forward(
    plan: &ForwardPlan<'_>,
    config: &ProxyConfig,
) -> Result<(BranchId, Request, Bytes), ForwardError> {
    // F1 — copy. Unknown headers and the body come along untouched.
    let mut request = plan.original.clone();

    // F2 — the Request-URI becomes the target, verbatim. A contact from the location service keeps
    // its parameters: re-serializing a parsed URI here would change what the UA registered.
    // `set_uri`, never field assignment: a parsed request keeps its raw start-line bytes for exact
    // forwarding, and a copy retargeted around them puts the *original* request line on the wire.
    request.set_uri(Uri::parse(plan.target.uri.clone()).map_err(|_| ForwardError::BadTarget)?);

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

    // F4 — Record-Route, carrying the affinity token as a URI parameter.
    if plan.record_route {
        // affinity-token §7 M2 fixes the order, and it is load-bearing rather than cosmetic: the
        // ORIG entry goes on first and the TERM entry on top of it, so that route-set learning —
        // RFC 3261 §12.1.1 *in order* for the UAS, §12.1.2 *reversed* for the UAC — hands each
        // endpoint its own side's entry as the first of the pair. Get it backwards and both ends
        // present the other side's direction on every mid-dialog request.
        //
        // `push_front` prepends one at a time, so pushing ORIG first leaves TERM topmost.
        for value in record_route_values(config, plan.tokens)? {
            request.headers.push_front(
                Header::build(HeaderName::RecordRoute, value)
                    .map_err(|_| ForwardError::BadRecordRoute)?,
            );
        }
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
    // the Route set and the first Route becomes the Request-URI. RFC 3261 §16.6 step 6. Runs after
    // the route set is applied, since the first hop is now the path's topmost value. Whether it
    // fired is F7's input, so the answer is kept rather than re-derived from the copy.
    let reformatted = apply_strict_route_swap(&mut request);

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

    // F7 — the next hop, read off the copy after F6 and off F6's answer. Computed once the copy is
    // final, so it is the hop this exact message must reach rather than the URI it is addressed to.
    let next_hop = route::next_hop_of(&request, reformatted);

    // F9 — Content-Length is the kernel's framing rule, applied at serialization.
    // F10/F11 — forwarding and Timer C are effects, which the engine emits.

    Ok((BranchId(branch), request, next_hop))
}

/// The `Record-Route` values to push, **in push order** — ORIG first, so TERM ends up on top (M2).
///
/// One value without a token, two with: the pair is a property of the token, not of Record-Routing.
fn record_route_values(
    config: &ProxyConfig,
    tokens: Option<&RecordRouteTokens>,
) -> Result<Vec<String>, ForwardError> {
    let Some(tokens) = tokens else {
        return Ok(vec![
            String::from_utf8_lossy(&config.record_route_uri).into_owned(),
        ]);
    };
    Ok(vec![
        record_route_value(config, Some(&tokens.originating))?,
        record_route_value(config, Some(&tokens.terminating))?,
    ])
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

/// §16.6 step 6: if the next hop is a strict router, swap the Request-URI and the last Route.
///
/// Returns whether it did — F7's condition (RFC 3261 §16.6 step 7), which cannot be re-read off
/// the finished copy: after a swap over `[strict, p2;lr]` the first `Route` carries `lr` again.
fn apply_strict_route_swap(request: &mut Request) -> bool {
    let routes: Vec<Bytes> = request
        .headers
        .get_all(&HeaderName::Route)
        .map(|header| Bytes::copy_from_slice(header.value().as_ref()))
        .collect();

    let Some(first) = routes.first() else {
        return false;
    };
    let Ok(address) = Address::parse(first, "Route") else {
        return false;
    };
    // A loose router advertises `;lr`. Its absence is what identifies a strict router, and RFC 3261
    // §16.6 step 6 is explicit that the swap is only for that case.
    if address
        .uri
        .params()
        .is_some_and(|params| params.get("lr").is_some())
    {
        return false;
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
    // `set_uri` for the same reason as F2's: the raw start line must not outlive the retarget.
    request.set_uri(new_uri);
    true
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
