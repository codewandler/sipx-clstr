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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use sipx_clstr_proxy::{
    AckRoute, BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig,
    ResponseContext, TokenVerdict, route_ack, targets_from_lookup,
};
use sipx_clstr_registrar::{
    Accepted, Admission, AuthOutcome, CanonicalAor, ContactValue, EdgeContext, InMemoryCredentials,
    InMemoryStore, LocationStore, Outcome, RegistrationAuthorizations, RegistrationPolicy,
    Rejection, RequestAuthority, TenantAuth, TenantPolicy, Timestamp, admit_audited, apply,
};
use sipx_sip::{
    HeaderName, Host, Method, Request, Response, ResponseBuilder, StatusCode, TransactionKey, Uri,
};
use sipx_transport::{Handle, Incoming, Responses, Target, TransportKind};
use tokio::task::JoinSet;

use crate::config::Capabilities;
use crate::listen::{Advertised, Listener, ListenerError, Listeners};

/// How the node is configured.
///
/// Deliberately minimal and **provisional**: `DP-1` owns the real schema and replaces this rather
/// than extending it, so nothing should grow to depend on its shape.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// What this node binds, and what it advertises on each of them (`DP-5`).
    pub listeners: Listeners,
    /// Which decision paths this node's declared roles wire up (`DP-13`, `cluster-config` §4 R3).
    ///
    /// The roles reached the projection and stopped there: they picked the listeners and the
    /// location store and were then dropped, so [`serve`] dispatched on method alone and every node
    /// answered every method. A node started as `inbound-proxy` therefore accepted and stored a
    /// REGISTER — a registrar nobody deployed, holding state no operator knows about.
    pub capabilities: Capabilities,
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
    /// The tenant's expiry and quota policy, as the document states it (`FC-4`).
    ///
    /// Was `TenantPolicy::default()` regardless of the document until `FC-4`, so a
    /// `maxBindingsPerAor: 3` loaded clean and the effective cap stayed 10.
    pub policy: TenantPolicy,
    /// The domains this tenant serves (location-service §5.1 S1/S5). Empty means any.
    ///
    /// Enforced since `FC-4`. It parsed into a struct field nothing read for a release, so a
    /// `REGISTER` for `alice@attacker.invalid` against `domains: [example.test]` was answered `200`.
    pub domains: Vec<String>,
    /// Which authenticated principals may write which canonical `AoRs` (`RG-18`, S4).
    ///
    /// Separate from digest credentials deliberately: authentication proves an identity; aliases,
    /// shared lines and administrators mean only this policy can decide what it may register.
    pub registration_authorizations: RegistrationAuthorizations,
    /// Where registrations live (`RG-12`).
    ///
    /// In-process by default, which is the only thing a single node needs. Two nodes that must agree
    /// about a registration need [`StoreChoice::Postgres`] — an in-process store on each of two
    /// nodes is not a cluster, it is two registrars each answering only for whoever happened to
    /// reach it.
    pub store: StoreChoice,
    /// How many proxied transactions this node will hold at once (`DP-11`).
    ///
    /// From `cluster.admission.maxInFlightTransactions`; see [`crate::config::AdmissionSpec`] for
    /// why the knob lives where it does, and [`AdmissionBound`] for what the bound does and does not
    /// cover.
    pub max_in_flight_transactions: usize,
    /// Timer C for INVITE branches (`PX-10`, proxy-behavior F11).
    ///
    /// From `cluster.timers.timerC`. It reaches the engine through [`proxy_config`] and nowhere
    /// else, so this field is the whole of the document→driver→engine path for it. Until `PX-10`
    /// there was no such path: `cluster.timers` was parsed, validated and projected onto
    /// `ProjectedConfig`, and then read by nobody, so every INVITE branch was guarded by the proxy
    /// crate's own default whatever the document said.
    pub timer_c: Duration,
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
/// `Debug` is hand-written and prints **no nonce secret** — see the impl below.
#[derive(Clone)]
pub struct AuthConfig {
    /// The protection space (registrar-auth §3 A3).
    pub realm: String,
    /// The nonce key. Stable across restarts, or in-flight nonces do not survive one — clients
    /// recover through `stale=true`, so the cost of an unstable one is a round trip, not a login.
    pub secret: [u8; 32],
    /// Who may register.
    pub credentials: InMemoryCredentials,
}

