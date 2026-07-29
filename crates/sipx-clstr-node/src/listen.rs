//! Listeners: what a node binds, and what it tells the world to use.
//!
//! A node reached through a NAT, a load balancer or a host-networked cloud instance has two
//! addresses, and only one of them is an answer to "where do I reach you". Every place an address
//! enters a message — the `Via` sent-by (RFC 3261 §18.1.1), a `Contact` (§8.1.1.8), the
//! `Record-Route` a proxy inserts (§16.6 step 4) — must carry the **advertised** one. The bound one
//! is a fact about a socket; a peer cannot route to it, and a `Record-Route` that names it produces
//! a dialog whose second request goes nowhere.
//!
//! **Which address that is, is a decision, not a socket property** (AGENTS.md #2). Everything in
//! this module is a pure function of the declared configuration: nothing here binds, resolves,
//! reads a clock or touches the network, so all of it runs in the deterministic harness. The
//! driver takes the answers and performs them.
//!
//! # Considered for upstream
//!
//! **No — this is orchestration, and the kernel already owns its half.** `sipx_transport::Config`
//! separates `bind` from `sent_by`/`sent_by_port` precisely because "behind a NAT or a load
//! balancer the two differ", and the endpoint stamps that sent-by into every `Via` it writes.
//! Re-deriving a `Via` here would shadow kernel logic, which this platform does not do. What stays
//! here is the part the kernel has no opinion about: a *set* of declared listeners, one per
//! transport, mapped onto that field and onto the two headers the kernel never writes —
//! `Record-Route`, which is proxy orchestration, and a `Contact` naming this platform.
//!
//! One gap sits on the boundary and is upstream's, not ours: the kernel derives the TLS sent-by
//! port from the port it *bound* TLS on ([`sipx_transport::Handle::sent_by_for`]), so a TLS
//! listener whose advertised port differs from its bound port cannot be expressed through
//! `Config`. Advertising a different *host* works for all three transports; advertising a different
//! *port* works for UDP and TCP. The decisions in this module are correct for all three either way,
//! which is what keeps `Record-Route` and `Contact` right while that is true.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use bytes::Bytes;
use sipx_transport::TransportKind;

/// What is wrong with a declared listener.
///
/// Every variant is a configuration a node must refuse to start on rather than run with. A node
/// that advertised an address nobody can reach would look healthy and answer nothing, which is the
/// failure this whole module exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ListenerError {
    /// An advertised address with no host in it.
    #[error("a listener must advertise a host")]
    EmptyHost,
    /// `0.0.0.0` or `::` — an address that means "everywhere" to us and nothing to a peer.
    #[error(
        "`{0}` cannot be advertised: an unspecified address is where to listen, not where to be reached"
    )]
    UnspecifiedHost(String),
    /// Port `0` — a request for whatever port is free, which is not an address either.
    #[error("port 0 cannot be advertised: it names no port")]
    ZeroPort,
    /// Neither a host nor a `host:port`.
    #[error("`{0}` is not a host or a host:port")]
    Malformed(String),
    /// Two listeners claim one transport, so a message arriving on it names two addresses.
    #[error("{0} is declared twice: a message arriving on it would have two addresses to give")]
    DuplicateTransport(&'static str),
    /// A node with no listener answers nothing.
    #[error("a node must declare at least one listener")]
    NoListener,
    /// The cleartext pair disagrees about an address sipx binds once for both.
    #[error(
        "the UDP and TCP listeners disagree about the {0}: sipx binds one endpoint for the cleartext pair"
    )]
    ClearTextDisagreement(&'static str),
    /// A transport this node cannot express a listener for yet.
    #[error("a {0} listener is not supported here yet")]
    UnsupportedTransport(&'static str),
}

/// The address a listener tells peers to use.
///
/// A host and, optionally, a port. The port is optional because a listener that advertises the port
/// it bound is the ordinary case and repeating it invites the two to drift apart; an omitted port
/// means "the one I bound", never "the default for the scheme".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertised {
    host: String,
    port: Option<u16>,
}

