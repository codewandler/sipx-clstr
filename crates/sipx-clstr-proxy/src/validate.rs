//! Request validation — [proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! §4, RFC 3261 §16.3 as amended by RFC 5393.
//!
//! Checks run **in order**, and the first failure responds and terminates. A response is only
//! possible when the request is well-formed enough to answer; otherwise it is dropped and never
//! guessed at, because a response built from a message we could not parse would carry fields we
//! invented.

use bytes::Bytes;
use sipx_sip::{HeaderName, Request};

use crate::config::ProxyConfig;
use crate::cookie::{CookieInput, cookie_of};

/// Why a request will not be forwarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// V1 — unanswerable. Dropped, with nothing sent.
    Drop,
    /// V2 — `416 Unsupported URI Scheme`.
    UnsupportedScheme,
    /// V3 — `483 Too Many Hops`.
    TooManyHops,
    /// V4 — `482 Loop Detected`.
    LoopDetected,
    /// V5 — `440 Max-Breadth Exceeded`.
    MaxBreadthExceeded,
    /// V6 — `420 Bad Extension`, naming the tags the profile does not implement.
    BadExtension(Vec<String>),
}

impl Refusal {
    /// The status to answer with, or `None` when the request must be dropped.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Drop => None,
            Self::UnsupportedScheme => Some(416),
            Self::TooManyHops => Some(483),
            Self::LoopDetected => Some(482),
            Self::MaxBreadthExceeded => Some(440),
            Self::BadExtension(_) => Some(420),
        }
    }

    /// The reason phrase for the status.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Drop => "",
            Self::UnsupportedScheme => "Unsupported URI Scheme",
            Self::TooManyHops => "Too Many Hops",
            Self::LoopDetected => "Loop Detected",
            Self::MaxBreadthExceeded => "Max-Breadth Exceeded",
            Self::BadExtension(_) => "Bad Extension",
        }
    }
}

/// What validation learned, for the steps that follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    /// `Max-Forwards` **as it arrived**, with the platform default substituted when absent.
    ///
    /// §16.6 lets a proxy insert one, and this platform does, so V3 is meaningful downstream for
    /// every request rather than only for the ones that happened to carry the header.
    pub max_forwards: u8,
    /// `Max-Breadth` as it arrived, or RFC 5393's recommended default.
    pub max_breadth: u32,
    /// The loop-detection state, computed once and reused for the branches (§6).
    pub cookie_input: CookieInput,
}

/// Run §4's checks in order.
pub fn validate(request: &Request, config: &ProxyConfig) -> Result<Validated, Refusal> {
    // V1 — respondability. The kernel's lossless model means an unknown header or method is *not*
    // a failure: a proxy must be able to forward what it cannot itself interpret. What makes a
    // request unanswerable is missing the fields a response is built from.
    if !is_respondable(request) {
        return Err(Refusal::Drop);
    }

    // V2 — the Request-URI's scheme.
    if !config.understands_scheme(&request.uri) {
        return Err(Refusal::UnsupportedScheme);
    }

    // V3 — Max-Forwards. Absent means insert the default, which is > 0 by construction, so only a
    // present-and-zero value refuses. Fires for *every* method including OPTIONS: this platform
    // never answers a proxied request on the target's behalf, because a proxy that impersonates
    // targets leaks topology and surprises whoever is diagnosing the call.
    let max_forwards = match header_number::<u8>(request, &HeaderName::MaxForwards) {
        Some(0) => return Err(Refusal::TooManyHops),
        Some(value) => value,
        None => config.default_max_forwards,
    };

    // V4 — loop detection per RFC 5393. A `482` only when one of *our* Via entries is present
    // **and** its cookie matches the cookie recomputed over the current routing state. Same Via
    // with a different cookie is a spiral, which is legitimate and must be forwarded.
    let cookie_input = CookieInput::from_request(request);
    let expected = cookie_input.cookie(&config.cookie_key);
    if is_loop(request, config, &expected) {
        return Err(Refusal::LoopDetected);
    }

    // V5 — Max-Breadth. A request that cannot give at least 1 to a branch is not forwarded.
    let max_breadth = match header_number::<u32>(
        request,
        &HeaderName::Other(Bytes::from_static(b"Max-Breadth")),
    ) {
        Some(0) => return Err(Refusal::MaxBreadthExceeded),
        Some(value) => value,
        None => config.default_max_breadth,
    };

    // V6 — Proxy-Require.
    let unsupported: Vec<String> = option_tags(request, &HeaderName::ProxyRequire)
        .into_iter()
        .filter(|tag| !config.supports(tag))
        .collect();
    if !unsupported.is_empty() {
        return Err(Refusal::BadExtension(unsupported));
    }

    // V7 — authentication is the driver's, via the hook phases. It reaches this crate as a fact,
    // not as a check, because the credential store and the tenant policy are not the proxy's.

    Ok(Validated {
        max_forwards,
        max_breadth,
        cookie_input,
    })
}

/// Whether a response can be built for this request at all.
///
/// The kernel's `ResponseBuilder::to_request` needs the fields a response echoes. Without them
/// there is nothing to answer *to*, and inventing them would put a fabricated `Call-ID` on the
/// wire.
fn is_respondable(request: &Request) -> bool {
    request.headers.get(&HeaderName::Via).is_some()
        && request.headers.get(&HeaderName::CallId).is_some()
        && request.headers.get(&HeaderName::CSeq).is_some()
        && request.headers.get(&HeaderName::From).is_some()
        && request.headers.get(&HeaderName::To).is_some()
}

/// V4: is one of our own `Via` entries here with a cookie that matches the current state?
fn is_loop(request: &Request, config: &ProxyConfig, expected: &str) -> bool {
    request
        .headers
        .get_all(&HeaderName::Via)
        .filter_map(|header| sipx_sip::headers::Via::parse_one(&header.value()).ok())
        .filter(|via| is_ours(via, config))
        .any(|via| {
            via.branch()
                .and_then(|branch| cookie_of(&String::from_utf8_lossy(branch)).map(str::to_owned))
                .is_some_and(|cookie| cookie == expected)
        })
}

/// Whether a `Via` was inserted by this platform — by sent-by, against every configured identity.
fn is_ours(via: &sipx_sip::headers::Via, config: &ProxyConfig) -> bool {
    let sent_by = String::from_utf8_lossy(&via.host.to_bytes()).to_ascii_lowercase();
    let sent_by = sent_by.strip_suffix('.').unwrap_or(&sent_by).to_owned();
    config.identities.iter().any(|identity| {
        identity.host == sent_by
            && match identity.port {
                // A port-agnostic identity matches whatever port the Via states.
                None => true,
                Some(port) => via.port.is_none_or(|actual| actual == port),
            }
    })
}

fn header_number<T: std::str::FromStr>(request: &Request, name: &HeaderName) -> Option<T> {
    let value = request.headers.value(name)?;
    String::from_utf8_lossy(&value).trim().parse().ok()
}

fn option_tags(request: &Request, name: &HeaderName) -> Vec<String> {
    request
        .headers
        .get_all(name)
        .flat_map(|header| {
            String::from_utf8_lossy(&header.value())
                .split(',')
                .map(|tag| tag.trim().to_owned())
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}