/// The realm, never the secret.
///
/// `NodeConfig` derives `Debug` and is the sort of thing that ends up in a startup dump, so a
/// derived impl here would print the 32-byte nonce key that every outstanding nonce is minted from
/// — the one value in this struct that lets a reader forge a challenge. `cluster-config` §8 V9
/// keeps it out of the *document* by reference; this keeps it out of the *output*, which is the
/// same argument at the other end. Same shape as `sipx-clstr-proxy`'s `CookieKey`.
impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("realm", &self.realm)
            .field("secret", &"<redacted>")
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl NodeConfig {
    /// A node with a declared set of listeners.
    #[must_use]
    pub fn listening(listeners: Listeners) -> Self {
        Self {
            listeners,
            // A node declared in code is the node these constructors have always produced: both
            // paths. Every node built from a **document** has this replaced by the wiring its
            // identity asks for, which is where the roles have to arrive (`DP-13`).
            capabilities: Capabilities::CALL_PATH,
            tenant: "default".to_owned(),
            auth: None,
            policy: TenantPolicy::default(),
            domains: Vec::new(),
            registration_authorizations: RegistrationAuthorizations::open(),
            store: StoreChoice::InMemory,
            max_in_flight_transactions: crate::config::DEFAULT_MAX_IN_FLIGHT_TRANSACTIONS,
            timer_c: sipx_clstr_proxy::DEFAULT_TIMER_C,
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

impl RegistrationPolicy for NodeConfig {
    fn serves(&self, tenant: &str, authority: &RequestAuthority) -> bool {
        if tenant != self.tenant {
            return false;
        }
        if self.domains.is_empty() {
            return true;
        }
        self.domains.iter().any(|served| {
            let raw = Bytes::copy_from_slice(served.as_bytes());
            let Ok((host, _configured_port)) = Host::parse_hostport(&raw) else {
                // A malformed configured domain can never turn into serve-any at runtime.
                return false;
            };
            host.equivalent(authority.host())
        })
    }

    fn authorizes(&self, tenant: &str, principal: Option<&[u8]>, aor: &CanonicalAor) -> bool {
        tenant == self.tenant && self.registration_authorizations.authorizes(principal, aor)
    }
}

// ---------------------------------------------------------------------------------- admission ---

/// The node's admission bound: how much work it will take on at once (`DP-11`).
///
/// **What it bounds, and what it deliberately does not.** The kernel's 1024-message queue plus
/// `503`-on-full is real backpressure, and it bounds the *queue*. The driver drains that queue as
/// fast as it can and spawns a task per new server transaction, and a **proxied** task lives for the
/// whole transaction — up to Timer B, or the 180-second unanswered backstop. So offered load
/// converted directly into resident tasks, and this is what stops that.
///
/// **Why REGISTER is exempt, and why that cannot starve.** A registration storm *is* the overload,
/// and REGISTER is the request a node most needs to answer while one is happening: a refused refresh
/// is a phone that becomes unreachable, so a node that shed REGISTERs would convert a spike into a
/// permanent outage and then get a second spike as every phone retried. A REGISTER never takes a
/// permit, never waits on one, and never observes this type at all — its path is `admit`, `apply`,
/// `respond`, with no acquisition anywhere in it. What bounds the *cost* of one REGISTER is `RG-14`;
/// what bounds how many proxied calls are resident is here, and the two compose.
///
/// An **ACK** is exempt for a different and harder reason: there is no response to an ACK in SIP, and
/// an ACK for a 2xx is a transaction of its own (RFC 3261 §17.1.1.3). "Refusing" one can only mean
/// dropping it, which leaves both ends in a dialog no timer reaps — the leak the kernel counts
/// separately as `ShedCounts::acks`. A bound that could not refuse must not gate.
///
/// Everything else on the proxy path — INVITE, BYE, CANCEL, OPTIONS — is gated. Exempting BYE and
/// CANCEL was considered, on the argument that shedding the messages that *end* work makes overload
/// self-sustaining; it was rejected because an unbounded method is an unbounded node, and a `503`
/// with `Retry-After` to a BYE is a retry, whereas an unbounded BYE flood is the defect this story
/// closes wearing a different method name.
///
/// **The counters are instance state, not process state.** They live in this struct, one per running
/// node, rather than in a global: the test suite runs in parallel, and a sibling scenario's flood
/// must not appear in this node's numbers.
#[derive(Debug)]
pub struct AdmissionBound {
    /// The bound itself.
    max: usize,
    /// Permits currently held — the gauge the scaling design calls decisive.
    in_flight: AtomicUsize,
    /// How many gated transactions have been admitted since the node started.
    admitted: AtomicU64,
    /// How many have been refused. **Counted, never logged per message**: the input that triggers
    /// the refusal is the input that would pay for the log line, and per-message logging under
    /// overload is a cost multiplier on exactly the wrong path. [`report_load`] samples this.
    refused: AtomicU64,
}

/// A permit held for as long as a transaction is in flight.
///
/// The bound is enforced by this value existing: it is taken at the moment of admission and released
/// when the task that serves the transaction ends, whether that is a final response, a timeout or a
/// panic in a spawned task.
#[derive(Debug)]
pub struct Admitted {
    gate: Arc<AdmissionBound>,
}

impl Drop for Admitted {
    fn drop(&mut self) {
        // `saturating_sub` rather than `fetch_sub`: an underflow here would print a gauge of
        // 18 quintillion in-flight transactions during an incident, and a wrong number is worse
        // than a slightly defensive one.
        let _ = self
            .gate
            .in_flight
            .fetch_update(Ordering::Release, Ordering::Acquire, |held| {
                Some(held.saturating_sub(1))
            });
    }
}

/// What the bound decided about one arrival.
#[derive(Debug)]
enum Verdict {
    /// Not subject to the bound — see [`AdmissionBound`] for which methods, and why.
    Exempt,
    /// Admitted; the permit is released when it drops.
    Admitted(Admitted),
    /// Over the bound. Answered `503`, not dropped.
    Refused,
}

impl AdmissionBound {
    /// A bound of `max` concurrent proxied transactions.
    ///
    /// A declared `0` is refused at load (`cluster-config` §8 V8), and clamped here as well rather
    /// than trusted: a bound of zero is a node that answers `503` to every call, and this layer is
    /// reached by configuration from more than one direction.
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            max: max.max(1),
            in_flight: AtomicUsize::new(0),
            admitted: AtomicU64::new(0),
            refused: AtomicU64::new(0),
        }
    }

    /// Whether the bound applies to `method`.
    fn gates(method: &Method) -> bool {
        !matches!(method, Method::Register | Method::Ack)
    }

    /// Decide whether to take on one arrival.
    ///
    /// A compare-and-swap and nothing else — no allocation, no lock, no `await` — because this runs
    /// on the accept loop, and the accept loop is the single consumer of a channel the kernel fills
    /// with `try_send` (sipx `T-19`).
    fn admit(self: &Arc<Self>, method: &Method) -> Verdict {
        if !Self::gates(method) {
            return Verdict::Exempt;
        }
        let taken = self
            .in_flight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                (held < self.max).then_some(held + 1)
            });
        if taken.is_ok() {
            self.admitted.fetch_add(1, Ordering::Relaxed);
            return Verdict::Admitted(Admitted {
                gate: Arc::clone(self),
            });
        }
        self.refused.fetch_add(1, Ordering::Relaxed);
        Verdict::Refused
    }

    /// How many proxied transactions are in flight right now.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }

    /// How many have been refused over the bound since the node started.
    #[must_use]
    pub fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    /// How many have been admitted since the node started.
    #[must_use]
    pub fn admitted(&self) -> u64 {
        self.admitted.load(Ordering::Relaxed)
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
    /// The transport driver stopped delivering (`DP-11`).
    ///
    /// `incoming.recv()` returning `None` means the kernel's endpoint loop is gone, and with it the
    /// socket, the timers and every transaction being held. This used to return `Ok(())`, so the
    /// process exited `0` and a node whose socket layer had died was indistinguishable from one that
    /// was asked to stop — a supervisor reading exit codes would not restart it, and nothing anywhere
    /// said what had happened. Contrast the care taken over refusing to *start*: the same event at
    /// the other end of the process deserves the same honesty.
    ///
    /// This node has no graceful-shutdown path, so there is no legitimate way to reach it. If one is
    /// added, it must close the loop through a signal it owns rather than by letting the channel
    /// close, or this error will report an intentional stop as a failure.
    #[error("the transport driver stopped delivering requests; this node can no longer serve")]
    TransportGone,
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
/// listeners cannot be served as declared. It also fails with [`NodeError::TransportGone`] if the
/// transport driver stops delivering, which is the one way a *running* node ends today.
pub async fn run(config: NodeConfig) -> Result<(), NodeError> {
    run_reporting(config, |_| {}).await
}

/// Run the node, and hand the address it **actually** bound to `bound` before serving anything.
///
/// [`run`] is this with a report that goes nowhere. The two exist because a node may be told to bind
/// port 0 — the kernel picks the port, and there is then no way to address the node except to ask it.
/// That is already the contract on stdout: `listening on <addr>` is printed after the bind precisely
/// so a caller need not guess (`scripts/e2e-call.sh` and `website/docs/guides/run-a-node.md` both
/// wait on that line). A caller *inside* the process cannot read stdout, so it gets the same fact
/// through this instead — the same value, at the same moment, from the same place.
///
/// `bound` is called after the bind and after every startup refusal, for the reason the `listening
/// on` line is printed there: a report that a node is up must not precede the last thing that can
/// stop it coming up.
///
/// # Errors
///
/// Exactly [`run`]'s. A node that never binds never reports, which is what makes waiting on the
/// report a sound readiness check rather than a hopeful one.
pub async fn run_reporting(
    config: NodeConfig,
    bound: impl FnOnce(SocketAddr),
) -> Result<(), NodeError> {
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
    // The same announcement, for a caller that shares the process with the node rather than its
    // stdout. Between the two `println!`s and the log line on purpose: whatever a reader of one of
    // these learns, a reader of the other learns at the same point in the startup sequence.
    bound(handle.local_addr());
    tracing::info!(
        listen = %handle.local_addr(),
        %advertised,
        tenant = %config.tenant,
        // What the declared roles wired (`DP-13`). Printed for the same reason the store and the
        // tenant are: it is a fact about what this node will answer, and while nothing consumed the
        // roles there was no way to tell a registrar from a proxy from outside.
        serves = config.capabilities.describe(),
        store = config.store.describe(),
        // Named for the same reason `RG-12` named the store: an operator reading one line should be
        // able to tell an open tenant from an authenticated one. Today it is always `open`, which is
        // exactly why it is worth printing.
        auth = if config.auth.is_some() { "required" } else { "open" },
        "node listening"
    );
    let policy = config.policy;
    // One authenticator for the node, because it holds the replay window: a per-request one would
    // forget every nonce-count the moment it was created, which is a replay window that never says
    // no. `std::sync::Mutex` rather than tokio's — `decide` is a hash and a lookup, and it is never
    // held across an await.
    let auth = Arc::new(Mutex::new(config.tenant_auth()));
    let credentials = Arc::new(config.credentials());
    let admission = Arc::new(AdmissionBound::new(config.max_in_flight_transactions));
    tracing::info!(
        max_in_flight_transactions = admission.max,
        "admission bound"
    );
    report_load(handle.clone(), Arc::clone(&admission));

    // The cookie key is the node's, not the request's: it must be the same for every message this
    // process forwards, or a loop through this node would not be detectable across two of them.
    let cookie_key = cookie_key();

    while let Some(arrival) = incoming.recv().await {
        // The admission decision (`DP-11`) is taken **here**, before anything is cloned and before a
        // task exists, because that is where "how much is in flight" is a fact. It is a
        // compare-and-swap and nothing else — the accept loop must never do work inline: it is the
        // single consumer of the incoming channel, and the kernel delivers into that channel with
        // `try_send`, so a blocked loop drops requests silently (sipx `T-19`).
        let permit = match admission.admit(&arrival.request.method) {
            Verdict::Exempt => None,
            Verdict::Admitted(permit) => Some(permit),
            Verdict::Refused => {
                let handle = handle.clone();
                // Refused from a task of its own, for two reasons: sending is IO and IO does not
                // happen on this loop, and a request the node has already decided not to serve
                // should not cost it a clone of the whole serving context first.
                tokio::spawn(async move {
                    // An error here means the endpoint is gone, which [`report_load`] says once
                    // rather than once per message. This is precisely the path that must not log
                    // per message: the input that triggers the refusal would be paying for it.
                    let _ = handle
                        .respond(&arrival.key, overloaded(&arrival.request))
                        .await;
                });
                continue;
            }
        };

        let handle = handle.clone();
        let store = Arc::clone(&store);
        let config = config.clone();
        let auth = Arc::clone(&auth);
        let credentials = Arc::clone(&credentials);
        let cookie_key = cookie_key.clone();

        // One task per admitted arrival.
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
            // Released here — when the transaction is done with, not when its request arrived.
            // Dropped by name so that the release point is something a reader can see.
            drop(permit);
        });
    }

    // Not `Ok(())`. The channel closing means the kernel's endpoint loop is gone, which is a failure
    // of the node and not a shutdown of it.
    Err(NodeError::TransportGone)
}

