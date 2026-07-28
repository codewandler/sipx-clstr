//! The canonical address-of-record key.
//!
//! RFC 3261 §19.1.4 equivalence is a pairwise *comparison*, and it is not transitive — the RFC's
//! own `;security=on`/`;security=off` example says so. The kernel implements it and deliberately
//! refuses to be `PartialEq` for that reason. A storage key and a rendezvous-hash input need the
//! opposite: a total function whose byte output is compared by equality, transitive by
//! construction. Two spellings of one AoR that produced two keys would land on two shards, and
//! the whole design forbids that.
//!
//! RFC 3261 §10.3 step 5 provides the anchor — convert the AoR to a canonical form with all URI
//! parameters removed (including `user`) and escaped characters unescaped. What is here is a
//! deterministic, injective, printable encoding of exactly that URI, plus the case and literal
//! normalizations §19.1.4 already calls meaningless.
//!
//! Specification: [location-service §3](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).

use std::fmt;
use std::net::IpAddr;

use bytes::Bytes;
use sipx_sip::{Host, Scheme, Uri};

/// The longest canonical key this platform will store.
///
/// Keys bound storage rows, change-stream payloads and hash inputs. An unbounded key is a
/// resource-exhaustion vector, so N13 caps it rather than discovering the cap in production.
pub const MAX_KEY_BYTES: usize = 512;

/// Why a URI cannot be an address-of-record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AorError {
    /// Not a `sip:` or `sips:` URI (§3.2).
    #[error("an address-of-record must be a sip or sips URI")]
    NotSipUri,
    /// The URI carries headers (`?…`), which N9 rejects.
    ///
    /// An AoR is an index, not a message template. §19.1.4 says headers are never ignored, so
    /// silently stripping them would conflate URIs the RFC calls different.
    #[error("an address-of-record must not carry URI headers")]
    UriHeaders,
    /// A password in the userinfo, which N10 rejects.
    ///
    /// §19.1.1 already recommends against it; more to the point, a credential must not become key
    /// material that flows into logs, hashes and change-stream payloads.
    #[error("an address-of-record must not carry a password")]
    Password,
    /// `sip:@host` — an empty user part (N11).
    ///
    /// The `user` production is `1*(…)`: an empty user is not a spelling of "no user".
    #[error("an empty user part is not an address-of-record")]
    EmptyUser,
    /// No host to key on.
    #[error("an address-of-record must name a host")]
    NoHost,
    /// The canonical form would exceed [`MAX_KEY_BYTES`] (N13).
    #[error("the canonical key would be {size} bytes, over the {MAX_KEY_BYTES} limit")]
    TooLong {
        /// How long it would have been.
        size: usize,
    },
}

/// A canonical address-of-record: printable ASCII, compared by equality.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalAor(String);

impl CanonicalAor {
    /// Canonicalize a kernel-parsed URI, per §3.2's rules N1–N13.
    pub fn from_uri(uri: &Uri) -> Result<Self, AorError> {
        // N1: sip and sips are distinct keys and nothing else is an AoR at all.
        let scheme = match uri.scheme() {
            Scheme::Sip => "sip",
            Scheme::Sips => "sips",
            _ => return Err(AorError::NotSipUri),
        };

        if uri.has_headers() {
            return Err(AorError::UriHeaders);
        }
        if uri.password().is_some() {
            return Err(AorError::Password);
        }

        // N2 and N11. `decoded_user` gives the §10.3 step 5 unescaped bytes; re-encoding them is
        // what keeps the key printable and delimiter-safe — a raw decoded `@` or NUL in a key
        // could otherwise forge key structure.
        let user = match uri.user() {
            Some(b"") => return Err(AorError::EmptyUser),
            Some(_) => {
                let decoded = uri.decoded_user().ok_or(AorError::EmptyUser)?;
                if decoded.is_empty() {
                    return Err(AorError::EmptyUser);
                }
                Some(escape_user(&decoded))
            }
            None => None,
        };

        // N5, N6.
        let host = match uri.host().ok_or(AorError::NoHost)? {
            Host::Name(name) => {
                let text = String::from_utf8_lossy(name.as_bytes()).to_lowercase();
                // The §25 `hostname` grammar admits one optional trailing dot for the identical
                // label sequence. Two spellings of one FQDN must not shard differently.
                text.strip_suffix('.').unwrap_or(&text).to_owned()
            }
            Host::Ip(IpAddr::V4(v4)) => v4.to_string(),
            // Rust's `Ipv6Addr` Display is already RFC 5952's recommended form.
            Host::Ip(IpAddr::V6(v6)) => format!("[{v6}]"),
        };
        if host.is_empty() {
            return Err(AorError::NoHost);
        }

        // N7: an absent port and an explicit `:5060` are different keys, and leading zeros are
        // not part of the value.
        let port = uri
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();

        // N8 is by construction: nothing above reads `params()`.
        let key = match user {
            Some(user) => format!("{scheme}:{user}@{host}{port}"),
            None => format!("{scheme}:{host}{port}"),
        };

        if key.len() > MAX_KEY_BYTES {
            return Err(AorError::TooLong { size: key.len() });
        }
        Ok(Self(key))
    }

