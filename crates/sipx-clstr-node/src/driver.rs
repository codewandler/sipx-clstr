//! The node: a real socket, the real registrar, the real forwarding core.
//!
//! This is [proxy-transaction-driver](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/proxy-transaction-driver.md)
//! made real. `PX-2` settled the shape — build on `sipx_transport::Handle` rather than on a socket
//! loop of our own — and this is that shape with sockets under it: one task per proxied request
//! owning its response context, branches as `Handle::send` calls, and nothing shared on the
//! signalling path because there is nothing to share.
//!
//! Everything that decides anything lives below, in the sans-IO crates. What is here is the part
//! that cannot be tested without a network, which is why it is small enough to read in one sitting.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use sipx_clstr_proxy::{
    BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig, ResponseContext,
    targets_from_lookup,
};
use sipx_clstr_registrar::{
    Admission, CanonicalAor, EdgeContext, InMemoryCredentials, InMemoryStore, LocationStore,
    TenantAuth, TenantPolicy, Timestamp, admit, apply,
};
use sipx_sip::{
    HeaderName, Method, Request, Response, ResponseBuilder, StatusCode, TransactionKey, Uri,
};
use sipx_transport::{Handle, Incoming, Target, TransportKind};

use crate::listen::{Advertised, Listener, ListenerError, Listeners};

/// How the node is configured.
///
/// Deliberately minimal and **provisional**: `DP-1` owns the real schema and replaces this rather
/// than extending it, so nothing should grow to depend on its shape.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// What this node binds, and what it advertises on each of them (`DP-5`).
    pub listeners: Listeners,
    /// The tenant every registration on this node belongs to.
    ///
    /// One tenant per node is the M1 simplification. The tenant never comes from the message —
    /// a registrar that read its tenant from a URI would let a caller choose whose bindings to write.
    pub tenant: String,
    /// How the tenant authenticates, or `None` for an open tenant (`RG-2`, registrar-auth §3 A1).
    ///
    /// Open by default. A default that quietly required credentials would make a node that answers
    /// nothing look like a node that is up, and a default that quietly invented a realm would put a
    /// protection space in the deployment that nobody configured.
    pub auth: Option<AuthConfig>,
    /// Where registrations live (`RG-12`).
    ///
    /// In-process by default, which is the only thing a single node needs. Two nodes that must agree
    /// about a registration need [`StoreChoice::Postgres`] — an in-process store on each of two
    /// nodes is not a cluster, it is two registrars each answering only for whoever happened to
    /// reach it.
    pub store: StoreChoice,
}

/// Which location service backs this node (`RG-12`, location-service §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StoreChoice {
    /// Process-local. Bindings die with the process, and no peer can see them.
    #[default]
    InMemory,
    /// The shared `PostgreSQL` location service (`RG-4`).
    ///
    /// The DSN is the *resolved* value: `cluster-config` §8 V9 keeps only a `dsnRef` in the
    /// document, and resolving it is IO, which is this layer's job and not the loader's.
    Postgres { dsn: String },
}

impl StoreChoice {
    /// The backend's name, and never its DSN.
    ///
    /// `cluster-config` §8 V9 keeps credentials out of the document by reference; a log line that
    /// debug-printed the *resolved* value would put them straight back, into the one artefact most
    /// likely to be copied into an issue.
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match self {
            StoreChoice::InMemory => "in-memory",
            StoreChoice::Postgres { .. } => "postgres",
        }
    }
}

/// A tenant's digest policy: the realm it challenges in, its nonce key, and its credentials.
///
/// Provisional alongside [`NodeConfig`] — `DP-1` owns the real schema, and `RG-7` owns arriving at
/// the credentials from a store rather than from a literal.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// The protection space (registrar-auth §3 A3).
    pub realm: String,
    /// The nonce key. Stable across restarts, or in-flight nonces do not survive one — clients
    /// recover through `stale=true`, so the cost of an unstable one is a round trip, not a login.
    pub secret: [u8; 32],
    /// Who may register.
    pub credentials: InMemoryCredentials,
}

impl NodeConfig {
    /// A node with a declared set of listeners.
    #[must_use]
    pub fn listening(listeners: Listeners) -> Self {
        Self {
            listeners,
            tenant: "default".to_owned(),
            auth: None,
            store: StoreChoice::InMemory,
        }
    }

