//! What this proxy is, and what it will accept.
//!
//! Identity is a *set*: recognizing "this platform" covers every configured edge identity, not only
//! the receiving node ([proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! §5). Any edge pops any edge's `Route` — that is the whole point of the affinity token, and a
//! node that only recognized itself would break mid-dialog routing the moment a flow moved.

use std::time::Duration;

use bytes::Bytes;

/// Timer C's floor, and it is **exclusive**: a legal Timer C is strictly larger than this.
///
/// RFC 3261 §16.6 step 11 — "the timer MUST be larger than 3 minutes". A MUST over a strict
/// inequality, with no SHOULD and no rounding language near it, so reading it as `≥` would admit
/// exactly the value the RFC forbids. §16.8 is "Processing Timer C" and states no bound at all; it
/// was cited here, and in [proxy-behavior] F11, until `PX-10`.
///
/// Stated in seconds, as F11 and the RFC state it, so the bound and [`DEFAULT_TIMER_C`] cannot be
/// misread for each other.
///
/// [proxy-behavior]: https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md
#[allow(clippy::duration_suboptimal_units)]
pub const TIMER_C_FLOOR: Duration = Duration::from_secs(180);

/// Timer C's default: the smallest whole-minute value **above** [`TIMER_C_FLOOR`].
///
/// It was 180 s — the floor exactly, and therefore the one value RFC 3261 forbids — in this crate
/// until `PX-10`, and in the configuration schema until `DP-12`. The number here and
/// `cluster-config` §8 V7's `timerC` default are deliberately the same value: this is the default a
/// proxy built in code gets, that one is the default a document gets, and a proxy whose two
/// defaults disagreed is how the contradiction survived being fixed once.
///
/// Deliberately not raised further. Timer C is the only bound on a branch that has gone quiet since
/// its last provisional (§16.7 restarts it on each 101–199), so every extra minute is a minute a
/// wedged branch holds a proxied transaction — and, since `DP-11`, an admission slot.
#[allow(clippy::duration_suboptimal_units)]
pub const DEFAULT_TIMER_C: Duration = Duration::from_secs(240);

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
    /// Timer C for INVITE branches (F11).
    ///
    /// The floor is [`TIMER_C_FLOOR`] and it is exclusive; the default is [`DEFAULT_TIMER_C`]. What
    /// a *document* puts here is `cluster.timers.timerC`, carried by the node's `NodeConfig`.
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
            timer_c: DEFAULT_TIMER_C,
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

    /// Timer C, never at or below §16.6 step 11's floor.
    ///
    /// A configuration that asked for 30 s would make the proxy cancel branches RFC 3261 considers
    /// healthy, so the bound is enforced here rather than trusted to whoever built the value.
    ///
    /// **Why the fallback is the default and not the floor.** This was `.max(TIMER_C_FLOOR)` until
    /// `PX-10`, which honoured F11 as it then read — an *inclusive* floor under an *exclusive* RFC
    /// bound — and so answered a request for 180 s, or for anything below it, with exactly 180 s:
    /// the one value the RFC forbids, arrived at by the code whose job was to prevent it. Clamping
    /// to a strict bound has no correct value on the bound itself, so an unusable request falls back
    /// to [`DEFAULT_TIMER_C`] instead. A document cannot reach this path — `cluster-config` §8 V7
    /// refuses `timerC <= 180 s` at load — so what it protects is a `ProxyConfig` built in code.
    #[must_use]
    pub fn effective_timer_c(&self) -> Duration {
        if self.timer_c > TIMER_C_FLOOR {
            self.timer_c
        } else {
            DEFAULT_TIMER_C
        }
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
    // Every value here is a Timer C reading in seconds, and the 30 it starts from is not a whole
    // minute: converting only some of them would hide which of them is the floor.
    #[allow(clippy::duration_suboptimal_units)]
    fn timer_c_never_sits_on_or_below_the_rfc_floor() {
        let mut config = config();

        // The bound is strict (§16.6 step 11), so the floor itself is not a legal answer — this is
        // the case that made the merge base arm exactly the forbidden 180 s.
        config.timer_c = TIMER_C_FLOOR;
        assert_eq!(config.effective_timer_c(), DEFAULT_TIMER_C);

        config.timer_c = Duration::from_secs(30);
        assert_eq!(config.effective_timer_c(), DEFAULT_TIMER_C);

        // One second over the floor is legal, and is honoured rather than rounded to anything.
        config.timer_c = TIMER_C_FLOOR + Duration::from_secs(1);
        assert_eq!(config.effective_timer_c(), Duration::from_secs(181));

        config.timer_c = Duration::from_secs(300);
        assert_eq!(config.effective_timer_c(), Duration::from_secs(300));
    }

    #[test]
    fn the_default_timer_c_satisfies_the_floor_it_is_declared_beside() {
        // The `DP-12` defect in this crate's half: a default that its own rule refuses cannot be
        // accepted by omission, and no operator can fix a default by writing nothing.
        assert!(DEFAULT_TIMER_C > TIMER_C_FLOOR);
        assert_eq!(config().effective_timer_c(), DEFAULT_TIMER_C);
    }

    #[test]
    fn the_cookie_key_does_not_print_itself() {
        let debug = format!("{:?}", CookieKey::new(Bytes::from_static(b"secret")));
        assert!(!debug.contains("secret"), "{debug}");
        assert!(debug.contains("redacted"), "{debug}");
    }
}