    /// Canonicalize from raw URI bytes, parsing with the kernel first.
    ///
    /// A parse failure is `NotSipUri`: N12 makes canonicalization a partial function over
    /// *kernel-parsed* URIs, because guessing at a key from a broken URI can only mis-shard.
    pub fn parse(raw: impl Into<Bytes>) -> Result<Self, AorError> {
        let uri = Uri::parse(raw.into()).map_err(|_| AorError::NotSipUri)?;
        Self::from_uri(&uri)
    }

    /// The key bytes. Printable ASCII, NUL-free — which is what makes §8's shard key injective.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The key as bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Display for CanonicalAor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// RFC 2396 `unreserved`: alphanumerics and `- _ . ! ~ * ' ( )`.
fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

/// Re-encode decoded user bytes: unreserved raw, everything else `%HH` with uppercase hex.
fn escape_user(decoded: &[u8]) -> String {
    let mut out = String::with_capacity(decoded.len());
    for byte in decoded {
        if is_unreserved(*byte) {
            out.push(char::from(*byte));
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4));
            out.push(hex_digit(byte & 0x0f));
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        _ => char::from(b'A' + (nibble - 10)),
    }
}

/// The rendezvous-hash and storage key for an AoR within a tenant (§8).
///
/// Injective because a tenant id contains no NUL byte (§4) and a canonical AoR is printable ASCII
/// (§3.2 escapes every non-printable byte). Two spellings of one AoR hashing to two shards is the
/// failure canonicalization exists to make impossible, so this is deliberately the *only* place
/// the two are joined.
#[must_use]
pub fn shard_key(tenant: &str, aor: &CanonicalAor) -> Vec<u8> {
    let mut key = Vec::with_capacity(tenant.len() + 1 + aor.as_bytes().len());
    key.extend_from_slice(tenant.as_bytes());
    key.push(0);
    key.extend_from_slice(aor.as_bytes());
    key
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The LS-C table of [location-service §9], row by row.
    fn canon(text: &str) -> Result<String, AorError> {
        CanonicalAor::parse(Bytes::copy_from_slice(text.as_bytes()))
            .map(|aor| aor.as_str().to_owned())
    }

    #[test]
    fn ls_c_1_host_folds() {
        assert_eq!(
            canon("sip:alice@ATLANTA.com").unwrap(),
            "sip:alice@atlanta.com"
        );
    }

    #[test]
    fn ls_c_2_scheme_folds_and_parameters_are_stripped() {
        assert_eq!(
            canon("SIP:alice@AtLanTa.CoM;Transport=tcp").unwrap(),
            "sip:alice@atlanta.com"
        );
    }

    #[test]
    fn ls_c_3_an_escaped_unreserved_character_decodes() {
        assert_eq!(
            canon("sip:%61lice@atlanta.com").unwrap(),
            "sip:alice@atlanta.com"
        );
    }

    #[test]
    fn ls_c_4_the_user_part_is_case_sensitive() {
        assert_eq!(
            canon("sip:Alice@atlanta.com").unwrap(),
            "sip:Alice@atlanta.com"
        );
        assert_ne!(
            canon("sip:Alice@atlanta.com"),
            canon("sip:alice@atlanta.com")
        );
    }

    #[test]
    fn ls_c_5_and_6_an_explicit_port_is_a_different_key_and_loses_leading_zeros() {
        assert_eq!(
            canon("sip:alice@atlanta.com:5060").unwrap(),
            "sip:alice@atlanta.com:5060"
        );
        assert_eq!(
            canon("sip:alice@atlanta.com:05060").unwrap(),
            "sip:alice@atlanta.com:5060"
        );
        assert_ne!(
            canon("sip:alice@atlanta.com:5060"),
            canon("sip:alice@atlanta.com")
        );
    }

    #[test]
    fn ls_c_7_sips_is_never_sip() {
        assert_eq!(
            canon("sips:alice@atlanta.com").unwrap(),
            "sips:alice@atlanta.com"
        );
        assert_ne!(
            canon("sips:alice@atlanta.com"),
            canon("sip:alice@atlanta.com")
        );
    }

    #[test]
    fn ls_c_8_a_trailing_dot_is_the_same_name() {
        assert_eq!(
            canon("sip:alice@atlanta.com.").unwrap(),
            "sip:alice@atlanta.com"
        );
    }

    #[test]
    fn ls_c_9_a_raw_and_an_escaped_delimiter_are_one_key() {
        // Deliberately coarser than §19.1.4, per §10.3 step 5's full decode.
        let raw = canon("sip:a;b@h.example").unwrap();
        let escaped = canon("sip:a%3Bb@h.example").unwrap();
        assert_eq!(raw, "sip:a%3Bb@h.example");
        assert_eq!(raw, escaped);
    }

    #[test]
    fn ls_c_10_escape_hex_is_uppercased() {
        assert_eq!(
            canon("sip:alice%2fx@h.example").unwrap(),
            canon("sip:alice%2Fx@h.example").unwrap()
        );
        assert_eq!(
            canon("sip:alice%2fx@h.example").unwrap(),
            "sip:alice%2Fx@h.example"
        );
    }

    #[test]
    fn ls_c_11_the_user_parameter_is_stripped_and_plus_re_escapes() {
        assert_eq!(
            canon("sip:+15550101@gw.example;user=phone").unwrap(),
            "sip:%2B15550101@gw.example"
        );
    }

    #[test]
    fn ls_c_12_there_is_no_tel_folding_inside_a_sip_user_part() {
        assert_eq!(
            canon("sip:+1-555-0101@gw.example").unwrap(),
            "sip:%2B1-555-0101@gw.example"
        );
        assert_ne!(
            canon("sip:+1-555-0101@gw.example"),
            canon("sip:+15550101@gw.example")
        );
    }

    #[test]
    fn ls_c_13_ipv6_takes_its_rfc_5952_form() {
        assert_eq!(
            canon("sip:bob@[2001:DB8:0:0:0:0:0:1]").unwrap(),
            "sip:bob@[2001:db8::1]"
        );
        assert_eq!(
            canon("sip:bob@[2001:DB8:0:0:0:0:0:1]").unwrap(),
            canon("sip:bob@[2001:db8::1]").unwrap()
        );
    }

    #[test]
    fn ls_c_14_a_hostname_never_becomes_an_address() {
        assert_ne!(canon("sip:bob@phone.example"), canon("sip:bob@192.0.2.4"));
    }

    #[test]
    fn ls_c_15_a_userless_aor_is_valid_and_distinct() {
        assert_eq!(canon("sip:h.example").unwrap(), "sip:h.example");
        assert_ne!(canon("sip:h.example"), canon("sip:x@h.example"));
    }

    #[test]
    fn ls_c_16_a_nul_stays_escaped_so_the_key_stays_printable() {
        let key = canon("sip:null-%00-null@h.example").unwrap();
        assert_eq!(key, "sip:null-%00-null@h.example");
        assert!(
            !key.as_bytes().contains(&0),
            "a key with a NUL breaks the shard key"
        );
    }

    #[test]
    fn ls_c_17_an_empty_user_is_rejected() {
        assert!(canon("sip:@h.example").is_err());
    }

    #[test]
    fn ls_c_18_a_password_is_rejected() {
        assert_eq!(canon("sip:alice:secret@h.example"), Err(AorError::Password));
    }

    #[test]
    fn ls_c_19_uri_headers_are_rejected() {
        assert_eq!(
            canon("sip:alice@h.example?subject=x"),
            Err(AorError::UriHeaders)
        );
    }

    #[test]
    fn ls_c_20_a_tel_uri_is_not_an_address_of_record() {
        assert_eq!(canon("tel:+15550101"), Err(AorError::NotSipUri));
    }

    #[test]
    fn ls_c_21_a_malformed_escape_is_rejected() {
        assert!(canon("sip:alice%zz@h.example").is_err());
    }

    #[test]
    fn ls_c_22_an_over_long_key_is_rejected() {
        let long = "a".repeat(MAX_KEY_BYTES);
        match canon(&format!("sip:{long}@h.example")) {
            Err(AorError::TooLong { size }) => assert!(size > MAX_KEY_BYTES),
            other => panic!("expected a length rejection, got {other:?}"),
        }
    }

    #[test]
    fn ls_h_the_shard_key_is_the_tenant_a_nul_and_the_canonical_aor() {
        let aor = CanonicalAor::parse(Bytes::from_static(b"sip:alice@ATLANTA.com")).unwrap();
        assert_eq!(shard_key("t1", &aor), b"t1\0sip:alice@atlanta.com".to_vec());
    }

    #[test]
    fn ls_h_2_spelling_variants_shard_identically() {
        let a = CanonicalAor::parse(Bytes::from_static(b"sip:alice@ATLANTA.com")).unwrap();
        let b = CanonicalAor::parse(Bytes::from_static(b"SIP:%61lice@Atlanta.com")).unwrap();
        assert_eq!(shard_key("t1", &a), shard_key("t1", &b));
    }

    #[test]
    fn ls_h_3_tenants_never_share_a_shard_domain() {
        let aor = CanonicalAor::parse(Bytes::from_static(b"sip:alice@atlanta.com")).unwrap();
        assert_ne!(shard_key("t1", &aor), shard_key("t2", &aor));
    }

    #[test]
    fn the_shard_key_separator_cannot_be_forged_from_either_side() {
        // The injectivity argument, made executable: a tenant cannot contain NUL (§4) and a
        // canonical AoR cannot either (§3.2 escapes it), so the first NUL is always the join.
        let aor = CanonicalAor::parse(Bytes::from_static(b"sip:null-%00-x@h.example")).unwrap();
        let key = shard_key("t1", &aor);
        assert_eq!(key.iter().filter(|byte| **byte == 0).take(2).count(), 1);
    }
}
