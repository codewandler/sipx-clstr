//! The `Via` branch and its loop-detection cookie (§6, RFC 5393).
//!
//! Every forwarded request gets one new topmost `Via` whose branch is
//! `z9hG4bK` · *transaction-unique part* · *cookie*.
//!
//! The **cookie** is a keyed MAC over the fields that determine how a request is *routed*:
//! Request-URI, To tag, From tag, `Call-ID`, `CSeq` number, and the sequence of `Route` values.
//! Two
//! arrivals whose cookies match are the *same* request revisiting this proxy — a loop (V4). Two
//! arrivals whose cookies differ took different routing decisions — a spiral, which is legitimate
//! and must be forwarded.
//!
//! # Deviation from proxy-behavior §6, deliberate: the topmost `Via` is **not** in the cookie
//!
//! The spec's §6 lists "topmost incoming `Via`" among the cookie's fields. Including it makes loop
//! detection **structurally unable to fire**, and the argument is a proof rather than a preference:
//!
//! 1. Pass one: the request arrives with the caller's `Via` on top. The cookie is `C₁`. We forward
//!    with our own `Via` — whose branch carries `C₁` — pushed in front of it.
//! 2. The request loops and comes back. Now the topmost `Via` is *ours from pass one*.
//! 3. We recompute over the current state. The top `Via` has changed, so the cookie is `C₂ ≠ C₁`.
//! 4. V4 looks for one of our `Via` entries carrying a cookie equal to `C₂`. Ours carries `C₁`. No
//!    match, so the request is judged a spiral and forwarded — and round it goes until
//!    `Max-Forwards` expires at every node on the cycle.
//!
//! RFC 3261 §16.3 step 4 requires the branch to depend on "all information affecting processing of
//! a request"; the topmost `Via` affects where the *response* goes, not where the request is routed.
//! RFC 3261 §16.6 step 8 does recommend the topmost `Via` — as **entropy**, for the part of the
//! branch that must be unique per client transaction. So it belongs in the unique part, which is
//! where this module puts it, and the two purposes stop fighting.
//!
//! `PX-1`'s §6 needs the correction; recorded as `PX-5`'s open question.
//!
//! The **unique part** distinguishes the branches of one fork from each other, because RFC 3261
//! §16.6 step 8 requires the branch to be unique per client transaction.
//!
//! Both are derived rather than random: a deterministic branch is reproducible in the harness, and
//! nothing about RFC 3261 requires the value to be unpredictable — only unique, and prefixed with
//! the magic cookie.

use bytes::Bytes;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use sipx_sip::headers::Address;
use sipx_sip::{HeaderName, Request};

use crate::config::{CookieKey, ProxyConfig};

/// RFC 3261 §8.1.1.7 — the magic cookie every compliant branch starts with.
pub const MAGIC_COOKIE: &str = "z9hG4bK";

/// How many MAC bytes go into a cookie.
///
/// Eight bytes, hex-encoded, so a branch stays short enough not to push a UDP datagram toward
/// fragmentation. Loop detection needs a value an attacker cannot *forge*, not one they cannot
/// guess: 64 bits of MAC is far past the point where a forgery attempt is cheaper than simply
/// sending traffic.
const COOKIE_BYTES: usize = 8;

/// The routing-relevant state RFC 5393 hashes over.
///
/// A struct rather than a pile of arguments so that adding a field is a compile error at every
/// call site — and so the field *order* is fixed in one place, since the MAC is order-sensitive
/// and two nodes disagreeing about it would see loops everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieInput {
    /// The Request-URI, verbatim.
    pub request_uri: Bytes,
    /// The `To` tag, if the request carries one.
    pub to_tag: Option<Bytes>,
    /// The `From` tag.
    pub from_tag: Option<Bytes>,
    /// The `Call-ID`, byte-exact.
    pub call_id: Bytes,
    /// The `CSeq` *number* only — the method is not routing-relevant.
    pub cseq: u32,
    /// The topmost incoming `Via`, verbatim.
    ///
    /// Feeds the branch's **unique part** only, never the cookie — see this module's header.
    pub top_via: Option<Bytes>,
    /// Every `Route` value, in order.
    pub routes: Vec<Bytes>,
}

impl CookieInput {
    /// Read the fields out of a request.
    #[must_use]
    pub fn from_request(request: &Request) -> Self {
        Self {
            request_uri: request.uri.to_bytes(),
            to_tag: tag_of(request, &HeaderName::To),
            from_tag: tag_of(request, &HeaderName::From),
            call_id: request
                .headers
                .value(&HeaderName::CallId)
                .map(|value| Bytes::copy_from_slice(trim(&value)))
                .unwrap_or_default(),
            cseq: cseq_number(request).unwrap_or(0),
            top_via: request
                .headers
                .get(&HeaderName::Via)
                .map(|header| Bytes::copy_from_slice(trim(&header.value()))),
            routes: request
                .headers
                .get_all(&HeaderName::Route)
                .map(|header| Bytes::copy_from_slice(trim(&header.value())))
                .collect(),
        }
    }

