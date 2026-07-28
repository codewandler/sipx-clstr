//! The correlation marker — [e2e-probe §4](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md).
//!
//! Without one, "the call was answered" cannot tell *this* run's call from a stale dialog, a
//! duplicate, or another probe reaching the same echo. With one, the design's named scenario — the
//! edge answers `200` but the echo never rang — is detectable rather than invisible.

use std::fmt;

use bytes::Bytes;
use sipx_sip::{HeaderName, Message};

/// The header the marker travels in, outbound and back.
///
/// `Subject` because RFC 3261 §20.42 gives no intermediary a reason to alter it, so a marker that
/// does not come back is evidence about the **path**. An `X-` header nothing is required to preserve
/// would let a header-stripping element look like a routing fault.
pub const MARKER_HEADER: HeaderName = HeaderName::Subject;

/// The prefix that makes a `Subject` recognisably ours.
pub const MARKER_PREFIX: &str = "sipx-probe/";

/// How many bytes of entropy a marker carries (§4 M1).
pub const MARKER_BYTES: usize = 16;

/// A run-unique opaque token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Marker(String);

impl Marker {
    /// Mint one from an injected random source.
    ///
    /// The source is a parameter, never a thread RNG: it is what lets a harness replay a run, and a
    /// probe whose failure scenarios cannot be replayed from a seed is a probe nobody will trust at
    /// 03:00.
    ///
    /// **Not derived from the `Call-ID`** (§4 M6). They are correlated but not equal: a marker that
    /// could be predicted from a `Call-ID` could be reflected by something that merely saw the
    /// request go past, which is exactly the confusion the marker exists to prevent.
    pub fn mint(random: &mut impl FnMut() -> u8) -> Self {
        let bytes: Vec<u8> = (0..MARKER_BYTES).map(|_| random()).collect();
        Self(base64_url(&bytes))
    }

    /// A marker from a known value — for vectors that need a specific one.
    #[must_use]
    pub fn from_token(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The token, without the prefix.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.0
    }

    /// The full header value: `sipx-probe/<token>`.
    #[must_use]
    pub fn header_value(&self) -> String {
        format!("{MARKER_PREFIX}{}", self.0)
    }

    /// Read a marker out of a message, if it carries one of ours.
    #[must_use]
    pub fn of(message: &Message) -> Option<Self> {
        let value = message.headers().value(&MARKER_HEADER)?;
        let text = String::from_utf8_lossy(&value);
        let token = text.trim().strip_prefix(MARKER_PREFIX)?;
        (!token.is_empty()).then(|| Self(token.to_owned()))
    }

    /// The header bytes, for building a request.
    #[must_use]
    pub fn header_bytes(&self) -> Bytes {
        Bytes::from(self.header_value())
    }
}

impl fmt::Display for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// URL-safe base64 without padding.
///
/// Written out rather than pulled in: it is twenty lines, the alphabet is fixed by RFC 4648 §5, and a
/// dependency for this would be a dependency to audit and upgrade forever.
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let indices = [
            (triple >> 18) & 0x3f,
            (triple >> 12) & 0x3f,
            (triple >> 6) & 0x3f,
            triple & 0x3f,
        ];
        // One character per 6 bits, but only as many as the input actually filled: padding would add
        // `=` characters, which are not `token` characters in a SIP header value.
        let produced = match chunk.len() {
            1 => 2,
            2 => 3,
            _ => 4,
        };
        for index in indices.iter().take(produced) {
            let position = usize::try_from(*index).unwrap_or(0);
            if let Some(byte) = ALPHABET.get(position) {
                out.push(char::from(*byte));
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use sipx_sip::{Method, RequestBuilder, Uri};

    fn counter() -> impl FnMut() -> u8 {
        let mut next = 0_u8;
        move || {
            next = next.wrapping_add(7);
            next
        }
    }

    #[test]
    fn a_marker_round_trips_through_a_header() {
        let marker = Marker::mint(&mut counter());
        let request = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::from_static(b"sip:echo@test.example")).unwrap(),
        )
        .header(MARKER_HEADER, marker.header_bytes())
        .unwrap()
        .build();

        assert_eq!(Marker::of(&Message::Request(request)), Some(marker));
    }

    #[test]
    fn a_message_with_no_subject_carries_no_marker() {
        let request = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::from_static(b"sip:echo@test.example")).unwrap(),
        )
        .build();
        assert_eq!(Marker::of(&Message::Request(request)), None);
    }

    #[test]
    fn someone_elses_subject_is_not_a_marker() {
        // A UA that sets a human-readable Subject must not be mistaken for our echo.
        let request = RequestBuilder::new(
            Method::Invite,
            Uri::parse(Bytes::from_static(b"sip:echo@test.example")).unwrap(),
        )
        .header(MARKER_HEADER, "Weekly sync")
        .unwrap()
        .build();
        assert_eq!(Marker::of(&Message::Request(request)), None);
    }

    #[test]
    fn a_marker_is_url_safe_and_unpadded() {
        // `=`, `+` and `/` are not `token` characters in a SIP header value, and a marker that had to
        // be quoted would be a marker something could re-quote differently.
        let marker = Marker::mint(&mut counter());
        assert!(
            marker
                .token()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "{marker}"
        );
    }

    #[test]
    fn markers_from_different_draws_differ() {
        let mut source = counter();
        let first = Marker::mint(&mut source);
        let second = Marker::mint(&mut source);
        assert_ne!(first, second);
    }

    #[test]
    fn the_encoding_matches_rfc_4648_test_vectors() {
        // Pinned against the RFC's own examples rather than against this function's output, so an
        // encoding bug cannot be frozen in by recording what it happened to produce.
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(b"foob"), "Zm9vYg");
        assert_eq!(base64_url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64_url(b"foobar"), "Zm9vYmFy");
        // The two characters that distinguish the URL-safe alphabet from the standard one.
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }
}
