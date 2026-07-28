//! What this proxy is, and what it will accept.
//!
//! Identity is a *set*: recognizing "this platform" covers every configured edge identity, not only
//! the receiving node ([proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! §5). Any edge pops any edge's `Route` — that is the whole point of the affinity token, and a
//! node that only recognized itself would break mid-dialog routing the moment a flow moved.

use std::time::Duration;

use bytes::Bytes;

/// The key the loop-detection cookie is computed under (§6).
///
/// Keyed so an outsider cannot forge "not a loop" and drive a request round a cycle until it
/// exhausts `Max-Forwards` at every node on the way. Distribution and rotation belong to `AF-6`;
/// this type is only the shape, and the cluster token key family is where the value comes from.
#[derive(Clone, PartialEq, Eq)]
pub struct CookieKey(Bytes);

impl CookieKey {
    /// A key from raw bytes.
    #[must_use]
    pub fn new(bytes: impl Into<Bytes>) -> Self {
        Self(bytes.into())
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

// Deliberately opaque. A key that prints itself ends up in a log, and a log is not where the thing
// that stops loop-forgery belongs.
impl std::fmt::Debug for CookieKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CookieKey({} bytes, redacted)", self.0.len())
    }
}

/// One identity this platform answers to: a host, optionally a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeIdentity {
    /// The host, compared case-insensitively (RFC 3261 §19.1.4).
    pub host: String,
    /// The port, if this identity is port-specific.
    pub port: Option<u16>,
}

impl EdgeIdentity {
    /// An identity that matches a host on any port.
    #[must_use]
    pub fn host(host: &str) -> Self {
        Self {
            host: host.to_ascii_lowercase(),
            port: None,
        }
    }

    /// An identity that matches a host on one port.
    #[must_use]
    pub fn host_port(host: &str, port: u16) -> Self {
        Self {
            host: host.to_ascii_lowercase(),
            port: Some(port),
        }
    }

    /// Whether a URI names this identity.
    #[must_use]
    pub fn matches(&self, uri: &sipx_sip::Uri) -> bool {
        let Some(host) = uri.host() else {
            return false;
        };
        let text = String::from_utf8_lossy(&host.to_bytes()).to_ascii_lowercase();
        // A single trailing dot spells the same label sequence (§25's hostname grammar).
        let text = text.strip_suffix('.').unwrap_or(&text).to_owned();
        if text != self.host {
            return false;
        }
        match self.port {
            // An identity with no port matches whatever port arrives: a proxy reached on 5061
            // because a client resolved SRV differently is still this proxy.
            None => true,
            Some(port) => uri.port() == Some(port),
        }
    }
}

/// The proxy's configuration.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Every identity this platform answers to (§5).
    pub identities: Vec<EdgeIdentity>,
    /// The `Record-Route` URI this node inserts, without the token parameter (F4).
    pub record_route_uri: Bytes,
    /// The option tags the active profile supports, for V6.
    pub supported: Vec<String>,
    /// URI schemes this proxy will forward, for V2.
    pub schemes: Vec<String>,
    /// `Max-Forwards` to insert when the request carries none.
    ///
    /// §16.6 allows a proxy to add one; this platform does, so V3 is always meaningful downstream
    /// rather than only for requests that happened to arrive with the header.
    pub default_max_forwards: u8,
    /// `Max-Breadth` assumed when the request carries none (RFC 5393 §5.2's recommended 60).
    pub default_max_breadth: u32,
    /// Timer C for INVITE branches. §16.8 requires "larger than 3 minutes"; 180 s is the floor and
    /// the default, and a smaller value is refused rather than silently raised.
    pub timer_c: Duration,
    /// The cookie key (§6).
    pub cookie_key: CookieKey,
}

impl ProxyConfig {
    /// A configuration for one identity, with the spec's defaults.
    #[must_use]
    pub fn new(host: &str, record_route_uri: impl Into<Bytes>, cookie_key: CookieKey) -> Self {
        Self {
            identities: vec![EdgeIdentity::host(host)],
            record_route_uri: record_route_uri.into(),
            supported: Vec::new(),
            schemes: vec!["sip".to_owned(), "sips".to_owned()],
            default_max_forwards: 70,
            default_max_breadth: 60,
            timer_c: Duration::from_secs(180),
            cookie_key,
        }
    }