    /// A node on one address, over UDP and TCP, advertising the address it binds.
    ///
    /// # Errors
    ///
    /// An unspecified bind address (`0.0.0.0`, `::`) has nothing to advertise, and a node that
    /// advertised it would put "everywhere" in its `Record-Route` — a dialog whose second request
    /// goes nowhere. Such a node must be told what it is reached on: [`Self::advertising`].
    pub fn new(listen: SocketAddr) -> Result<Self, ListenerError> {
        Ok(Self::listening(Listeners::new([
            Listener::bound(TransportKind::Udp, listen)?,
            Listener::bound(TransportKind::Tcp, listen)?,
        ])?))
    }

    /// A node on one address, over UDP and TCP, reached at `advertise`.
    ///
    /// The listen-private/advertise-public shape (`DP-5`): the two are declared independently and
    /// neither is derived from the other.
    ///
    /// # Errors
    ///
    /// Whatever is wrong with the advertised address ([`Advertised::parse`]).
    pub fn advertising(listen: SocketAddr, advertise: &str) -> Result<Self, ListenerError> {
        let advertise = Advertised::parse(advertise)?;
        Ok(Self::listening(Listeners::new([
            Listener::new(TransportKind::Udp, listen, advertise.clone())?,
            Listener::new(TransportKind::Tcp, listen, advertise)?,
        ])?))
    }

    /// The tenant policy this configuration describes, ready to decide with.
    fn tenant_auth(&self) -> TenantAuth {
        match &self.auth {
            Some(auth) => TenantAuth::required(&self.tenant, &auth.realm, auth.secret),
            None => TenantAuth::open(&self.tenant),
        }
    }

    fn credentials(&self) -> InMemoryCredentials {
        self.auth
            .as_ref()
            .map(|auth| auth.credentials.clone())
            .unwrap_or_default()
    }
}