    /// The routing state, with an explicit length prefix per field.
    ///
    /// Length-prefixed rather than delimiter-joined: with a separator byte, a `Call-ID` that
    /// contained the separator could be spelled two ways that hash alike, and "these two different
    /// requests look like the same request" is exactly the confusion a loop detector must not have.
    ///
    /// The topmost `Via` is absent by design — see the module header.
    fn encode_routing_state(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut field = |bytes: &[u8]| {
            out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
            out.extend_from_slice(bytes);
        };
        field(&self.request_uri);
        field(self.to_tag.as_deref().unwrap_or(b""));
        field(self.from_tag.as_deref().unwrap_or(b""));
        field(&self.call_id);
        field(&self.cseq.to_be_bytes());
        field(
            &u32::try_from(self.routes.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        for route in &self.routes {
            field(route);
        }
        out
    }

    /// The loop-detection cookie for this routing state, under this key.
    #[must_use]
    pub fn cookie(&self, key: &CookieKey) -> String {
        hex(&mac(key, &self.encode_routing_state())[..COOKIE_BYTES])
    }
}

/// The branch for one fork of one request: `z9hG4bK-<unique>-<cookie>`.
///
/// The unique part is a MAC over the same state plus the branch index, so two forks of one request
/// differ, two requests never collide, and a replay of one request produces the identical branch —
/// which is what lets the kernel's transaction layer absorb a retransmission instead of forking a
/// second time.
#[must_use]
pub fn branch_for(config: &ProxyConfig, input: &CookieInput, index: usize) -> String {
    let cookie = input.cookie(&config.cookie_key);

    // The unique part is where the topmost `Via` belongs: RFC 3261 §16.6 step 8 recommends it as
    // entropy for transaction uniqueness, and unlike the cookie it is never recomputed and compared
    // against a later arrival.
    let mut unique_input = input.encode_routing_state();
    unique_input.extend_from_slice(b"/via/");
    unique_input.extend_from_slice(input.top_via.as_deref().unwrap_or(b""));
    unique_input.extend_from_slice(b"/branch/");
    unique_input.extend_from_slice(&u32::try_from(index).unwrap_or(u32::MAX).to_be_bytes());
    let unique = hex(&mac(&config.cookie_key, &unique_input)[..COOKIE_BYTES]);
    format!("{MAGIC_COOKIE}-{unique}-{cookie}")
}

/// The cookie carried by a branch string, if it has the shape we mint.
#[must_use]
pub fn cookie_of(branch: &str) -> Option<&str> {
    let rest = branch.strip_prefix(MAGIC_COOKIE)?.strip_prefix('-')?;
    let (_unique, cookie) = rest.rsplit_once('-')?;
    (!cookie.is_empty()).then_some(cookie)
}

/// HMAC-SHA256 over `message`, keyed by `key`.
///
/// The key is hashed to a fixed 32 bytes first. That is not for strength — HMAC accepts any key
/// length — but to make the length precondition true *by construction*, so there is no impossible
/// error branch to invent behaviour for. The `expect` that remains is on a fixed 32-byte slice,
/// which `Hmac<Sha256>` accepts unconditionally; the alternative was an infinite loop or a silent
/// fallback to an unkeyed MAC, and a loop-detector that silently loses its key is worse than one
/// that cannot compile.
#[allow(clippy::expect_used)]
fn mac(key: &CookieKey, message: &[u8]) -> [u8; 32] {
    let normalized: [u8; 32] = Sha256::digest(key.as_bytes()).into();
    let mut hmac = <Hmac<Sha256> as Mac>::new_from_slice(&normalized)
        .expect("HMAC-SHA256 accepts a 32-byte key");
    hmac.update(message);
    hmac.finalize().into_bytes().into()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble(byte >> 4));
        out.push(nibble(byte & 0x0f));
    }
    out
}

fn nibble(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'a' + (value - 10)),
    }
}

fn tag_of(request: &Request, name: &HeaderName) -> Option<Bytes> {
    let value = request.headers.value(name)?;
    let address = Address::parse(&value, "To").ok()?;
    address.tag().map(Bytes::copy_from_slice)
}