    /// Whether a URI names this platform — any edge of it (§5).
    #[must_use]
    pub fn is_ours(&self, uri: &sipx_sip::Uri) -> bool {
        self.identities.iter().any(|identity| identity.matches(uri))
    }

    /// Whether the profile supports an option tag (V6).
    #[must_use]
    pub fn supports(&self, tag: &str) -> bool {
        let tag = tag.to_ascii_lowercase();
        self.supported
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&tag))
    }

    /// Whether a URI scheme is one this proxy forwards (V2).
    #[must_use]
    pub fn understands_scheme(&self, uri: &sipx_sip::Uri) -> bool {
        let scheme = String::from_utf8_lossy(uri.scheme().as_bytes()).to_ascii_lowercase();
        self.schemes
            .iter()
            .any(|known| known.eq_ignore_ascii_case(&scheme))
    }

    /// Timer C, never below §16.8's floor.
    ///
    /// A configuration that asked for 30 s would make the proxy cancel branches that RFC 3261
    /// considers healthy, so the floor is enforced here rather than trusted to the operator.
    #[must_use]
    pub fn effective_timer_c(&self) -> Duration {
        self.timer_c.max(Duration::from_secs(180))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use sipx_sip::Uri;

    fn uri(text: &str) -> Uri {
        Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
    }

    fn config() -> ProxyConfig {
        ProxyConfig::new(
            "edge-1.example",
            "<sip:edge-1.example;lr>",
            CookieKey::new(Bytes::from_static(b"key")),
        )
    }

    #[test]
    fn an_identity_matches_case_insensitively_and_through_a_trailing_dot() {
        let identity = EdgeIdentity::host("edge-1.example");
        assert!(identity.matches(&uri("sip:EDGE-1.Example;lr")));
        assert!(identity.matches(&uri("sip:edge-1.example.;lr")));
        assert!(!identity.matches(&uri("sip:edge-2.example;lr")));
    }

    #[test]
    fn a_port_specific_identity_does_not_match_another_port() {
        let identity = EdgeIdentity::host_port("edge-1.example", 5060);
        assert!(identity.matches(&uri("sip:edge-1.example:5060;lr")));
        assert!(!identity.matches(&uri("sip:edge-1.example:5061;lr")));
        assert!(!identity.matches(&uri("sip:edge-1.example;lr")));
    }

    #[test]
    fn any_edge_recognizes_any_edges_identity() {
        // §5: "Recognizing 'this platform' covers every configured edge identity, not only the
        // receiving node" — a node that only knew itself would drop mid-dialog requests that the
        // token exists precisely to let it handle.
        let mut config = config();
        config.identities.push(EdgeIdentity::host("edge-2.example"));
        assert!(config.is_ours(&uri("sip:edge-2.example;lr")));
        assert!(!config.is_ours(&uri("sip:elsewhere.example;lr")));
    }

    #[test]
    fn only_the_configured_schemes_are_understood() {
        let config = config();
        assert!(config.understands_scheme(&uri("sip:bob@example.test")));
        assert!(config.understands_scheme(&uri("sips:bob@example.test")));
        assert!(!config.understands_scheme(&uri("tel:+15550101")));
    }

    #[test]
    fn timer_c_never_drops_below_the_rfc_floor() {
        let mut config = config();
        config.timer_c = Duration::from_secs(30);
        assert_eq!(config.effective_timer_c(), Duration::from_secs(180));
        config.timer_c = Duration::from_secs(240);
        assert_eq!(config.effective_timer_c(), Duration::from_secs(240));
    }

    #[test]
    fn the_cookie_key_does_not_print_itself() {
        let debug = format!("{:?}", CookieKey::new(Bytes::from_static(b"secret")));
        assert!(!debug.contains("secret"), "{debug}");
        assert!(debug.contains("redacted"), "{debug}");
    }
}