/// What stops a node from starting.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// The listener could not be bound.
    #[error(transparent)]
    Transport(#[from] sipx_transport::Error),
    /// A listener was declared that cannot be served as declared.
    #[error(transparent)]
    Listener(#[from] ListenerError),
    /// Only a TLS listener was declared, and TLS needs a server identity this node cannot yet be
    /// given (`DP-1` owns certificate configuration).
    #[error("no UDP or TCP listener is declared, and a TLS-only node cannot be served yet")]
    NoCleartextListener,
    /// The configured location service could not be reached (`RG-12`).
    ///
    /// Refusing to start is the only correct answer. A registrar that fell back to an in-process
    /// store would come up healthy, answer `200` to every REGISTER, and serve bindings no peer can
    /// see — which is worse than not starting, because nothing would say so.
    #[error("the configured location store could not be reached: {0}")]
    LocationStoreUnreachable(String),
}

/// Open the location service this node was configured for (`RG-12`).
///
/// The `postgres` feature is what compiles the backend in; a node asking for it in a build without
/// it is a configuration that cannot be honoured, and saying so is better than quietly using a
/// different store than the one that was asked for.
/// # Errors
///
/// [`NodeError::LocationStoreUnreachable`] when the configured store cannot be opened.
pub fn open_store(choice: &StoreChoice) -> Result<Arc<dyn LocationStore + Send + Sync>, NodeError> {
    match choice {
        StoreChoice::InMemory => Ok(Arc::new(InMemoryStore::new())),
        #[cfg(feature = "postgres")]
        StoreChoice::Postgres { dsn } => {
            // Connect here, at startup, rather than lazily on the first REGISTER. A registrar that
            // discovers its store is gone while answering a request has already told the client it
            // was up.
            // Connected on a **plain thread with no tokio context at all**, not merely inside
            // `block_in_place`. The synchronous `postgres` client builds its own runtime to drive the
            // connection, and building one while tokio's context is visible does not fail loudly —
            // it produced `error communicating with the server`, a connection that never got driven.
            // Handing the work to a thread tokio has never touched is the only arrangement that both
            // connects and keeps the client's runtime its own.
            let dsn = dsn.clone();
            let store =
                std::thread::spawn(move || crate::postgres_store::PostgresStore::connect(&dsn))
                    .join()
                    .map_err(|_| {
                        NodeError::LocationStoreUnreachable(
                            "the connecting thread panicked".to_owned(),
                        )
                    })?
                    .map_err(|error| NodeError::LocationStoreUnreachable(error.to_string()))?;
            Ok(Arc::new(crate::blocking_store::BlockingStore::new(store)))
        }
        #[cfg(not(feature = "postgres"))]
        StoreChoice::Postgres { .. } => Err(NodeError::LocationStoreUnreachable(
            "this binary was built without the `postgres` feature".to_owned(),
        )),
    }
}

/// Run the node until the process is asked to stop.
///
/// # Errors
///
/// Fails if the listener cannot be bound — the one error worth refusing to start over, since a node
/// that silently listened nowhere would look healthy and answer nothing — and if the declared
/// listeners cannot be served as declared.
pub async fn run(config: NodeConfig) -> Result<(), NodeError> {
    let advertised = config
        .listeners
        .cleartext()
        .ok_or(NodeError::NoCleartextListener)?
        .sent_by();
    let endpoint = config
        .listeners
        .endpoint_config()
        .ok_or(NodeError::NoCleartextListener)?;
    let (handle, mut incoming) = sipx_transport::bind(endpoint).await?;

    // Opened **before** the announcement, for the same reason the announcement comes after the bind.
    // It was the other way round for one commit, and the failure was instructive: the node printed
    // `listening on`, a script waiting for that line proceeded, and the node then exited because the
    // store was unreachable. Everything that can refuse to start must refuse before anything says
    // the node started.
    let store: Arc<dyn LocationStore + Send + Sync> = open_store(&config.store)?;

    // Announced on stdout **after** the bind and after every other startup refusal, so a script can
    // wait for this line instead of sleeping and hoping. Printing it before would make a failed
    // start look like a successful one — which it did, until a test of the failure path noticed the
    // node saying "listening" and then dying.
    //
    // Both addresses, because they are allowed to differ (`DP-5`) and an operator debugging "the
    // phone registers but nothing rings" needs to see which one went into the messages.
    println!("listening on {}", handle.local_addr());
    println!("advertising {advertised}");
    tracing::info!(
        listen = %handle.local_addr(),
        %advertised,
        tenant = %config.tenant,
        store = config.store.describe(),
        // Named for the same reason `RG-12` named the store: an operator reading one line should be
        // able to tell an open tenant from an authenticated one. Today it is always `open`, which is
        // exactly why it is worth printing.
        auth = if config.auth.is_some() { "required" } else { "open" },
        "node listening"
    );
    let policy = TenantPolicy::default();
    // One authenticator for the node, because it holds the replay window: a per-request one would
    // forget every nonce-count the moment it was created, which is a replay window that never says
    // no. `std::sync::Mutex` rather than tokio's — `decide` is a hash and a lookup, and it is never
    // held across an await.
    let auth = Arc::new(Mutex::new(config.tenant_auth()));
    let credentials = Arc::new(config.credentials());
    report_transactions_in_flight(handle.clone());

    // The cookie key is the node's, not the request's: it must be the same for every message this
    // process forwards, or a loop through this node would not be detectable across two of them.
    let cookie_key = cookie_key();

    while let Some(arrival) = incoming.recv().await {
        let handle = handle.clone();
        let store = Arc::clone(&store);
        let config = config.clone();
        let auth = Arc::clone(&auth);
        let credentials = Arc::clone(&credentials);
        let cookie_key = cookie_key.clone();

        // One task per arrival. The accept loop must never do work inline: it is the single consumer
        // of the incoming channel, and the kernel delivers into that channel with `try_send`, so a
        // blocked loop drops requests silently (sipx `T-19`).
        tokio::spawn(async move {
            // Which listener it arrived on decides which address goes back into it (`DP-5`). Built
            // per arrival because that answer is per arrival: a node that advertises one address on
            // UDP and another on TLS has to answer each on its own terms.
            let receiving = config.listeners.receiving(arrival.transport);
            if receiving.is_none() {
                // Only reachable if sipx delivered on a transport nobody declared. The request is
                // still served — from the cleartext listener — because an answer from the wrong
                // address beats no answer at all.
                tracing::warn!(
                    transport = arrival.transport.as_str(),
                    "a request arrived on an undeclared transport"
                );
            }
            let proxy = proxy_config_keyed(&config, receiving, cookie_key);
            let edge = Edge {
                store: store.as_ref(),
                policy: &policy,
                config: &config,
                proxy: &proxy,
                auth: &auth,
                credentials: &credentials,
            };
            if let Err(error) = serve(&handle, &edge, arrival).await {
                tracing::warn!(%error, "request handling failed");
            }
        });
    }
    Ok(())
}

/// Report how many transactions the kernel is holding, whenever that number changes.
///
/// A proxy that leaks one transaction per call is a slow, quiet outage: nothing looks wrong until
/// the process does. This is the cheapest instrument that would notice, and it is `DP-3`'s gauge in
/// embryo.
///
/// **On change, not on a schedule.** A number logged every second is noise nobody reads; a number
/// logged when it moves is a record of what the node did. It also has to be sampled rather than
/// emitted per request: the count that matters is the one *after* the last request, and a per-request
/// line can never show that, which is exactly how the first version of this failed to observe the
/// store draining at all.
fn report_transactions_in_flight(handle: Handle) {
    tokio::spawn(async move {
        let mut previous = usize::MAX;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let Ok(outstanding) = handle.outstanding().await else {
                // The endpoint is gone, and so is the reason to keep counting.
                return;
            };
            if outstanding != previous {
                previous = outstanding;
                tracing::info!(outstanding, "transactions in flight");
            }
        }
    });
}