/// The refusal for a request the node will not take on: `503` with `Retry-After`.
///
/// **The same shape the kernel uses** when its own queue is full (`sipx-transport`'s
/// `Endpoint::refuse`), down to the `Retry-After` value, so a client sees one behaviour regardless of
/// which layer shed it. A node-level refusal that looked different would make the two limits two
/// protocols, and an operator correlating a client's retry pattern with a server's counters would
/// have to know which one had fired to read either.
fn overloaded(request: &Request) -> Response {
    let mut response = answer(request, 503, reason_for(503));
    if let Ok(header) = sipx_sip::Header::build(HeaderName::RetryAfter, RETRY_AFTER) {
        response.headers.push(header);
    }
    response
}

/// The kernel's `Retry-After` on a queue-full refusal, adopted rather than restated differently.
const RETRY_AFTER: &[u8] = b"5";

/// What the node's load instruments read at one instant.
///
/// A struct so that "has anything moved?" is one comparison rather than five, and so that adding an
/// instrument cannot forget to add it to the change test.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Load {
    /// Transactions the kernel is holding, including the ones it keeps for Timer J.
    outstanding: usize,
    /// Proxied transactions this node has admitted and not yet finished (`DP-11`).
    in_flight: usize,
    /// Admitted over the node's admission bound, cumulative. Reported because a shed *rate* needs a
    /// denominator: "32 refused" says something very different beside 40 admitted than beside 4000.
    admitted: u64,
    /// Refused over the node's admission bound, cumulative.
    refused: u64,
    /// Requests the kernel shed because this application was not keeping up, cumulative.
    shed_requests: u64,
    /// ACKs the kernel shed. The serious one: an ACK cannot be refused, so each of these is a
    /// dialog no timer will reap.
    shed_acks: u64,
    /// Requests that matched no transaction and could not be handed over.
    shed_unmatched: u64,
}