impl Advertised {
    /// An advertised host, and the port to advertise with it.
    ///
    /// # Errors
    ///
    /// Refuses an empty host, an unspecified address and port zero — the three values that parse
    /// as an address and are not one.
    pub fn new(host: &str, port: Option<u16>) -> Result<Self, ListenerError> {
        let host = host.trim().trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(ListenerError::EmptyHost);
        }
        if let Ok(ip) = host.parse::<IpAddr>()
            && ip.is_unspecified()
        {
            return Err(ListenerError::UnspecifiedHost(host.to_owned()));
        }
        if port == Some(0) {
            return Err(ListenerError::ZeroPort);
        }
        Ok(Self {
            host: host.to_owned(),
            port,
        })
    }

    /// Parse `host`, `host:port` or `[v6]:port`.
    ///
    /// A bare IPv6 literal needs no brackets here — `2001:db8::1` is unambiguous because it parses
    /// as an address, and only the bracketed form can carry a port (RFC 3261 §19.1.1's
    /// `IPv6reference`).
    ///
    /// # Errors
    ///
    /// [`ListenerError::Malformed`] for text that is neither, plus everything [`Self::new`]
    /// refuses.
    pub fn parse(text: &str) -> Result<Self, ListenerError> {
        let text = text.trim();
        let malformed = || ListenerError::Malformed(text.to_owned());

        if let Some(rest) = text.strip_prefix('[') {
            let Some((host, tail)) = rest.split_once(']') else {
                return Err(malformed());
            };
            let port = match tail {
                "" => None,
                _ => Some(
                    tail.strip_prefix(':')
                        .ok_or_else(malformed)?
                        .parse()
                        .map_err(|_| malformed())?,
                ),
            };
            return Self::new(host, port);
        }
        if text.parse::<IpAddr>().is_ok() {
            return Self::new(text, None);
        }
        match text.rsplit_once(':') {
            Some((host, port)) => Self::new(host, Some(port.parse().map_err(|_| malformed())?)),
            None => Self::new(text, None),
        }
    }

    /// The host, as written.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, if this address names one.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The host as it must be spelled inside a URI: an IPv6 literal in brackets (§19.1.1).
    ///
    /// Without the brackets the colons of the address are indistinguishable from the colon before
    /// the port, and every parser downstream reads a different host.
    #[must_use]
    pub fn host_in_uri(&self) -> String {
        if self.host.parse::<Ipv6Addr>().is_ok() {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }
}

/// One listener: an address it binds, and the address it advertises.
///
/// The two are declared independently and neither is derived from the other. That is the whole
/// point: on a host-networked cloud node the bind address is private and the advertised one is
/// public, and a configuration that could only express "both the same" cannot describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    transport: TransportKind,
    bind: SocketAddr,
    advertise: Advertised,
}

impl Listener {
    /// A listener that binds one address and advertises another.
    ///
    /// # Errors
    ///
    /// [`ListenerError::UnsupportedTransport`] for a transport this node cannot yet run a listener
    /// on. `WS`/`WSS` are the kernel's (it has both) but nothing here decides for them yet, and a
    /// listener that could be declared and not served would be worse than one that cannot.
    pub fn new(
        transport: TransportKind,
        bind: SocketAddr,
        advertise: Advertised,
    ) -> Result<Self, ListenerError> {
        match transport {
            TransportKind::Udp | TransportKind::Tcp | TransportKind::Tls => {}
            other => return Err(ListenerError::UnsupportedTransport(other.as_str())),
        }
        Ok(Self {
            transport,
            bind,
            advertise,
        })
    }

    /// A listener that advertises the address it binds.
    ///
    /// # Errors
    ///
    /// The bind address is the advertised address here, so an unspecified one is refused: a node
    /// bound to `0.0.0.0` has nothing to advertise and must be told what it is reached on.
    pub fn bound(transport: TransportKind, bind: SocketAddr) -> Result<Self, ListenerError> {
        let advertise = Advertised::new(&bind.ip().to_string(), Some(bind.port()))?;
        Self::new(transport, bind, advertise)
    }

    /// The same listener, advertising a different address.
    #[must_use]
    pub fn advertising(mut self, advertise: Advertised) -> Self {
        self.advertise = advertise;
        self
    }

    /// How messages arrive on it.
    #[must_use]
    pub fn transport(&self) -> TransportKind {
        self.transport
    }

    /// Where it binds.
    #[must_use]
    pub fn bind(&self) -> SocketAddr {
        self.bind
    }

    /// What it advertises.
    #[must_use]
    pub fn advertise(&self) -> &Advertised {
        &self.advertise
    }