/// The proxy this node runs for a request that arrived on `receiving`.
///
/// The one place the advertised address becomes protocol (`DP-5`). `Record-Route` is the receiving
/// listener's, because that is the address the peer just reached us on and the one its mid-dialog
/// requests must come back to — and the proxy engine derives its own `Via` sent-by from the same
/// URI, so the two agree by construction rather than by discipline.
///
/// The identity **set** is every listener's, not only the receiving one: any edge recognizes any
/// edge's `Route` (proxy-behavior §5), and a node that only knew the address a request came in on
/// would forward its own `Record-Route` straight back out again.
///
/// Public because it is a decision and decisions are testable without a socket (AGENTS.md #2).
#[must_use]
pub fn proxy_config(config: &NodeConfig, receiving: Option<&Listener>) -> ProxyConfig {
    proxy_config_keyed(config, receiving, cookie_key())
}

fn proxy_config_keyed(
    config: &NodeConfig,
    receiving: Option<&Listener>,
    cookie_key: CookieKey,
) -> ProxyConfig {
    // A validated set is never empty, so `speaking_for` is only ever `None` for a set that could
    // not exist — expressed as a fallback rather than an `unwrap`, because this is on the path a
    // network message reaches (AGENTS.md #3).
    let speaking_for = receiving
        .or_else(|| config.listeners.cleartext())
        .or_else(|| config.listeners.iter().next());
    let record_route = speaking_for
        .map(Listener::record_route_uri)
        .unwrap_or_default();
    let host = speaking_for.map_or("", Listener::advertised_host);

    let mut proxy = ProxyConfig::new(host, record_route, cookie_key);
    // The node answers to every address it advertises, on any port: a client that resolved a port
    // differently, or reached us over another transport, is still talking to this node.
    proxy.identities.clear();
    for listener in config.listeners.iter() {
        let identity = sipx_clstr_proxy::EdgeIdentity::host(listener.advertised_host());
        if !proxy.identities.contains(&identity) {
            proxy.identities.push(identity);
        }
    }
    proxy
}

/// The loop-detection cookie key.
///
/// Per-process for now. `AF-6` owns distribution and rotation, and until it exists a key that
/// changes when the process restarts is honest about what it protects: loops *through this node*,
/// for as long as this node has been up.
fn cookie_key() -> CookieKey {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    CookieKey::new(Bytes::from(format!("sipx-clstr/{seed}")))
}

/// Everything one arrival is served against. A struct rather than seven arguments, which is what it
/// had become.
struct Edge<'a> {
    store: &'a (dyn LocationStore + Send + Sync),
    policy: &'a TenantPolicy,
    config: &'a NodeConfig,
    proxy: &'a ProxyConfig,
    auth: &'a Mutex<TenantAuth>,
    credentials: &'a InMemoryCredentials,
}