fn cseq_number(request: &Request) -> Option<u32> {
    let value = request.headers.value(&HeaderName::CSeq)?;
    String::from_utf8_lossy(&value)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn trim(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    value.get(start..end).unwrap_or(&[])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use sipx_sip::{Method, RequestBuilder, Uri};

    fn key() -> CookieKey {
        CookieKey::new(Bytes::from_static(b"cluster-cookie-key"))
    }

    fn config() -> ProxyConfig {
        ProxyConfig::new("edge-1.example", "<sip:edge-1.example;lr>", key())
    }

    fn request(uri: &str, extra: Vec<(HeaderName, &str)>) -> Request {
        let mut builder = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::copy_from_slice(uri.as_bytes())).unwrap(),
        )
        .header(HeaderName::CallId, "call-1")
        .unwrap()
        .cseq(1, &Method::Invite)
        .unwrap()
        .header(HeaderName::From, "<sip:alice@a.example>;tag=af")
        .unwrap()
        .header(HeaderName::To, "<sip:bob@b.example>")
        .unwrap()
        .header(
            HeaderName::Via,
            "SIP/2.0/UDP alice.example;branch=z9hG4bK-in",
        )
        .unwrap();
        for (name, value) in extra {
            builder = builder.header(name, value.to_owned()).unwrap();
        }
        builder.build()
    }

    #[test]
    fn the_same_routing_state_gives_the_same_cookie() {
        let a = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let b = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        assert_eq!(a.cookie(&key()), b.cookie(&key()));
    }

    #[test]
    fn a_different_request_uri_is_a_spiral_not_a_loop() {
        // RFC 5393: the cookie covers the state that determines processing. A changed Request-URI
        // means a different routing decision, which is a spiral and must be forwarded.
        let a = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let b = CookieInput::from_request(&request("sip:carol@b.example", vec![]));
        assert_ne!(a.cookie(&key()), b.cookie(&key()));
    }

    #[test]
    fn the_topmost_via_does_not_change_the_cookie() {
        // The deviation, asserted. If this ever fails, loop detection has silently stopped working:
        // a looping request arrives with *our* Via on top, so a cookie that depended on it would
        // never match the one we minted.
        let plain = request("sip:bob@b.example", vec![]);
        let mut relayed = CookieInput::from_request(&plain);
        relayed.top_via = Some(Bytes::from_static(
            b"SIP/2.0/UDP edge-1.example;branch=z9hG4bK-ours",
        ));
        assert_eq!(
            CookieInput::from_request(&plain).cookie(&key()),
            relayed.cookie(&key())
        );
    }

    #[test]
    fn the_topmost_via_does_change_the_branch() {
        // It is still entropy for uniqueness — just not for loop detection.
        let config = config();
        let plain = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let mut relayed = plain.clone();
        relayed.top_via = Some(Bytes::from_static(
            b"SIP/2.0/UDP other.example;branch=z9hG4bK-x",
        ));
        assert_ne!(
            branch_for(&config, &plain, 0),
            branch_for(&config, &relayed, 0)
        );
    }

    #[test]
    fn a_different_route_set_changes_the_cookie() {
        let a = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let b = CookieInput::from_request(&request(
            "sip:bob@b.example",
            vec![(HeaderName::Route, "<sip:p1.example;lr>")],
        ));
        assert_ne!(a.cookie(&key()), b.cookie(&key()));
    }

    #[test]
    fn a_different_key_gives_a_different_cookie() {
        // The forgery defence: without the key, an outsider can compute the fields but not the
        // cookie, so they cannot craft a request that claims "not a loop".
        let input = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let other = CookieKey::new(Bytes::from_static(b"another-key"));
        assert_ne!(input.cookie(&key()), input.cookie(&other));
    }

    #[test]
    fn length_prefixing_stops_two_states_hashing_alike() {
        // With a delimiter-joined encoding these two would produce the same MAC input, and the
        // proxy would call a legitimate spiral a loop — or worse, miss a real one.
        let mut a = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let mut b = a.clone();
        a.call_id = Bytes::from_static(b"x");
        a.from_tag = Some(Bytes::from_static(b"yz"));
        b.call_id = Bytes::from_static(b"xy");
        b.from_tag = Some(Bytes::from_static(b"z"));
        assert_ne!(a.cookie(&key()), b.cookie(&key()));
    }

    #[test]
    fn forks_of_one_request_get_different_branches_but_the_same_cookie() {
        let config = config();
        let input = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let first = branch_for(&config, &input, 0);
        let second = branch_for(&config, &input, 1);
        assert_ne!(first, second, "§16.6 step 8: unique per client transaction");
        assert_eq!(
            cookie_of(&first),
            cookie_of(&second),
            "same request, same loop-detection cookie"
        );
    }

    #[test]
    fn every_branch_carries_the_magic_cookie() {
        let branch = branch_for(
            &config(),
            &CookieInput::from_request(&request("sip:bob@b.example", vec![])),
            0,
        );
        assert!(branch.starts_with(MAGIC_COOKIE), "{branch}");
    }

    #[test]
    fn a_retransmission_produces_the_identical_branch() {
        // Which is what lets the kernel's transaction layer absorb it instead of forking twice.
        let config = config();
        let first = branch_for(
            &config,
            &CookieInput::from_request(&request("sip:bob@b.example", vec![])),
            0,
        );
        let again = branch_for(
            &config,
            &CookieInput::from_request(&request("sip:bob@b.example", vec![])),
            0,
        );
        assert_eq!(first, again);
    }

    #[test]
    fn a_cookie_can_be_read_back_out_of_a_branch() {
        let config = config();
        let input = CookieInput::from_request(&request("sip:bob@b.example", vec![]));
        let branch = branch_for(&config, &input, 0);
        assert_eq!(cookie_of(&branch), Some(input.cookie(&key()).as_str()));
    }

    #[test]
    fn a_foreign_branch_yields_no_cookie() {
        assert_eq!(cookie_of("z9hG4bKsomethingelse"), None);
        assert_eq!(cookie_of("not-a-branch"), None);
        assert_eq!(cookie_of("z9hG4bK-"), None);
    }
}