    /// The advertised host.
    #[must_use]
    pub fn advertised_host(&self) -> &str {
        self.advertise.host()
    }

    /// The advertised port: the declared one, or the port actually bound.
    #[must_use]
    pub fn advertised_port(&self) -> u16 {
        self.advertise.port().unwrap_or_else(|| self.bind.port())
    }

    /// The `Via` sent-by for a message this listener puts on the wire (RFC 3261 §18.1.1).
    #[must_use]
    pub fn sent_by(&self) -> String {
        format!(
            "{}:{}",
            self.advertise.host_in_uri(),
            self.advertised_port()
        )
    }

    /// The `Record-Route` URI this node inserts for a request received here (§16.6 step 4).
    ///
    /// Bracketed and carrying `;lr`, so the proxy engine can append its affinity token inside the
    /// angle brackets where a `Route` derived from it will still carry it (§12.1.1).
    #[must_use]
    pub fn record_route_uri(&self) -> Bytes {
        Bytes::from(format!(
            "<sip:{}{};lr>",
            self.sent_by(),
            self.transport_param()
        ))
    }

    /// A `Contact` naming this node on this listener (§8.1.1.8).
    ///
    /// The address a peer is being told to send its next request to, which is the advertised one
    /// for exactly the same reason a `Via` sent-by is.
    #[must_use]
    pub fn contact_uri(&self, user: Option<&str>) -> Bytes {
        let user = user.map_or_else(String::new, |user| format!("{user}@"));
        Bytes::from(format!(
            "<sip:{user}{}{}>",
            self.sent_by(),
            self.transport_param()
        ))
    }

    /// The URI transport parameter this listener needs, if any (§19.1.1).
    ///
    /// Empty for UDP, which is what a `sip:` URI with no parameter already means (RFC 3263 §4.1) —
    /// spelling it out would add bytes to every `Record-Route` and change nothing.
    ///
    /// The scheme stays `sip:` on the TLS listener rather than becoming `sips:`. A `sips:` URI is a
    /// claim about the whole remaining path, not about this hop, and settling that is a policy
    /// question this story does not own; `;transport=tls` is §19.1.1's way to say "come back to me
    /// over TLS", and the kernel's RFC 3263 resolution honours it.
    fn transport_param(&self) -> &'static str {
        match self.transport {
            TransportKind::Udp | TransportKind::Ws | TransportKind::Wss => "",
            TransportKind::Tcp => ";transport=tcp",
            TransportKind::Tls => ";transport=tls",
        }
    }
}

/// Every listener a node runs, validated as a set.
///
/// A set rather than a list because the questions asked of it are set-shaped: which listener did
/// this message arrive on, and what are all the addresses this node answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listeners {
    listeners: Vec<Listener>,
}

impl Listeners {
    /// Validate a declared set.
    ///
    /// # Errors
    ///
    /// A set with no listener, one that claims a transport twice, or a cleartext pair that
    /// disagrees about the address sipx binds once for both.
    pub fn new(listeners: impl IntoIterator<Item = Listener>) -> Result<Self, ListenerError> {
        let listeners: Vec<Listener> = listeners.into_iter().collect();
        if listeners.is_empty() {
            return Err(ListenerError::NoListener);
        }
        for (index, listener) in listeners.iter().enumerate() {
            if listeners
                .iter()
                .take(index)
                .any(|earlier| earlier.transport() == listener.transport())
            {
                return Err(ListenerError::DuplicateTransport(
                    listener.transport().as_str(),
                ));
            }
        }

        let set = Self { listeners };
        // sipx listens for TCP on the port it bound for UDP (`Config::tcp`), and stamps one
        // sent-by into both. A configuration that asked for two would be silently served as one,
        // and silently is the part that is unacceptable.
        if let (Some(udp), Some(tcp)) = (
            set.receiving(TransportKind::Udp),
            set.receiving(TransportKind::Tcp),
        ) {
            if udp.bind() != tcp.bind() {
                return Err(ListenerError::ClearTextDisagreement("bind address"));
            }
            if udp.advertise() != tcp.advertise() {
                return Err(ListenerError::ClearTextDisagreement("advertised address"));
            }
        }
        Ok(set)
    }