async fn serve(
    handle: &Handle,
    edge: &Edge<'_>,
    arrival: Incoming,
) -> Result<(), sipx_transport::Error> {
    match arrival.request.method {
        Method::Register => {
            let response = register(edge, &arrival);
            handle.respond(&arrival.key, response).await
        }
        // An ACK for a 2xx is a separate transaction end to end: forwarded, never answered.
        Method::Ack => forward_statelessly(handle, edge.store, edge.config, &arrival).await,
        _ => proxy_request(handle, edge.store, edge.config, edge.proxy, arrival).await,
    }
}

// ---------------------------------------------------------------------------------- registrar ---

fn register(edge: &Edge<'_>, arrival: &Incoming) -> Response {
    let context = EdgeContext {
        tenant: edge.config.tenant.clone(),
        received: Some(sipx_clstr_registrar::SourceAddr {
            transport: arrival.transport.as_str().to_owned(),
            ip: arrival.source.ip(),
            port: arrival.source.port(),
        }),
        ..EdgeContext::default()
    };

    // registrar-auth §2 — before processing, not inside it. The lock spans the decision only; the
    // store work below is outside it, so one slow registration cannot stall every other tenant user.
    let admission = {
        let mut auth = edge.auth.lock().unwrap_or_else(PoisonError::into_inner);
        admit(
            &arrival.request,
            &mut auth,
            edge.credentials,
            &context,
            now(),
        )
    };

    let cmd = match admission {
        Admission::Command(cmd) => *cmd,
        Admission::Challenge(challenge) => {
            let mut response = answer(
                &arrival.request,
                challenge.status,
                reason_for(challenge.status),
            );
            if let Ok(header) = sipx_sip::Header::build(challenge.header, challenge.value) {
                response.headers.push(header);
            }
            return response;
        }
        Admission::Reject(rejection) => {
            return answer(
                &arrival.request,
                rejection.status(),
                reason_for(rejection.status()),
            );
        }
    };

    let applied = apply(
        edge.store,
        &cmd,
        edge.policy,
        sipx_clstr_registrar::store::DEFAULT_CAS_RETRIES,
    );
    let status = applied.outcome.status();
    let mut response = answer(&arrival.request, status, reason_for(status));

    // §5.6 — the `200` enumerates the **complete** active set, with what each binding was granted.
    // A response that listed only what changed would leave a UA guessing about its other devices.
    if let Some(accepted) = applied.outcome.accepted() {
        for contact in &accepted.contacts {
            let value = format!(
                "<{}>;expires={}",
                String::from_utf8_lossy(&contact.contact),
                contact.expires
            );
            if let Ok(header) = sipx_sip::Header::build(HeaderName::Contact, value) {
                response.headers.push(header);
            }
        }
    }
    response
}

// -------------------------------------------------------------------------------------- proxy ---

async fn proxy_request(
    handle: &Handle,
    store: &(dyn LocationStore + Send + Sync),
    config: &NodeConfig,
    proxy: &ProxyConfig,
    arrival: Incoming,
) -> Result<(), sipx_transport::Error> {
    let mut context = ResponseContext::new(proxy.clone());
    let key = arrival.key.clone();
    let effects = context.on_input(ProxyInput::Upstream(Box::new(arrival.request.clone())));

    let mut pending = Vec::new();
    perform(
        handle,
        store,
        config,
        &key,
        &mut context,
        effects,
        &mut pending,
    )
    .await?;

    // Drive the branches. One `select` over their streams would be the shape at scale; with M1's
    // single fork group, awaiting them in order is the same thing and considerably easier to read.
    while let Some((branch, mut responses)) = pending.pop() {
        while let Some(event) = responses.next().await {
            let input = match event {
                sipx_sip::TuEvent::Response(response) => {
                    ProxyInput::BranchResponse(response, branch.clone())
                }
                sipx_sip::TuEvent::Timeout => {
                    // The driver design's mapping: a kernel timeout is a `408` from that branch.
                    match ResponseBuilder::to_request(
                        &arrival.request,
                        ok_status(408),
                        "Request Timeout",
                    ) {
                        Ok(builder) => {
                            ProxyInput::BranchResponse(Box::new(builder.build()), branch.clone())
                        }
                        Err(_) => ProxyInput::BranchTransportError(branch.clone()),
                    }
                }
                sipx_sip::TuEvent::TransportError => {
                    ProxyInput::BranchTransportError(branch.clone())
                }
                // An ACK on a client transaction is not ours; counted by being ignored explicitly
                // rather than by falling through a wildcard.
                sipx_sip::TuEvent::Ack(_) | sipx_sip::TuEvent::Request(_) => continue,
            };
            let effects = context.on_input(input);
            let mut more = Vec::new();
            perform(
                handle,
                store,
                config,
                &key,
                &mut context,
                effects,
                &mut more,
            )
            .await?;
            pending.extend(more);
            if context.is_finished() {
                return Ok(());
            }
        }
        // The stream ended with no final. A branch that vanishes is a branch that failed, and
        // leaving the context waiting on it forever is the failure mode this arm exists to prevent.
        if !context.is_finished() {
            let effects = context.on_input(ProxyInput::BranchTransportError(branch));
            let mut more = Vec::new();
            perform(
                handle,
                store,
                config,
                &key,
                &mut context,
                effects,
                &mut more,
            )
            .await?;
            pending.extend(more);
        }
    }
    Ok(())
}