/// Report the node's load whenever it changes — the kernel's instruments and this node's own.
///
/// A proxy that leaks one transaction per call is a slow, quiet outage: nothing looks wrong until
/// the process does. This is the cheapest instrument that would notice, and it is `DP-3`'s gauge in
/// embryo.
///
/// **`Handle::shed()` is read here** (`DP-11`), split the way the kernel splits it — requests, ACKs,
/// unmatched — because the three are different failures. `website/docs/operate/scaling.md` names
/// overload shed rate as the one number that says the platform is past its limit, and that number
/// existed in-process and was discarded. `ShedCounts::acks` gets a line of its own when it moves: an
/// ACK cannot be answered with a `503`, so shedding one leaks a call, and "calls are leaking" is not
/// a field on a routine gauge line.
///
/// **On change, not on a schedule.** A number logged every second is noise nobody reads; a number
/// logged when it moves is a record of what the node did. It also has to be sampled rather than
/// emitted per request: the count that matters is the one *after* the last request, and a per-request
/// line can never show that, which is exactly how the first version of this failed to observe the
/// store draining at all. Sampling is also what keeps overload logging off the per-message path —
/// the node **counts** refusals and this reports the count, so a flood costs one line per sampling
/// interval rather than one line per refused request.
fn report_load(handle: Handle, admission: Arc<AdmissionBound>) {
    tokio::spawn(async move {
        let mut previous = Load::default();
        // An idle node's first sample equals `Load::default()`, so "report on change" alone would
        // never say anything at all about a node that is up and quiet. The first sample always goes
        // out; after that, only movement does.
        let mut reported = false;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Read the shed counters **first**. They come off a shared atomic rather than from the
            // event loop, so they are available in exactly the situation that makes them
            // interesting; `outstanding()` has to ask the loop, and the loop is busy then.
            let shed = handle.shed();
            let Ok(outstanding) = handle.outstanding().await else {
                // The endpoint is gone. Say so with the final counts rather than vanishing, because
                // these are the numbers that describe how it ended.
                tracing::warn!(
                    shed_requests = shed.requests,
                    shed_acks = shed.acks,
                    shed_unmatched = shed.unmatched,
                    refused = admission.refused(),
                    "the transport endpoint is gone; final load counters"
                );
                return;
            };
            let sample = Load {
                outstanding,
                in_flight: admission.in_flight(),
                admitted: admission.admitted(),
                refused: admission.refused(),
                shed_requests: shed.requests,
                shed_acks: shed.acks,
                shed_unmatched: shed.unmatched,
            };

            if sample.shed_acks > previous.shed_acks {
                tracing::error!(
                    shed_acks = sample.shed_acks,
                    since_last = sample.shed_acks - previous.shed_acks,
                    "the kernel shed ACKs: each one is a dialog no timer will reap, and calls are \
                     leaking"
                );
            }
            if sample != previous || !reported {
                previous = sample;
                reported = true;
                tracing::info!(
                    outstanding = sample.outstanding,
                    in_flight = sample.in_flight,
                    admitted = sample.admitted,
                    refused = sample.refused,
                    shed_requests = sample.shed_requests,
                    shed_acks = sample.shed_acks,
                    shed_unmatched = sample.shed_unmatched,
                    "node load"
                );
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
    // `PX-10` — the document's Timer C, or nothing the document said ever reaches a branch. Assigned
    // rather than left at `ProxyConfig::new`'s default, which is what `grep -n timer_c driver.rs`
    // used to return no matches for. The engine still applies F11's floor on top
    // (`effective_timer_c`), so a value the loader could not have refused — a `NodeConfig` built in
    // code — cannot arm a Timer C the RFC forbids.
    proxy.timer_c = config.timer_c;
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

/// Which path an arrival takes, given what this node's roles wired (`cluster-config` §4 R3).
///
/// A function of the node's [`Capabilities`] and the request's method, and deliberately **not** a
/// `match` buried inside [`serve`]: R3's whole point is that the role set is consulted when a node is
/// *wired* and never when a request is *classified*, and a decision that can be exercised without a
/// socket is the only kind this project keeps (AGENTS.md #2). The unit tests at the foot of this file
/// are the whole matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dispatch {
    /// The registrar answers it out of the location service.
    Registrar,
    /// The proxy engine carries it onward.
    Proxy,
    /// An ACK for a 2xx: a separately routed request (RFC 3261 §17.1.1.3, proxy-behavior §7.2 K3),
    /// forwarded with no transaction of its own and **never answered**.
    ///
    /// Only one of the method's three messages reaches here at all, which is why this arm can treat
    /// every arrival as K3. The other two never leave the kernel: the ACK for a non-2xx going
    /// downstream is generated by the client transaction that received the final response (K1), and
    /// the ACK for a non-2xx arriving from upstream is absorbed by the server transaction that sent
    /// it — Completed → Confirmed, which is also what stops that response being retransmitted (K2,
    /// §17.2.1). The transaction layer hands an ACK up only when it matched no server transaction, or
    /// matched one in `Accepted`, and both of those are ACKs for a 2xx.
    Ack,
    /// No path on this node serves this method — answered `405`, never dropped.
    NotAllowed,
    /// An ACK this node cannot forward. RFC 3261 §17.1.1.3 makes an ACK for a 2xx a transaction of
    /// its own that **nothing answers**, so a node with no forwarding path can only drop it: there is
    /// no status to send, and inventing one would put a response on a transaction that has none.
    /// Recorded rather than silent, which is the most a refusal can be here.
    Unroutable,
}

impl Dispatch {
    fn of(capabilities: Capabilities, method: &Method) -> Self {
        match method {
            Method::Register if capabilities.registrar => Dispatch::Registrar,
            Method::Register => Dispatch::NotAllowed,
            Method::Ack if capabilities.proxy => Dispatch::Ack,
            Method::Ack => Dispatch::Unroutable,
            _ if capabilities.proxy => Dispatch::Proxy,
            _ => Dispatch::NotAllowed,
        }
    }
}

async fn serve(
    handle: &Handle,
    edge: &Edge<'_>,
    arrival: Incoming,
) -> Result<(), sipx_transport::Error> {
    let capabilities = edge.config.capabilities;
    match Dispatch::of(capabilities, &arrival.request.method) {
        Dispatch::Registrar => {
            let response = register(edge, &arrival);
            handle.respond(&arrival.key, response).await
        }
        Dispatch::Ack => forward_ack(handle, edge.proxy, &arrival).await,
        Dispatch::Proxy => {
            proxy_request(handle, edge.store, edge.config, edge.proxy, arrival).await
        }
        Dispatch::NotAllowed => {
            tracing::info!(
                method = %arrival.request.method,
                source = %arrival.source,
                serves = capabilities.describe(),
                "refused: this node's roles do not wire that method"
            );
            let response = not_allowed(&arrival.request, capabilities);
            handle.respond(&arrival.key, response).await
        }
        Dispatch::Unroutable => {
            tracing::info!(
                source = %arrival.source,
                serves = capabilities.describe(),
                "dropped an ACK: this node has no forwarding path, and an ACK has no response"
            );
            Ok(())
        }
    }
}

/// The refusal for a method this node's roles do not wire: `405`, naming the methods they do.
///
/// **An answer, not a drop.** `cluster-config` §4 R5's "projected away" is about configuration a node
/// ignores; a *request* it received is a different thing, and silence there is indistinguishable from
/// a broken listener. RFC 3261 §21.4.6 is the status for a method the server understands and does not
/// allow here, and it requires the `Allow` header — which is also the only way the far end can tell a
/// role boundary from a defect.
fn not_allowed(request: &Request, capabilities: Capabilities) -> Response {
    let mut response = answer(request, 405, reason_for(405));
    if let Ok(header) = sipx_sip::Header::build(HeaderName::Allow, allowed(capabilities)) {
        response.headers.push(header);
    }
    response
}

/// The methods this node's wiring serves, as an `Allow` value (RFC 3261 §20.5).
///
/// Derived from the capabilities rather than written out per role, so a node that is both a registrar
/// and a proxy cannot end up advertising one of the two.
fn allowed(capabilities: Capabilities) -> String {
    let mut methods = Vec::new();
    if capabilities.proxy {
        methods.extend(["INVITE", "ACK", "BYE", "CANCEL", "OPTIONS"]);
    }
    if capabilities.registrar {
        methods.push("REGISTER");
    }
    methods.join(", ")
}

// ---------------------------------------------------------------------------------- registrar ---

/// Write one line of the authentication audit trail — [registrar-auth](
/// https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §9, `RG-15`.
///
/// Every outcome of §3 gets exactly one record (§9 L1). Before this, the decision computed the
/// reason for every `401` and `403` — `ChallengeResponse::because` is documented "Why, for logs and
/// tests" — and this driver dropped it, so a refusal was indistinguishable from silence outside the
/// process. With no rate limiting and a 300-second nonce lifetime, that is what made brute force
/// against a tenant undetectable as well as unbounded.
///
/// **It takes an [`AuthOutcome`] and not an `Admission`, and that is the fix for the hole `RG-15`'s
/// first pass left.** An `Admission::Reject` cannot say what §3 decided: a correct digest followed
/// by a malformed `Contact` arrives there with the proven principal already dropped, so a driver
/// reading the `Admission` recorded nothing at all — precisely the state §9 L3 forbids, since "an
/// absent record and an unauthenticated one are different facts". `admit_audited` keeps the record.
///
/// **Nothing the far end sent may ride into a record** (§9 L2). Every reason comes from
/// [`AuthOutcome::describe`], which returns `&'static str` and is therefore structurally incapable
/// of carrying a nonce, a `cnonce`, a response digest, a presented username or a password. The one
/// runtime value in a record is §5's principal, which is the identity the digest *proved* and is
/// already what the binding is stored under — and it is rendered quoted, because an
/// operator-provisioned username containing CR/LF would otherwise split a record across lines, and
/// log injection in an audit trail is worth a pair of quotes. The far end is identified by the
/// address the socket observed, which is the one field in a record no attacker chooses.
fn record_authentication(edge: &Edge<'_>, arrival: &Incoming, outcome: &AuthOutcome) {
    let tenant = edge.config.tenant.as_str();
    let source = arrival.source;
    let because = outcome.describe();
    match outcome {
        // §9 L3 — a success is a record too, and `Unauthenticated` is the trail *saying* nobody was
        // authenticated (§3 A1) rather than failing to say anything.
        AuthOutcome::Authenticated(principal) => tracing::info!(
            tenant,
            %source,
            principal = ?String::from_utf8_lossy(principal),
            because,
            "authentication succeeded"
        ),
        AuthOutcome::Unauthenticated => tracing::info!(
            tenant,
            %source,
            because,
            "authentication not required: proceeding unauthenticated"
        ),
        // A2 is **not** a refusal — it is the first half of a round trip the client is expected to
        // complete, and every phone's ordinary first REGISTER takes it. Recording it as trouble
        // would bury the real thing. The split itself is `AuthOutcome`'s, not this driver's.
        AuthOutcome::Challenged { status } => tracing::info!(
            tenant,
            %source,
            status,
            because,
            "authentication challenged"
        ),
        AuthOutcome::Refused {
            status,
            stale,
            because: _,
        } => tracing::warn!(
            tenant,
            %source,
            status,
            stale,
            because,
            "authentication refused"
        ),
        // §3 A3.
        AuthOutcome::Forbidden => tracing::warn!(
            tenant,
            %source,
            status = 403,
            because,
            "authentication refused"
        ),
    }
}

/// Why a complete registrar outcome could not be represented as SIP headers.
///
/// This is a driver failure, never a reason to send a partial success. The outcome is already
/// decided by the time this can happen; changing its policy answer would reconstruct registrar
/// semantics here, so the only controlled response is an internal failure with none of the
/// partially built fact headers attached.
#[derive(Debug, thiserror::Error)]
enum RegisterRenderError {
    /// The kernel refused bytes that cannot safely inhabit one header field.
    #[error("the kernel refused a required REGISTER response header: {0}")]
    Header(#[from] sipx_sip::BuildError),
    /// The sans-IO type stores thousandths in a `u16`; only 0 through 1000 have a SIP q spelling.
    #[error("the registrar produced Contact q={0}, outside the 0..=1000 invariant")]
    InvalidQ(u16),
}

/// Render every fact of a registrar decision, or none of them.
///
/// Exhaustive over both [`Outcome`] and [`Rejection`] on purpose. Adding a core outcome must make
/// this driver decide its wire representation at compile time; a status-only wildcard would reopen
/// the exact seam `RG-19` closes. No policy is decided here — successful and rejection payloads are
/// serialized exactly as the sans-IO registrar supplied them.
fn render_register_outcome(
    request: &Request,
    outcome: &Outcome,
) -> Result<Response, RegisterRenderError> {
    match outcome {
        Outcome::Commit { response, .. } | Outcome::Noop { response } => {
            render_accepted(request, response)
        }
        Outcome::Reject(rejection) => render_rejection(request, rejection),
    }
}

/// A `200` with the complete current binding set and its shared Path (§5.6).
fn render_accepted(
    request: &Request,
    accepted: &Accepted,
) -> Result<Response, RegisterRenderError> {
    let mut response = answer(request, 200, reason_for(200));

    // Order is the outcome's order: the complete active set first, then the stored Path vector.
    // Each value gets its own row, which the kernel's typed list reader treats identically to one
    // comma row while preserving exact element order.
    for contact in &accepted.contacts {
        push_register_header(&mut response, HeaderName::Contact, render_contact(contact)?)?;
    }
    for path in &accepted.path {
        push_register_header(&mut response, HeaderName::Path, bracketed(path))?;
    }
    if !accepted.path.is_empty() {
        push_register_header(
            &mut response,
            HeaderName::Supported,
            Bytes::from_static(b"path"),
        )?;
    }

    Ok(response)
}

/// A refusal with the remedy facts its typed variant carries.
fn render_rejection(
    request: &Request,
    rejection: &Rejection,
) -> Result<Response, RegisterRenderError> {
    let status = rejection.status();
    let mut response = answer(request, status, reason_for(status));
    match rejection {
        Rejection::BadExtension(offenders) => {
            push_register_header(&mut response, HeaderName::Unsupported, offenders.join(", "))?;
        }
        Rejection::IntervalTooBrief { min } => {
            push_register_header(&mut response, HeaderName::MinExpires, min.to_string())?;
        }
        Rejection::ExtensionRequired(extension) => {
            push_register_header(&mut response, HeaderName::Require, *extension)?;
        }
        Rejection::NotFound
        | Rejection::BadRequest(_)
        | Rejection::Forbidden(_)
        | Rejection::StaleSequence
        | Rejection::Unavailable => {}
    }
    Ok(response)
}

/// Append one required fact, propagating rather than swallowing a builder refusal.
fn push_register_header(
    response: &mut Response,
    name: HeaderName,
    value: impl Into<Bytes>,
) -> Result<(), RegisterRenderError> {
    response.headers.push(sipx_sip::Header::build(name, value)?);
    Ok(())
}

/// One stored contact with the granted duration and q, both supplied by the outcome.
fn render_contact(contact: &ContactValue) -> Result<Bytes, RegisterRenderError> {
    let q = match contact.q {
        0..=999 => format!("0.{:03}", contact.q),
        1_000 => "1.000".to_owned(),
        other => return Err(RegisterRenderError::InvalidQ(other)),
    };
    let mut value = Vec::with_capacity(contact.contact.len() + q.len() + 32);
    value.push(b'<');
    value.extend_from_slice(&contact.contact);
    value.extend_from_slice(b">;expires=");
    value.extend_from_slice(contact.expires.to_string().as_bytes());
    value.extend_from_slice(b";q=");
    value.extend_from_slice(q.as_bytes());
    Ok(Bytes::from(value))
}

/// A stored URI as a route-style header value; brackets keep URI parameters inside the URI.
fn bracketed(uri: &Bytes) -> Bytes {
    let mut value = Vec::with_capacity(uri.len() + 2);
    value.push(b'<');
    value.extend_from_slice(uri);
    value.push(b'>');
    Bytes::from(value)
}

/// Fail closed if any required header cannot be built: a `500`, never a partial original outcome.
fn register_outcome_or_internal(request: &Request, outcome: &Outcome, tenant: &str) -> Response {
    match render_register_outcome(request, outcome) {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(
                tenant,
                decided_status = outcome.status(),
                %error,
                "could not render the complete REGISTER outcome; sending an internal failure"
            );
            answer(request, 500, reason_for(500))
        }
    }
}

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
    let (admission, outcome) = {
        let mut auth = match edge.auth.lock() {
            Ok(guard) => guard,
            // **The poison bypass is deliberate, and `RG-15` re-argued it rather than inheriting
            // it.** Keeping the bypass, because the alternative is worse in the direction that
            // matters: propagating the poison would make one panic anywhere in the decision stop
            // *every* REGISTER for this tenant for the life of the process, and a registrar that
            // stops answering refreshes converts a transient fault into every phone on the tenant
            // going unreachable.
            //
            // What the bypass can cost is bounded, and it is bounded on the safe side. Nothing a
            // panic can leave half-written makes the edge accept something it should refuse: the
            // realm, the secret and the algorithm are immutable for the authenticator's life, and
            // the only mutable state is the replay window, whose torn state is an entry whose count
            // advanced without its digest. That refuses a *correct* credential as a replay — a
            // client re-challenged with a fresh nonce, which it answers by itself. Fail-closed.
            //
            // What was actually wrong here was that it was silent. A poisoned authentication lock
            // means something panicked while deciding who a caller is, which is not a fact to
            // swallow, so it is recorded before it is stepped over.
            Err(poisoned) => {
                tracing::error!(
                    tenant = %edge.config.tenant,
                    "the authentication lock is poisoned: something panicked while deciding \
                     authentication. Continuing on the recovered state, which can only refuse a \
                     correct credential and never accept a wrong one"
                );
                poisoned.into_inner()
            }
        };
        // `admit_audited`, not `admit`: the §3 outcome has to survive a `Proceed` that then fails
        // to become a command, or a correctly authenticated REGISTER with a malformed `Contact`
        // records nothing at all (§9 L3).
        admit_audited(
            &arrival.request,
            &mut auth,
            edge.credentials,
            edge.config,
            &context,
            now(),
        )
    };

    // registrar-auth §9 — the audit trail. Emitted here, from the driver, and not from the decision
    // that produced it: the registrar is sans-IO, and a decision function that logs does an effect
    // the harness cannot replay from a seed. The registrar's job is to produce the fact; this
    // layer's job is to emit it. Every request that reached authentication has `Some`; S1 rejects
    // before authentication and therefore has no authentication fact to record. Emission remains
    // before the branch below so no post-authentication path can lose its record.
    if let Some(outcome) = &outcome {
        record_authentication(edge, arrival, outcome);
    }

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
            // **Not** an authentication record — §3's outcome was written above, unconditionally.
            // This is the parse diagnostic for a request that authenticated (or needed no
            // authentication) and then failed to become a command, and it is `debug` because a
            // malformed REGISTER is a client bug rather than a security event.
            //
            // `detail` is bound out of `BadRequest`, whose payload is `&'static str` **by type**,
            // which is what makes printing it safe. Do not replace this with `%rejection`:
            // `Rejection` as a whole is not static-only — `BadExtension(Vec<String>)` formats
            // attacker-supplied option tags with `{0:?}` and `IntervalTooBrief` formats a number —
            // and only reachability keeps those out of `admit` today.
            let detail = match &rejection {
                sipx_clstr_registrar::Rejection::BadRequest(detail) => *detail,
                _ => "the message could not become a command",
            };
            tracing::debug!(
                tenant = %edge.config.tenant,
                status = rejection.status(),
                detail,
                "a REGISTER could not become a command"
            );
            return register_outcome_or_internal(
                &arrival.request,
                &Outcome::Reject(rejection),
                &edge.config.tenant,
            );
        }
    };

    let applied = apply(
        edge.store,
        &cmd,
        edge.policy,
        sipx_clstr_registrar::store::DEFAULT_CAS_RETRIES,
    );
    register_outcome_or_internal(&arrival.request, &applied.outcome, &edge.config.tenant)
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

    let mut forwarded = Vec::new();
    perform(
        handle,
        store,
        config,
        &key,
        &mut context,
        effects,
        &mut forwarded,
    )
    .await?;

    // Drive the branches **concurrently** (`PX-9`), which is what makes parallel forking parallel.
    //
    // Awaiting them in order was the shape until this story, on the argument that with a single fork
    // group it came to the same thing. It does not, and the ordinary two-device registration is the
    // counterexample: two contacts, both `q=1000`, one group (`lookup.rs` §7 L4). A branch's stream
    // only ends when its transaction does, so draining one to exhaustion first meant a dead device
    // held the task until the kernel's Timer B — 64·T1, about thirty seconds — while the live
    // device's `200 OK` sat unread in the other stream.
    //
    // The engine still sees **one input at a time**, in a total order, which is the property the
    // driver design names and the reason a `JoinSet` rather than a shared context: the branches are
    // read concurrently and reduced serially. Nothing about §16.7 selection moved into the driver,
    // and nothing about concurrency moved into the engine.
    let mut branches = JoinSet::new();
    watch_all(&mut branches, forwarded);

    while let Some(joined) = branches.join_next().await {
        let (branch, responses, event) = match joined {
            Ok(next) => next,
            Err(error) => {
                // Only reachable if a task that does nothing but await a channel panicked, or if the
                // set were aborted — which only happens when it is dropped, after this loop. Said
                // rather than swallowed: the branch is lost, and a lost branch is a call the context
                // will wait on until it runs out of branches.
                tracing::warn!(%error, "a branch reader ended unexpectedly");
                continue;
            }
        };
        let input = match event {
            Some(sipx_sip::TuEvent::Response(response)) => {
                // Re-armed *before* the effects are performed, so the other branches — and this one
                // — keep being read while a response is going upstream.
                watch(&mut branches, branch.clone(), responses);
                ProxyInput::BranchResponse(response, branch)
            }
            Some(sipx_sip::TuEvent::Timeout) => {
                watch(&mut branches, branch.clone(), responses);
                // The driver design's mapping: a kernel timeout is a `408` from that branch.
                match ResponseBuilder::to_request(
                    &arrival.request,
                    ok_status(408),
                    "Request Timeout",
                ) {
                    Ok(builder) => ProxyInput::BranchResponse(Box::new(builder.build()), branch),
                    Err(_) => ProxyInput::BranchTransportError(branch),
                }
            }
            Some(sipx_sip::TuEvent::TransportError) => {
                watch(&mut branches, branch.clone(), responses);
                ProxyInput::BranchTransportError(branch)
            }
            // An ACK on a client transaction is not ours; counted by being ignored explicitly
            // rather than by falling through a wildcard. The branch is put back: ignoring an event
            // must not stop the branch being read.
            Some(sipx_sip::TuEvent::Ack(_) | sipx_sip::TuEvent::Request(_)) => {
                watch(&mut branches, branch, responses);
                continue;
            }
            // A finished context accepts no further input, so there is nothing left to drive and
            // holding the task open would hold this transaction's admission permit (`DP-11`) with
            // it. The loop is left by `return` the instant the context finishes, so this is the
            // belt to that braces — and it stops rather than spins, because the alternative is a
            // task that lives until every branch's Timer B for no reason.
            None if context.is_finished() => break,
            // The stream ended with no final. A branch that vanishes is a branch that failed, and
            // leaving the context waiting on it forever is the failure mode this arm exists to
            // prevent. Not re-armed: there is nothing left to read.
            None => ProxyInput::BranchTransportError(branch),
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
        // A later `q` group forks when this one concludes (§7 L4), so new branches arrive here.
        watch_all(&mut branches, more);
        if context.is_finished() {
            return Ok(());
        }
    }
    Ok(())
}

/// One branch's next event, with its stream handed back so the branch can be read again.
///
/// The stream travels with the event because [`sipx_transport::Responses`] is consumed by reference
/// from an `async fn` and is not a `Stream`: the only way to await several at once without a lock is
/// to move each into a task for one event and take it back.
type BranchEvent = (BranchId, Responses, Option<sipx_sip::TuEvent>);

/// Read one event from `responses`, concurrently with every other branch.
fn watch(branches: &mut JoinSet<BranchEvent>, branch: BranchId, mut responses: Responses) {
    branches.spawn(async move {
        let event = responses.next().await;
        (branch, responses, event)
    });
}

/// Start reading every branch a round of effects forwarded.
fn watch_all(branches: &mut JoinSet<BranchEvent>, forwarded: Vec<(BranchId, Responses)>) {
    for (branch, responses) in forwarded {
        watch(branches, branch, responses);
    }
}

async fn perform(
    handle: &Handle,
    store: &(dyn LocationStore + Send + Sync),
    config: &NodeConfig,
    key: &TransactionKey,
    context: &mut ResponseContext,
    effects: Vec<ProxyEffect>,
    pending: &mut Vec<(BranchId, Responses)>,
) -> Result<(), sipx_transport::Error> {
    for effect in effects {
        match effect {
            ProxyEffect::ResolveTargets(query) => {
                // A Request-URI this platform cannot canonicalize is still an *answer*: location-service
                // §3 says a lookup rejection is the empty target set, and §7 L5 leaves what to say about
                // it to the proxy — `480`. Only the store failing to answer is `TargetsUnavailable`
                // (§7 L8): the question was well-formed and this node could not resolve it.
                let found = match CanonicalAor::parse(query.uri.clone()) {
                    Ok(aor) => store.lookup(&config.tenant, &aor, now()),
                    Err(_) => Ok(Vec::new()),
                };
                let input = match found {
                    Ok(found) => ProxyInput::TargetsResolved(targets_from_lookup(&found)),
                    Err(failure) => {
                        tracing::error!(
                            tenant = %config.tenant,
                            uri = %String::from_utf8_lossy(&query.uri),
                            %failure,
                            "the location store could not be read; the call is refused rather \
                             than answered as an empty address-of-record"
                        );
                        ProxyInput::TargetsUnavailable
                    }
                };
                let more = context.on_input(input);
                Box::pin(perform(handle, store, config, key, context, more, pending)).await?;
            }
            ProxyEffect::Forward {
                branch,
                request,
                target,
                next_hop,
            } => {
                // F7's hop, not the target: the two differ whenever a `Route` survived preprocessing
                // or the target carries a `Path`, and sending to the target then skips the hop the
                // dialog or the registration says must be traversed.
                let Some(destination) = destination_of(&next_hop) else {
                    // §16.9 — a hop we cannot address is a branch that failed, and it settles as an
                    // input rather than as a `continue`. The engine has already recorded the branch,
                    // so skipping it silently left a context waiting on a request that was never
                    // sent: no response would ever come, the transaction held its admission permit
                    // (`DP-11`), and nothing upstream was ever answered.
                    tracing::warn!(
                        %branch,
                        next_hop = %String::from_utf8_lossy(&next_hop),
                        target = %String::from_utf8_lossy(&target.uri),
                        "no address for this branch's next hop; treating it as a transport failure"
                    );
                    let more = context.on_input(ProxyInput::BranchTransportError(branch));
                    Box::pin(perform(handle, store, config, key, context, more, pending)).await?;
                    continue;
                };
                let responses = handle.send(*request, destination).await?;
                pending.push((branch, responses));
            }
            // P2's question, and this driver can only give it P3's answer. Verifying needs the
            // cluster key set (affinity-token §6), which arrives by configuration and which
            // `cluster-membership` §4's `keys[]` schema does not yet have a loader for — so this
            // node holds no keys, mints no tokens, and its own `Record-Route` carries no `aft`. An
            // `aft` reaching it therefore came from somewhere else, and §8 S2's answer for a key id
            // that is not in the key set is exactly this: `Invalid`, and the engine's `403`. There
            // is no fallback and no degraded mode to reach for. Leaving the effect unanswered would
            // be worse than wrong — the context would wait for a verdict forever, holding its
            // transaction and its admission permit (`DP-11`).
            ProxyEffect::VerifyToken { .. } => {
                tracing::warn!(
                    "a Route carried an affinity token and this node holds no key set: rejecting"
                );
                let more = context.on_input(ProxyInput::TokenFact(TokenVerdict::Invalid));
                Box::pin(perform(handle, store, config, key, context, more, pending)).await?;
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
            // `SetTimer`/`ClearTimer` reach no clock here, and `PX-10` did **not** change that: what
            // it changed is the deadline the engine puts in `SetTimer` — the document's Timer C
            // rather than a private default (see `proxy_config_keyed`). So on this driver a Timer C
            // is armed with the right value and never fires, and a branch that goes quiet after a
            // provisional is reaped by the kernel's Timer B or not at all. The deterministic harness
            // does perform these effects, which is why `PB-C-5`/`PB-C-6` are proved there. Wiring a
            // real clock to them belongs with `CancelBranch` above — a fired Timer C's first act is
            // to cancel the branch (§9 C5), so a driver that armed the timer without wiring the
            // cancel would reap branches it could not tell to stop. `PX-6` owns the pair.
            ProxyEffect::AnswerCancel
            | ProxyEffect::SetTimer { .. }
            | ProxyEffect::ClearTimer { .. }
            | ProxyEffect::Terminate => {}
        }
    }
    Ok(())
}

/// Forward an ACK for a 2xx: a request in its own right, and one nothing ever answers.
///
/// **The engine decides where it goes** (proxy-behavior §7.2 K3): validation, `Route` preprocessing,
/// the predetermined target set and F7's next hop are [`route_ack`]'s, so this ACK is edited by
/// exactly the code that edits every other forwarded request. What is left here is the two things
/// that need a socket — resolving the hop's address and putting the bytes on the wire — and the
/// record for the cases that cannot.
///
/// This used to ask the **location service** instead: it canonicalized the Request-URI as an address
/// of record, took the first registration and returned `Ok(())` when there was none. A dialog's
/// remote target is a `Contact`, not an AoR, so for an ordinary call there was never a binding under
/// that key — every ACK was dropped, silently, and both ends were left in a call no timer reaps
/// (`V-03`). Where a binding *did* exist under the contact's canonical key, it was worse: the
/// acknowledgement went to whatever that binding named.
///
/// `handle.respond` is deliberately unreachable from here. There is no response to an ACK in SIP
/// (RFC 3261 §17.1.1.3), which `cluster-config` §8 V11 already relies on, so a refusal can only be a
/// log line — and that is the one thing the merge base did not do either.
async fn forward_ack(
    handle: &Handle,
    proxy: &ProxyConfig,
    arrival: &Incoming,
) -> Result<(), sipx_transport::Error> {
    // K3 takes §5 like any other request, so a carried token is verified before this ACK moves.
    // The verdict is the same one `perform` gives above and for the same reason — this node holds
    // no key set, so nothing it did not mint can verify — and the refusal is the only one this
    // method has: a record, never a response.
    let verdict = match route_ack(arrival.request.clone(), proxy, None) {
        AckRoute::Verify { .. } => Some(TokenVerdict::Invalid),
        _ => None,
    };
    match route_ack(arrival.request.clone(), proxy, verdict.as_ref()) {
        AckRoute::Forward { request, next_hop } => {
            let Some(destination) = destination_of(&next_hop) else {
                tracing::warn!(
                    next_hop = %String::from_utf8_lossy(&next_hop),
                    source = %arrival.source,
                    "dropped an ACK: its next hop resolves to no address, and an ACK has no response"
                );
                return Ok(());
            };
            // `send_directly`, not `send`: an ACK for a 2xx is not a transaction to be retransmitted
            // or timed out, and creating a client transaction for one would leave an entry nothing
            // ever concludes.
            handle.send_directly(*request, destination).await
        }
        AckRoute::Unroutable(refusal) => {
            tracing::warn!(
                because = refusal.describe(),
                source = %arrival.source,
                "dropped an ACK: it cannot be forwarded, and an ACK has no response"
            );
            Ok(())
        }
        // Unreachable: the second call supplies the verdict the first one asked for, so `Verify`
        // cannot come back twice. Recorded rather than `unreachable!()`, because a panic here would
        // be a panic on network input (non-negotiable #3) for the sake of an invariant this
        // function can simply state instead.
        AckRoute::Verify { .. } => {
            tracing::error!(
                source = %arrival.source,
                "dropped an ACK: the token verdict was not consumed, which is a bug in this driver"
            );
            Ok(())
        }
    }
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
        405 => "Method Not Allowed",
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

    /// A request the test can hand to the refusal path.
    fn a_request(method: &Method) -> Request {
        sipx_sip::RequestBuilder::new(
            method.clone(),
            Uri::parse(Bytes::from_static(b"sip:bob@b.example")).unwrap(),
        )
        .header(HeaderName::CallId, "dp11")
        .and_then(|b| b.cseq(1, method))
        .and_then(|b| b.header(HeaderName::From, "<sip:alice@a.example>;tag=a"))
        .and_then(|b| b.header(HeaderName::To, "<sip:bob@b.example>"))
        .and_then(|b| b.header(HeaderName::Via, "SIP/2.0/UDP a.example;branch=z9hG4bK-1"))
        .map(sipx_sip::RequestBuilder::build)
        .expect("a well-formed request")
    }

    /// A builder failure invalidates the whole registrar response, for success and rejection.
    ///
    /// The injected line break cannot come from a parsed URI or option tag, but these public
    /// outcome types can also be populated by a future module. This pins the failure boundary:
    /// never send the original `200`/`420` with the one fact the builder refused silently absent.
    #[test]
    fn rg19_a_required_header_failure_becomes_a_bare_internal_response() {
        let request = a_request(&Method::Register);
        let bad_success = Outcome::Noop {
            response: Accepted {
                contacts: vec![ContactValue {
                    contact: Bytes::from_static(b"sip:alice@example.test\r\nInjected: yes"),
                    expires: 3600,
                    q: 1000,
                }],
                path: Vec::new(),
            },
        };
        assert!(matches!(
            render_register_outcome(&request, &bad_success),
            Err(RegisterRenderError::Header(_))
        ));
        let response = register_outcome_or_internal(&request, &bad_success, "test");
        assert_eq!(response.status.code(), 500);
        assert_eq!(response.headers.count(&HeaderName::Contact), 0);

        let bad_rejection = Outcome::Reject(Rejection::BadExtension(vec![
            "unknown\r\nInjected: yes".to_owned(),
        ]));
        assert!(matches!(
            render_register_outcome(&request, &bad_rejection),
            Err(RegisterRenderError::Header(_))
        ));
        let response = register_outcome_or_internal(&request, &bad_rejection, "test");
        assert_eq!(response.status.code(), 500);
        assert_eq!(response.headers.count(&HeaderName::Unsupported), 0);
    }

    /// The bound admits up to itself and then refuses — and a released permit is capacity again.
    #[test]
    fn dp11_the_bound_admits_up_to_itself_and_then_refuses() {
        let bound = Arc::new(AdmissionBound::new(2));
        let first = bound.admit(&Method::Invite);
        let second = bound.admit(&Method::Invite);
        assert!(matches!(first, Verdict::Admitted(_)));
        assert!(matches!(second, Verdict::Admitted(_)));
        assert_eq!(bound.in_flight(), 2);

        assert!(matches!(bound.admit(&Method::Invite), Verdict::Refused));
        assert_eq!(bound.refused(), 1);
        assert_eq!(bound.in_flight(), 2, "a refusal takes no permit");

        // A transaction that finished is capacity again — the bound is on concurrency, not on a
        // total. A permit that leaked here would turn a busy minute into a permanent refusal.
        drop(first);
        assert_eq!(bound.in_flight(), 1);
        assert!(matches!(bound.admit(&Method::Invite), Verdict::Admitted(_)));
        assert_eq!(bound.admitted(), 3);
    }

    /// REGISTER is never held behind the bound, however spent it is.
    ///
    /// The trap this pins: a registration storm *is* the overload, so a blanket cap would refuse the
    /// one request a node under load most needs to answer, and a refused refresh is a phone that
    /// becomes unreachable. `RG-14` bounds what one REGISTER costs; this bounds how many calls are
    /// resident, and neither may become the other.
    #[test]
    fn dp11_register_is_never_refused_by_the_bound() {
        let bound = Arc::new(AdmissionBound::new(1));
        let held = bound.admit(&Method::Invite);
        assert!(matches!(held, Verdict::Admitted(_)));
        assert!(matches!(bound.admit(&Method::Invite), Verdict::Refused));

        for _ in 0..1000 {
            assert!(
                matches!(bound.admit(&Method::Register), Verdict::Exempt),
                "a REGISTER must not wait behind proxied calls"
            );
        }
        assert_eq!(bound.in_flight(), 1, "an exempt method takes no permit");
        assert_eq!(bound.refused(), 1, "and is never counted as refused");
    }

    /// An ACK is exempt too, and for a harder reason: there is no response to an ACK in SIP, so
    /// "refusing" one can only mean dropping it — RFC 3261 §17.1.1.3 makes an ACK for a 2xx its own
    /// transaction with nothing to answer, and dropping it leaves a dialog no timer reaps. That is the
    /// leak the kernel counts apart as `ShedCounts::acks`, and this node must not add to it.
    #[test]
    fn dp11_an_ack_is_never_refused_because_it_cannot_be() {
        let bound = Arc::new(AdmissionBound::new(1));
        let _held = bound.admit(&Method::Invite);
        assert!(matches!(bound.admit(&Method::Ack), Verdict::Exempt));
    }

    /// The refusal is the kernel's own shape: `503` with `Retry-After`.
    #[test]
    fn dp11_a_refusal_is_a_503_with_retry_after() {
        let response = overloaded(&a_request(&Method::Invite));
        assert_eq!(response.status.code(), 503);
        let retry_after = response
            .headers
            .get(&HeaderName::RetryAfter)
            .expect("a Retry-After, as the kernel sends");
        assert_eq!(retry_after.value().as_ref(), RETRY_AFTER);
    }

    /// Losing the transport is a failure, and says so.
    ///
    /// The behaviour itself — `run` returning `Err` rather than `Ok(())` when `incoming.recv()` goes
    /// `None` — is the single `Err(NodeError::TransportGone)` at the end of the accept loop, which is
    /// now the only way out of it. It cannot be *induced* from outside `run`, because the handle that
    /// could shut the endpoint down never leaves it; what is pinned here is that the variant exists
    /// and reads as a failure, so a refactor cannot quietly make it a success again.
    #[test]
    fn dp11_a_lost_transport_reads_as_a_failure() {
        let message = NodeError::TransportGone.to_string();
        assert!(message.contains("stopped delivering"), "{message}");
        assert!(
            !message.contains("shutdown"),
            "this is not a shutdown, and must not read as one: {message}"
        );
    }

    /// Everything else on the proxy path is gated, including the methods that end work.
    ///
    /// Exempting BYE and CANCEL was considered — shedding what ends work makes overload
    /// self-sustaining — and rejected: an unbounded method is an unbounded node, and a `503` with
    /// `Retry-After` to a BYE is a retry, where an unbounded BYE flood is this story's defect wearing
    /// a different method name.
    #[test]
    fn dp11_the_methods_that_end_work_are_still_gated() {
        for method in [Method::Bye, Method::Cancel, Method::Options] {
            let bound = Arc::new(AdmissionBound::new(1));
            let _held = bound.admit(&Method::Invite);
            assert!(
                matches!(bound.admit(&method), Verdict::Refused),
                "{method:?} must be subject to the bound"
            );
        }
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

    // ------------------------------------------------------- roles reach dispatch (DP-13, §4 R3) ---

    const REGISTRAR_ONLY: Capabilities = Capabilities {
        registrar: true,
        proxy: false,
    };
    const PROXY_ONLY: Capabilities = Capabilities {
        registrar: false,
        proxy: true,
    };

    /// **`DP-13`.** A method reaches a path only where the node's roles wired one.
    ///
    /// The whole matrix, because the defect was one arm of it: `serve` matched on the method alone,
    /// so a node with no registrar wiring still ran `register` — accepted a REGISTER, wrote a binding
    /// and answered `200 OK`.
    #[test]
    fn dp13_dispatch_follows_the_wiring_and_not_the_method() {
        let of = Dispatch::of;

        // A registrar registers; without the wiring the same request is refused rather than served.
        assert_eq!(of(REGISTRAR_ONLY, &Method::Register), Dispatch::Registrar);
        assert_eq!(of(PROXY_ONLY, &Method::Register), Dispatch::NotAllowed);

        // A proxy proxies; a registrar is not a proxy (§4 R7 — a role's wiring is the union of the
        // sections its column marks, and there is no other way to acquire behaviour).
        for method in [Method::Invite, Method::Bye, Method::Cancel, Method::Options] {
            assert_eq!(of(PROXY_ONLY, &method), Dispatch::Proxy);
            assert_eq!(of(REGISTRAR_ONLY, &method), Dispatch::NotAllowed);
        }

        // Both paths on one node behave as either of them alone — R2's "roles is a set".
        assert_eq!(
            of(Capabilities::CALL_PATH, &Method::Register),
            Dispatch::Registrar
        );
        assert_eq!(
            of(Capabilities::CALL_PATH, &Method::Invite),
            Dispatch::Proxy
        );
    }

    /// An ACK is the one arrival a refusal cannot answer, so it is dropped rather than answered.
    ///
    /// RFC 3261 §17.1.1.3: an ACK for a 2xx is a transaction of its own, and nothing responds to it.
    /// A `405` here would put a response on a transaction that has none, which is a worse fault than
    /// the drop — so the refusal is a log line, and the drop is deliberate rather than incidental.
    #[test]
    fn dp13_an_ack_is_dropped_rather_than_answered_where_nothing_forwards_it() {
        assert_eq!(
            Dispatch::of(REGISTRAR_ONLY, &Method::Ack),
            Dispatch::Unroutable
        );
        assert_eq!(Dispatch::of(PROXY_ONLY, &Method::Ack), Dispatch::Ack);
    }

    /// The refusal is RFC 3261 §21.4.6's `405`, and it carries the methods the node does serve.
    #[test]
    fn dp13_a_refused_method_is_answered_405_with_allow() {
        let response = not_allowed(&a_request(&Method::Register), PROXY_ONLY);
        assert_eq!(response.status.code(), 405);
        let allow = response
            .headers
            .get(&HeaderName::Allow)
            .expect("§21.4.6 requires the methods that are allowed");
        assert_eq!(
            String::from_utf8_lossy(allow.value().as_ref()),
            "INVITE, ACK, BYE, CANCEL, OPTIONS"
        );

        // …and a registrar advertises the one method it has.
        assert_eq!(allowed(REGISTRAR_ONLY), "REGISTER");
        assert_eq!(
            allowed(Capabilities::CALL_PATH),
            "INVITE, ACK, BYE, CANCEL, OPTIONS, REGISTER"
        );
    }
}