    /// The listener a message that arrived over `transport` was received on.
    ///
    /// The whole decision input: with one endpoint per node the transport identifies the listener,
    /// and it is what the kernel reports on every arrival.
    #[must_use]
    pub fn receiving(&self, transport: TransportKind) -> Option<&Listener> {
        self.listeners
            .iter()
            .find(|listener| listener.transport() == transport)
    }

    /// Every declared listener.
    pub fn iter(&self) -> impl Iterator<Item = &Listener> {
        self.listeners.iter()
    }

    /// The listener sipx's one endpoint binds, if the set declares one.
    ///
    /// UDP first, then TCP: the pair share an endpoint and [`Self::new`] has already made them
    /// agree, so either answers for both.
    #[must_use]
    pub fn cleartext(&self) -> Option<&Listener> {
        self.receiving(TransportKind::Udp)
            .or_else(|| self.receiving(TransportKind::Tcp))
    }

    /// The endpoint configuration sipx is asked to bind, or `None` for a set with no cleartext
    /// listener in it.
    ///
    /// The mapping onto the kernel's own separation of the two addresses: `bind` is what the socket
    /// gets, `sent_by`/`sent_by_port` is what goes into a `Via` sipx writes. Pure — it builds a
    /// value and binds nothing, so the harness can assert on it.
    ///
    /// The TLS listener is not bound here: sipx wants a server identity with it
    /// (`Config::tls_server`), and certificate material is configuration `DP-1` owns. Its
    /// advertised address is decided all the same — `Record-Route` and `Contact` for a TLS arrival
    /// are right the moment the listener is served.
    #[must_use]
    pub fn endpoint_config(&self) -> Option<sipx_transport::Config> {
        let cleartext = self.cleartext()?;
        let mut config = sipx_transport::Config::new(cleartext.bind());
        cleartext.advertised_host().clone_into(&mut config.sent_by);
        config.sent_by_port = Some(cleartext.advertised_port());
        config.tcp = self.receiving(TransportKind::Tcp).is_some();
        Some(config)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn addr(text: &str) -> SocketAddr {
        text.parse().expect("an address")
    }

    fn listener(transport: TransportKind, bind: &str, advertise: &str) -> Listener {
        Listener::new(
            transport,
            addr(bind),
            Advertised::parse(advertise).expect("an advertised address"),
        )
        .expect("a supported transport")
    }

    #[test]
    fn an_advertised_address_parses_in_every_shape_a_deployment_writes_it() {
        let bare = Advertised::parse("edge.example").unwrap();
        assert_eq!((bare.host(), bare.port()), ("edge.example", None));

        let with_port = Advertised::parse("edge.example:5060").unwrap();
        assert_eq!(
            (with_port.host(), with_port.port()),
            ("edge.example", Some(5060))
        );

        // A bare IPv6 literal is unambiguous; only the bracketed form can carry a port.
        let v6 = Advertised::parse("2001:db8::1").unwrap();
        assert_eq!((v6.host(), v6.port()), ("2001:db8::1", None));
        let v6_port = Advertised::parse("[2001:db8::1]:5060").unwrap();
        assert_eq!(
            (v6_port.host(), v6_port.port()),
            ("2001:db8::1", Some(5060))
        );
        assert_eq!(v6_port.host_in_uri(), "[2001:db8::1]");
    }

    #[test]
    fn an_address_that_is_not_one_is_refused_rather_than_advertised() {
        // "Everywhere" is an answer to where to listen and not to where to be reached.
        assert_eq!(
            Advertised::parse("0.0.0.0:5060"),
            Err(ListenerError::UnspecifiedHost("0.0.0.0".to_owned()))
        );
        assert_eq!(
            Advertised::parse("[::]:5060"),
            Err(ListenerError::UnspecifiedHost("::".to_owned()))
        );
        assert_eq!(
            Advertised::parse("edge.example:0"),
            Err(ListenerError::ZeroPort)
        );
        assert_eq!(Advertised::parse(""), Err(ListenerError::EmptyHost));
        assert!(matches!(
            Advertised::parse("edge.example:http"),
            Err(ListenerError::Malformed(_))
        ));
    }

    #[test]
    fn binding_an_unspecified_address_is_fine_but_advertising_it_is_not() {
        // The ordinary cloud shape: bind everything, be reached at one address.
        let listener = listener(TransportKind::Udp, "0.0.0.0:5060", "203.0.113.9:5060");
        assert_eq!(listener.advertised_host(), "203.0.113.9");
        assert!(matches!(
            Listener::bound(TransportKind::Udp, addr("0.0.0.0:5060")),
            Err(ListenerError::UnspecifiedHost(_))
        ));
    }

    #[test]
    fn an_omitted_advertised_port_is_the_port_that_was_bound() {
        // Never the scheme's default: a listener on 5080 that advertised 5060 because its port was
        // not repeated would be unreachable, and unreachable in a way nothing in the config shows.
        let listener = listener(TransportKind::Udp, "10.0.0.7:5080", "203.0.113.9");
        assert_eq!(listener.advertised_port(), 5080);
        assert_eq!(listener.sent_by(), "203.0.113.9:5080");
    }

    #[test]
    fn the_uri_of_a_listener_names_the_transport_to_come_back_on() {
        assert_eq!(
            listener(TransportKind::Udp, "10.0.0.7:5060", "203.0.113.9:5060").record_route_uri(),
            Bytes::from_static(b"<sip:203.0.113.9:5060;lr>"),
        );
        assert_eq!(
            listener(TransportKind::Tcp, "10.0.0.7:5060", "203.0.113.9:5060").record_route_uri(),
            Bytes::from_static(b"<sip:203.0.113.9:5060;transport=tcp;lr>"),
        );
        assert_eq!(
            listener(TransportKind::Tls, "10.0.0.7:5061", "203.0.113.9:5061").record_route_uri(),
            Bytes::from_static(b"<sip:203.0.113.9:5061;transport=tls;lr>"),
        );
        assert_eq!(
            listener(TransportKind::Tls, "10.0.0.7:5061", "203.0.113.9:5061")
                .contact_uri(Some("alice")),
            Bytes::from_static(b"<sip:alice@203.0.113.9:5061;transport=tls>"),
        );
    }

    #[test]
    fn a_transport_with_no_decision_behind_it_cannot_be_declared() {
        assert_eq!(
            Listener::bound(TransportKind::Ws, addr("10.0.0.7:5080")),
            Err(ListenerError::UnsupportedTransport("WS"))
        );
    }

    #[test]
    fn a_set_refuses_what_it_could_not_serve_honestly() {
        assert_eq!(
            Listeners::new(Vec::<Listener>::new()),
            Err(ListenerError::NoListener)
        );
        assert_eq!(
            Listeners::new([
                listener(TransportKind::Udp, "10.0.0.7:5060", "203.0.113.9:5060"),
                listener(TransportKind::Udp, "10.0.0.7:5062", "203.0.113.9:5062"),
            ]),
            Err(ListenerError::DuplicateTransport("UDP"))
        );
        // sipx binds one endpoint for the cleartext pair, so two answers to "what do I advertise"
        // would be served as one — silently, which is the part that is unacceptable.
        assert_eq!(
            Listeners::new([
                listener(TransportKind::Udp, "10.0.0.7:5060", "203.0.113.9:5060"),
                listener(TransportKind::Tcp, "10.0.0.7:5060", "198.51.100.4:5060"),
            ]),
            Err(ListenerError::ClearTextDisagreement("advertised address"))
        );
    }

    #[test]
    fn the_endpoint_binds_one_address_and_advertises_the_other() {
        let set = Listeners::new([
            listener(TransportKind::Udp, "10.0.0.7:5060", "203.0.113.9:5060"),
            listener(TransportKind::Tls, "10.0.0.7:5061", "203.0.113.9:5061"),
        ])
        .unwrap();
        let endpoint = set.endpoint_config().expect("a cleartext listener");
        assert_eq!(endpoint.bind, addr("10.0.0.7:5060"));
        assert_eq!(endpoint.sent_by, "203.0.113.9");
        assert_eq!(endpoint.sent_by_port, Some(5060));
        // Declared listeners only: a TCP port bound because it was on by default is a port nobody
        // asked for and nothing decides for.
        assert!(!endpoint.tcp);
    }

    /// A refusal names the address and says what was wrong with it.
    ///
    /// This one is read by an operator at 3am with a node that will not start; "invalid
    /// configuration" would send them to the source.
    #[test]
    fn a_refusal_says_what_was_wrong_with_the_address() {
        assert_eq!(
            ListenerError::UnspecifiedHost("0.0.0.0".to_owned()).to_string(),
            "`0.0.0.0` cannot be advertised: an unspecified address is where to listen, not where to be reached"
        );
    }
}