async fn perform(
    handle: &Handle,
    store: &(dyn LocationStore + Send + Sync),
    config: &NodeConfig,
    key: &TransactionKey,
    context: &mut ResponseContext,
    effects: Vec<ProxyEffect>,
    pending: &mut Vec<(BranchId, sipx_transport::Responses)>,
) -> Result<(), sipx_transport::Error> {
    for effect in effects {
        match effect {
            ProxyEffect::ResolveTargets(query) => {
                let found = match CanonicalAor::parse(query.uri.clone()) {
                    Ok(aor) => store.lookup(&config.tenant, &aor, now()),
                    Err(_) => Vec::new(),
                };
                let targets = targets_from_lookup(&found);
                let more = context.on_input(ProxyInput::TargetsResolved(targets));
                Box::pin(perform(handle, store, config, key, context, more, pending)).await?;
            }
            ProxyEffect::Forward {
                branch,
                request,
                target,
            } => {
                let Some(destination) = destination_of(&target.uri) else {
                    tracing::warn!(target = %String::from_utf8_lossy(&target.uri), "unroutable");
                    continue;
                };
                let responses = handle.send(*request, destination).await?;
                pending.push((branch, responses));
            }
            ProxyEffect::Respond(response) => {
                handle.respond(key, *response).await?;
            }
            ProxyEffect::CancelBranch(branch) => {
                // `PX-6` mints the CANCEL; the kernel retransmits it. Recorded rather than
                // performed in M1, where nothing cancels: a silent gap here would be a call that
                // rings forever on the losing branch.
                tracing::info!(%branch, "branch cancellation is PX-6's, not yet wired to a socket");
            }
            ProxyEffect::AnswerCancel
            | ProxyEffect::SetTimer { .. }
            | ProxyEffect::ClearTimer { .. }
            | ProxyEffect::Terminate => {}
        }
    }
    Ok(())
}

/// Forward a request with no transaction of its own — an ACK for a 2xx.
async fn forward_statelessly(
    handle: &Handle,
    store: &(dyn LocationStore + Send + Sync),
    config: &NodeConfig,
    arrival: &Incoming,
) -> Result<(), sipx_transport::Error> {
    let Ok(aor) = CanonicalAor::from_uri(&arrival.request.uri) else {
        return Ok(());
    };
    let found = store.lookup(&config.tenant, &aor, now());
    let Some(first) = found.first() else {
        return Ok(());
    };
    let Some(destination) = destination_of(&first.contact) else {
        return Ok(());
    };
    handle
        .send_directly(arrival.request.clone(), destination)
        .await
}

// ------------------------------------------------------------------------------------ helpers ---

/// Where a contact URI actually is.
fn destination_of(contact: &Bytes) -> Option<Target> {
    let uri = Uri::parse(contact.clone()).ok()?;
    let host = uri.host()?;
    let port = uri.port().unwrap_or(5060);
    let addr: SocketAddr = match host {
        sipx_sip::Host::Ip(ip) => SocketAddr::new(*ip, port),
        // M1 forwards to registered contacts, which are addresses. A name here would need RFC 3263
        // resolution — `RT-1`'s work, and the story says so rather than pretending otherwise.
        sipx_sip::Host::Name(_) => return None,
    };
    Some(Target::new(addr, TransportKind::Udp))
}

fn answer(request: &Request, status: u16, reason: &str) -> Response {
    let code = ok_status(status);
    ResponseBuilder::to_request(request, code, reason.to_owned()).map_or_else(
        // Unreachable for anything the kernel handed us — it only delivers respondable requests —
        // and a bare response beats no response for the case that does not exist.
        |_| {
            ResponseBuilder::new(code, reason.to_owned())
                .map_or_else(|_| bare_error(), sipx_sip::ResponseBuilder::build)
        },
        sipx_sip::ResponseBuilder::build,
    )
}

/// The last resort: a `500` with nothing echoed.
///
/// Reachable only if the kernel cannot build a response from a status and a reason, which it always
/// can. Written without recursion so that an impossible case cannot become an infinite one.
fn bare_error() -> Response {
    let mut response = Response::new(ok_status(500), "Server Internal Error");
    response.set_body(Bytes::new());
    response
}

/// A status code, falling back to `500` for a value that is not one.
///
/// `500` rather than `200`: a status this code could not construct means the caller asked for
/// something impossible, and answering "fine" to that would be the worst available lie.
fn ok_status(status: u16) -> StatusCode {
    StatusCode::new(status)
        .or_else(|| StatusCode::new(500))
        .unwrap_or_else(|| ok_status(500))
}

fn reason_for(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        407 => "Proxy Authentication Required",
        420 => "Bad Extension",
        421 => "Extension Required",
        423 => "Interval Too Brief",
        500 => "Server Internal Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

/// The wall clock, as the registrar's `Timestamp`.
///
/// The **only** clock read in the workspace outside the harness, and it is here rather than in the
/// registrar precisely because reading a clock is a driver's job (AGENTS.md rule 2).
fn now() -> Timestamp {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    Timestamp::from_nanos(u64::try_from(since.as_nanos()).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_contact_with_an_address_resolves_to_a_destination() {
        let target =
            destination_of(&Bytes::from_static(b"sip:alice@192.0.2.7:5062")).expect("an address");
        assert_eq!(target.addr.port(), 5062);
    }

    #[test]
    fn a_contact_with_no_port_uses_the_default() {
        let target =
            destination_of(&Bytes::from_static(b"sip:alice@192.0.2.7")).expect("an address");
        assert_eq!(target.addr.port(), 5060);
    }

    #[test]
    fn a_contact_naming_a_host_is_not_routable_yet() {
        // M1 forwards to registered contacts, which are addresses. A name needs RFC 3263 resolution,
        // which is `RT-1`'s — and returning `None` here is what makes that a visible gap rather than
        // a call that fails somewhere further along.
        assert!(destination_of(&Bytes::from_static(b"sip:alice@phone.example")).is_none());
    }

    #[test]
    fn the_node_answers_to_every_address_it_advertises() {
        // proxy-behavior §5: any edge recognizes any edge's `Route`. Here that starts at home — a
        // node that advertised one address on UDP and another on TLS must recognize both, or a
        // mid-dialog request that came back on the other one would be forwarded straight out again.
        let config = NodeConfig::listening(
            Listeners::new([
                Listener::new(
                    TransportKind::Udp,
                    "10.0.0.7:5060".parse().unwrap(),
                    Advertised::parse("203.0.113.9:5060").unwrap(),
                )
                .unwrap(),
                Listener::new(
                    TransportKind::Tls,
                    "10.0.0.7:5061".parse().unwrap(),
                    Advertised::parse("edge-1.example:5061").unwrap(),
                )
                .unwrap(),
            ])
            .unwrap(),
        );
        let proxy = proxy_config(&config, None);
        let uri = |text: &str| Uri::parse(Bytes::copy_from_slice(text.as_bytes())).unwrap();
        assert!(proxy.is_ours(&uri("sip:203.0.113.9:5060;lr")));
        assert!(proxy.is_ours(&uri("sip:edge-1.example:5061;transport=tls;lr")));
        assert!(
            !proxy.is_ours(&uri("sip:10.0.0.7:5060;lr")),
            "the bound address is not an identity"
        );
    }
}
