//! `CF-22` — a completed call leaves the node holding nothing.
//!
//! One node, one call, and then the question no other check in the gate asks: **does the
//! transaction store come back to zero?** RFC 3261 §17 keeps a concluded transaction alive for its
//! absorption timer — 64·T1, thirty-two seconds with the kernel's default constants — so that a
//! retransmission arriving after the final response is answered from the transaction rather than
//! delivered to the application twice. Asserting "empty immediately" would be asserting a bug. What
//! this asserts is that one absorption window later the store is **empty**, not merely smaller.
//!
//! **Why zero rather than "it went down".** `PX-13` passed `scripts/gate.sh` in its worktree,
//! passed it again on the integration branch, passed an independent review and passed the local
//! two-node call proof, and was merged. CI's `e2e` job then failed on
//! `scripts/e2e-call.sh`'s drain check with `outstanding=3 after 50s`, and the merge was reverted
//! (`a02cd7c`). The count on that run went 6 → 11 → 16 → 10 → 5 → 3 and stopped: a check that only
//! required the number to fall would have passed it.
//!
//! **Why here.** `scripts/e2e-call.sh` is the only thing that watched this, and `CF-15` made it a
//! separate CI job on purpose, so that a red there reads "the end-to-end call broke" rather than
//! "the gate is red" and so `gate.sh` stays runnable without a second checkout. That was a good
//! decision, and this is the hole it leaves: the one check that watches resource lifetime is the one
//! contributors never run. This harness observes the same counter in virtual time, with no Docker,
//! no PostgreSQL and no external `sipx` CLI, so thirty-two seconds of absorption cost nothing and
//! the check can sit in the gate.
//!
//! **What the node here is.** The kernel's own [`TransactionLayer`] — the same sans-IO state
//! machines `sipx-transport`'s endpoint drives behind `sipx-clstr-node`'s socket — with the real
//! forwarding engine above it and the real registrar beside it. `outstanding` is counted the way
//! the endpoint counts it for `Handle::outstanding()`: the transactions **plus** the per-transaction
//! bookkeeping the driver keeps beside them, because an entry that outlives its transaction is
//! exactly the leak a count of transactions alone would miss.
//!
//! Considered for upstream: **no.** The counter and the state machines are the kernel's and are
//! already exported; asserting that this platform's driver returns them to zero is orchestration.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_proxy::{
    BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig, ProxyTimer,
    ResponseContext, targets_from_lookup,
};
use sipx_clstr_registrar::{
    CanonicalAor, EdgeContext, InMemoryStore, LocationStore, TenantPolicy, Timestamp, apply,
    register_command,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::transaction::{Dispatch, Output};
use sipx_sip::{
    HeaderName, Message, Method, Reliability, Request, RequestBuilder, Response, ResponseBuilder,
    StatusCode, Timer, Timers, TransactionKey, TransactionLayer, TuEvent, Uri,
};

const EDGE: &str = "edge-1.example";
const TENANT: &str = "t1";
const CALL: &str = "cf22-the-call";

/// The value this node record-routes, and the identity it answers to.
const RECORD_ROUTE: &[u8] = b"<sip:edge-1.example;lr>";

/// Bob's device: a contact with an **explicit port**, which is a different location-service key
/// from his address of record (location-service §3 N7, RFC 3261 §19.1.4). That is what an ordinary
/// dialog's remote target looks like, and keeping it distinct from the AoR is what stops this
/// scenario proving anything by accident.
const BOB_CONTACT: &str = "sip:bob@10.0.0.1:5061";

/// Alice's device, on the same terms.
const ALICE_CONTACT: &str = "sip:alice@10.0.0.9:5062";

/// RFC 3261 §17's absorption window, 64·T1 with the kernel's default T1 of 500 ms.
///
/// Every timer that collects a concluded transaction — J for a non-INVITE server, L and M for the
/// RFC 6026 `Accepted` states, D for a client — is this long or shorter, so one window after the
/// last message of a call there is nothing left for §17 to collect.
const ABSORPTION: Duration = Duration::from_secs(32);

/// Slack on top of [`ABSORPTION`], so the assertion is not a race with the instant a timer fires.
///
/// T2, the retransmission ceiling. Deliberately small: the failure this exists to catch overran the
/// window by a **whole second window**, and a slack generous enough to hide that would be a check
/// that passes the thing it was written for.
const SLACK: Duration = Duration::from_secs(4);

/// How long the call itself is given. Everything in it is one hop on a clean link, so this is two
/// orders of magnitude more than it needs and still far below [`ABSORPTION`].
const CALL_WINDOW: Duration = Duration::from_secs(1);

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

fn header_of(message: &Message, name: &HeaderName) -> String {
    message
        .headers()
        .value(name)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// the node: the kernel's transaction layer, the real engine, the real registrar
// ---------------------------------------------------------------------------------------------

/// What the node is still holding, broken out so a red says which map rather than only "not zero".
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Outstanding {
    /// Client transactions the kernel's layer still holds.
    clients: usize,
    /// Server transactions, including the ones held only for their absorption timer.
    servers: usize,
    /// The driver's per-transaction map — where a transaction's messages go. The endpoint's
    /// `destinations`, and counted for the same reason it is: an entry that outlives its
    /// transaction is a leak a count of transactions alone cannot see.
    peers: usize,
}

impl Outstanding {
    fn total(self) -> usize {
        self.clients + self.servers + self.peers
    }
}

/// A timer this node has armed, in the two families it owns.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    /// RFC 3261 §17's, belonging to one transaction in the kernel's layer.
    Transaction(TransactionKey, Timer),
    /// §16.8's Timer C, belonging to one branch of one response context.
    TimerC(BranchId),
}

/// One edge node: registrar and proxy, over the kernel's transaction layer.
#[derive(Debug)]
struct Edge {
    name: String,
    layer: TransactionLayer,
    store: InMemoryStore,
    policy: TenantPolicy,
    /// Where a live transaction's messages go.
    peers: HashMap<TransactionKey, NodeId>,
    /// One response context per proxied request, keyed by `Call-ID`.
    contexts: HashMap<String, ResponseContext>,
    /// The server transaction each proxied request is answered on.
    upstream: HashMap<String, TransactionKey>,
    /// Which call a branch belongs to, and the client transaction carrying it.
    branch_call: HashMap<BranchId, String>,
    branch_key: HashMap<BranchId, TransactionKey>,
    key_branch: HashMap<TransactionKey, BranchId>,
    /// Which contact URI reaches which simulated node — the sim's stand-in for a socket address.
    reachable: HashMap<String, NodeId>,
    /// Armed timers, by the id the scheduler hands back.
    slots: Vec<Slot>,
    /// The current virtual time, captured on every input. A lookup is evaluated against it.
    now: Timestamp,
}

impl Edge {
    fn new(name: &str, reachable: HashMap<String, NodeId>) -> Self {
        Self {
            name: name.to_owned(),
            layer: TransactionLayer::new(Timers::default()),
            store: InMemoryStore::new(),
            policy: TenantPolicy::default(),
            peers: HashMap::new(),
            contexts: HashMap::new(),
            upstream: HashMap::new(),
            branch_call: HashMap::new(),
            branch_key: HashMap::new(),
            key_branch: HashMap::new(),
            reachable,
            slots: Vec::new(),
            now: Timestamp::ZERO,
        }
    }

    fn config() -> ProxyConfig {
        ProxyConfig::new(
            EDGE,
            Bytes::from_static(RECORD_ROUTE),
            CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
        )
    }

    /// What the node is still holding — the assertion's subject.
    fn outstanding(&self) -> Outstanding {
        let (clients, servers) = self.layer.len();
        Outstanding {
            clients,
            servers,
            peers: self.peers.len(),
        }
    }

    fn slot(&mut self, slot: &Slot) -> TimerId {
        if let Some(index) = self.slots.iter().position(|known| known == slot) {
            return TimerId(u64::try_from(index).unwrap_or(u64::MAX));
        }
        self.slots.push(slot.clone());
        TimerId(u64::try_from(self.slots.len() - 1).unwrap_or(u64::MAX))
    }

    // ------------------------------------------------------------- the transaction layer -------

    fn on_message(&mut self, from: NodeId, message: Message) -> Vec<Effect> {
        match self.layer.receive(message, Reliability::Unreliable) {
            Dispatch::Created { key, outputs } => {
                self.peers.insert(key.clone(), from);
                self.perform(&key, outputs, Some(from))
            }
            Dispatch::Matched { key, outputs } => self.perform(&key, outputs, Some(from)),
            // An ACK that matched nothing is an ACK for a 2xx whose transaction has gone; that is
            // ordinary and it belongs to the application. Anything else here is noise.
            Dispatch::Unmatched(message) => match *message {
                Message::Request(request) if request.method == Method::Ack => {
                    self.forward_ack(&request)
                }
                _ => Vec::new(),
            },
        }
    }

    /// Perform the layer's outputs, in order.
    fn perform(
        &mut self,
        key: &TransactionKey,
        outputs: Vec<Output>,
        origin: Option<NodeId>,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        for output in outputs {
            match output {
                Output::Send(message) => {
                    let Some(to) = self.peers.get(key).copied().or(origin) else {
                        continue;
                    };
                    out.push(send(to, *message));
                }
                Output::SetTimer { timer, after } => {
                    let id = self.slot(&Slot::Transaction(key.clone(), timer));
                    out.push(Effect::SetTimer { timer: id, after });
                }
                Output::ClearTimer(timer) => {
                    let id = self.slot(&Slot::Transaction(key.clone(), timer));
                    out.push(Effect::ClearTimer(id));
                }
                Output::ToTu(event) => out.extend(self.on_tu(key, *event, origin)),
                // The transaction is over. Everything keyed on it goes with it — which is the
                // whole subject of this file.
                Output::Terminated(_) => {
                    self.peers.remove(key);
                    if let Some(branch) = self.key_branch.remove(key) {
                        self.branch_key.remove(&branch);
                        self.branch_call.remove(&branch);
                    }
                    // Its timers go with it, the way the endpoint forgets them. A timer that
                    // fires for a transaction that is gone does nothing, but leaving one armed
                    // leaves the harness doing work no driver does.
                    for (index, slot) in self.slots.iter().enumerate() {
                        if matches!(slot, Slot::Transaction(armed, _) if armed == key) {
                            out.push(Effect::ClearTimer(TimerId(
                                u64::try_from(index).unwrap_or(u64::MAX),
                            )));
                        }
                    }
                }
            }
        }
        out
    }

    fn on_tu(
        &mut self,
        key: &TransactionKey,
        event: TuEvent,
        origin: Option<NodeId>,
    ) -> Vec<Effect> {
        match event {
            TuEvent::Request(request) => self.on_request(key, &request, origin),
            TuEvent::Ack(request) => self.forward_ack(&request),
            TuEvent::Response(response) => self.on_branch_response(key, *response),
            // The driver turns a kernel timeout into a `408` from that branch and a transport
            // failure into §16.9's branch failure. Both are recorded here as the branch failing:
            // what differs is the status the caller reads, and what this scenario measures is how
            // long the transaction lives.
            TuEvent::Timeout | TuEvent::TransportError => self.on_branch_failure(key),
        }
    }

    // ----------------------------------------------------------------- the application ---------

    fn on_request(
        &mut self,
        key: &TransactionKey,
        request: &Request,
        origin: Option<NodeId>,
    ) -> Vec<Effect> {
        if request.method == Method::Register {
            let (note, response) = self.register(request);
            let mut out = vec![Effect::Note(note)];
            out.extend(self.respond(key, response, origin));
            return out;
        }
        self.proxy(key, request)
    }

    fn register(&mut self, request: &Request) -> (String, Response) {
        let context = EdgeContext {
            tenant: TENANT.to_owned(),
            ..EdgeContext::default()
        };
        let command = match register_command(request, &context, self.now) {
            Ok(command) => command,
            Err(rejection) => {
                return (
                    format!("register refused {}", rejection.status()),
                    answer(request, rejection.status(), "Bad Request"),
                );
            }
        };
        let applied = apply(&self.store, &command, &self.policy, 3);
        let status = applied.outcome.status();
        (
            format!("registered {status}"),
            answer(request, status, "OK"),
        )
    }

    /// Answer a server transaction.
    fn respond(
        &mut self,
        key: &TransactionKey,
        response: Response,
        origin: Option<NodeId>,
    ) -> Vec<Effect> {
        let outputs = self.layer.send_response(key, response);
        self.perform(key, outputs, origin)
    }

    /// Forward an ACK for a 2xx: a request in its own right, with no transaction and no answer
    /// (RFC 3261 §17.1.1.3).
    ///
    /// Sent to the dialog's remote target, which is the Request-URI. The driver on `main` asks the
    /// **location service** instead and drops the ACK when the remote target is not a registered
    /// address of record — `V-03`, which is `PX-13`'s subject and not this file's. Modelling that
    /// defect here would make the two trees run *different calls*, and then the drain times would
    /// not be comparable, which is the one thing this scenario has to get right.
    fn forward_ack(&mut self, request: &Request) -> Vec<Effect> {
        let target = String::from_utf8_lossy(&request.uri.to_bytes()).into_owned();
        let Some(node) = self.reachable.get(&target).copied() else {
            return vec![Effect::Note(format!(
                "no address for the ACK's hop {target}"
            ))];
        };
        vec![
            Effect::Note("forward ack".to_owned()),
            send(node, Message::Request(request.clone())),
        ]
    }

    // --------------------------------------------------------------------- the proxy -----------

    fn proxy(&mut self, key: &TransactionKey, request: &Request) -> Vec<Effect> {
        let call = header_of(&Message::Request(request.clone()), &HeaderName::CallId);
        self.upstream.insert(call.clone(), key.clone());
        let mut context = ResponseContext::new(Self::config());
        let effects = context.on_input(ProxyInput::Upstream(Box::new(request.clone())));
        self.contexts.insert(call.clone(), context);
        self.drive(&call, effects)
    }

    fn on_branch_response(&mut self, key: &TransactionKey, response: Response) -> Vec<Effect> {
        let Some(branch) = self.key_branch.get(key).cloned() else {
            return Vec::new();
        };
        let Some(call) = self.branch_call.get(&branch).cloned() else {
            return Vec::new();
        };
        let Some(mut context) = self.contexts.remove(&call) else {
            return Vec::new();
        };
        let effects = context.on_input(ProxyInput::BranchResponse(Box::new(response), branch));
        if !context.is_finished() {
            self.contexts.insert(call.clone(), context);
        }
        self.drive(&call, effects)
    }

    fn on_branch_failure(&mut self, key: &TransactionKey) -> Vec<Effect> {
        let Some(branch) = self.key_branch.get(key).cloned() else {
            return Vec::new();
        };
        let Some(call) = self.branch_call.get(&branch).cloned() else {
            return Vec::new();
        };
        let Some(mut context) = self.contexts.remove(&call) else {
            return Vec::new();
        };
        let effects = context.on_input(ProxyInput::BranchTransportError(branch));
        if !context.is_finished() {
            self.contexts.insert(call.clone(), context);
        }
        self.drive(&call, effects)
    }

    /// Perform the engine's effects, in order.
    fn drive(&mut self, call: &str, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProxyEffect::ResolveTargets(query) => {
                    // §16.5 — the location service answers, and an empty set is the engine's `480`
                    // rather than the driver's.
                    let found = match CanonicalAor::parse(query.uri.clone()) {
                        Ok(aor) => self.store.lookup(TENANT, &aor, self.now),
                        Err(_) => Vec::new(),
                    };
                    let targets = targets_from_lookup(&found);
                    out.push(Effect::Note(format!(
                        "looked up {} target(s)",
                        targets.len()
                    )));
                    let Some(mut context) = self.contexts.remove(call) else {
                        continue;
                    };
                    let more = context.on_input(ProxyInput::TargetsResolved(targets));
                    if !context.is_finished() {
                        self.contexts.insert(call.to_owned(), context);
                    }
                    out.extend(self.drive(call, more));
                }
                // `..` because the engine's hop is `PX-13`'s to add: this file has to compile
                // against the tree with that branch and against the tree without it, or the
                // failing-first proof compares two different tests.
                ProxyEffect::Forward {
                    branch,
                    request,
                    target,
                    ..
                } => {
                    let hop = String::from_utf8_lossy(&target.uri).into_owned();
                    let Some(node) = self.reachable.get(&hop).copied() else {
                        out.push(Effect::Note(format!("no address for {hop}")));
                        continue;
                    };
                    let Some((key, outputs)) =
                        self.layer.send_request(*request, Reliability::Unreliable)
                    else {
                        continue;
                    };
                    self.peers.insert(key.clone(), node);
                    self.branch_key.insert(branch.clone(), key.clone());
                    self.key_branch.insert(key.clone(), branch.clone());
                    self.branch_call.insert(branch.clone(), call.to_owned());
                    out.push(Effect::Note(format!("forward to {hop}")));
                    out.extend(self.perform(&key, outputs, Some(node)));
                }
                ProxyEffect::Respond(response) => {
                    let Some(key) = self.upstream.get(call).cloned() else {
                        continue;
                    };
                    out.push(Effect::Note(format!("respond {}", response.status.code())));
                    out.extend(self.respond(&key, *response, None));
                }
                ProxyEffect::SetTimer {
                    timer: ProxyTimer::C,
                    branch: Some(branch),
                    after,
                } => {
                    let id = self.slot(&Slot::TimerC(branch));
                    out.push(Effect::SetTimer { timer: id, after });
                }
                ProxyEffect::ClearTimer {
                    timer: ProxyTimer::C,
                    branch: Some(branch),
                } => {
                    let id = self.slot(&Slot::TimerC(branch));
                    out.push(Effect::ClearTimer(id));
                }
                ProxyEffect::Terminate => {
                    self.contexts.remove(call);
                    self.upstream.remove(call);
                    out.push(Effect::Note("terminate".to_owned()));
                }
                ProxyEffect::CancelBranch(_)
                | ProxyEffect::AnswerCancel
                | ProxyEffect::SetTimer { .. }
                | ProxyEffect::ClearTimer { .. } => {}
            }
        }
        out
    }

    // ----------------------------------------------------------------------- timers ------------

    fn on_timer(&mut self, timer: TimerId) -> Vec<Effect> {
        let Some(index) = usize::try_from(timer.0).ok() else {
            return Vec::new();
        };
        let Some(slot) = self.slots.get(index).cloned() else {
            return Vec::new();
        };
        match slot {
            Slot::Transaction(key, timer) => {
                let outputs = self.layer.on_timer(&key, timer);
                self.perform(&key, outputs, None)
            }
            Slot::TimerC(branch) => {
                let Some(call) = self.branch_call.get(&branch).cloned() else {
                    return Vec::new();
                };
                let Some(mut context) = self.contexts.remove(&call) else {
                    return Vec::new();
                };
                let effects = context.on_input(ProxyInput::TimerFired(ProxyTimer::C, Some(branch)));
                if !context.is_finished() {
                    self.contexts.insert(call.clone(), context);
                }
                self.drive(&call, effects)
            }
        }
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        self.now = Timestamp::from_nanos(now.as_nanos());
        match input {
            Input::Message { from, message } => self.on_message(from, message.clone()),
            Input::Timer(timer) => self.on_timer(timer),
            Input::Started | Input::TransportError { .. } => Vec::new(),
        }
    }
}

fn answer(request: &Request, status: u16, reason: &str) -> Response {
    ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason.to_owned(),
    )
    .expect("a response")
    .build()
}

// ---------------------------------------------------------------------------------------------
// the devices
// ---------------------------------------------------------------------------------------------

/// The caller. Registers, calls bob, acknowledges his `200` — and is then **gone**.
///
/// Gone is the point. `scripts/e2e-call.sh` gives `sipx dial` a `--duration`, and when it elapses
/// the process exits; the callee's `BYE` then reaches a socket nobody is reading. That is the run
/// CI failed on, and it is the ordinary shape of a hang-up race rather than an exotic one.
#[derive(Debug)]
struct Caller {
    name: String,
    edge: NodeId,
    registered: bool,
    /// The `Record-Route` set the `200` carried — her route set (RFC 3261 §12.1.2).
    route_set: Vec<String>,
    /// The `Contact` the `200` carried — the dialog's remote target.
    remote_target: String,
    answered: Option<u16>,
    acknowledged: bool,
    /// Once she has acknowledged, her device is gone and answers nothing.
    gone: bool,
    cseq: u32,
}

impl SimNode for Caller {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        if self.gone {
            return Vec::new();
        }
        match input {
            Input::Started => {
                let register = self.register();
                vec![send(self.edge, register)]
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            _ => Vec::new(),
        }
    }
}

impl Caller {
    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let cseq = header_of(&Message::Response(response.clone()), &HeaderName::CSeq);
        if cseq.ends_with("REGISTER") {
            if !response.status.is_success() || self.registered {
                return Vec::new();
            }
            self.registered = true;
            let invite = self.invite();
            return vec![
                Effect::Note(format!("{} registered", self.name)),
                send(self.edge, invite),
            ];
        }
        if response.status.is_provisional() {
            return Vec::new();
        }
        if self.answered.is_some() {
            return Vec::new();
        }
        self.answered = Some(response.status.code());
        if !response.status.is_success() {
            return vec![Effect::Note(format!(
                "{} got {}",
                self.name,
                response.status.code()
            ))];
        }
        self.route_set = response
            .headers
            .get_all(&HeaderName::RecordRoute)
            .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
            .collect();
        self.remote_target = bare_uri(&header_of(
            &Message::Response(response.clone()),
            &HeaderName::Contact,
        ));
        let ack = self.ack(response);
        self.acknowledged = true;
        // The dialog is up and acknowledged, and this device now goes away.
        self.gone = true;
        vec![
            Effect::Note(format!("{} got 200 and acknowledged", self.name)),
            send(self.edge, ack),
        ]
    }

    fn register(&mut self) -> Message {
        self.cseq += 1;
        Message::Request(
            register_for("alice", ALICE_CONTACT, self.cseq, &self.name)
                .max_forwards(70)
                .build(),
        )
    }

    fn invite(&mut self) -> Message {
        self.cseq += 1;
        Message::Request(
            RequestBuilder::new(Method::Invite, uri("sip:bob@atlanta.example"))
                .header(HeaderName::CallId, CALL)
                .expect("Call-ID")
                .cseq(self.cseq, &Method::Invite)
                .expect("CSeq")
                .header(HeaderName::From, "<sip:alice@atlanta.example>;tag=alice")
                .expect("From")
                .header(HeaderName::To, "<sip:bob@atlanta.example>")
                .expect("To")
                .header(
                    HeaderName::Via,
                    format!(
                        "SIP/2.0/UDP {}.sim;branch=z9hG4bK-c{}",
                        self.name, self.cseq
                    ),
                )
                .expect("Via")
                .header(HeaderName::Contact, format!("<{ALICE_CONTACT}>"))
                .expect("Contact")
                .max_forwards(70)
                .build(),
        )
    }

    /// The `ACK` for the `2xx`, per RFC 3261 §13.2.2.4: the Request-URI is the dialog's remote
    /// target and the `Route` is the route set the `Record-Route` established.
    fn ack(&self, response: &Response) -> Message {
        let to = header_of(&Message::Response(response.clone()), &HeaderName::To);
        let mut builder = RequestBuilder::new(Method::Ack, uri(&self.remote_target))
            .header(HeaderName::CallId, CALL)
            .expect("Call-ID")
            .cseq(self.cseq, &Method::Ack)
            .expect("CSeq")
            .header(HeaderName::From, "<sip:alice@atlanta.example>;tag=alice")
            .expect("From")
            .header(HeaderName::To, to)
            .expect("To")
            .header(
                HeaderName::Via,
                format!(
                    "SIP/2.0/UDP {}.sim;branch=z9hG4bK-a{}",
                    self.name, self.cseq
                ),
            )
            .expect("Via")
            .max_forwards(70);
        for route in &self.route_set {
            builder = builder
                .header(HeaderName::Route, route.clone())
                .expect("Route");
        }
        Message::Request(builder.build())
    }
}

/// The callee. Registers, answers the call, and hangs up when the `ACK` arrives.
#[derive(Debug)]
struct Callee {
    name: String,
    edge: NodeId,
    registered: bool,
    /// The `Record-Route` list as it arrived on the `INVITE` — his route set (§12.1.1).
    route_set: Vec<String>,
    /// The caller's `Contact` — his remote target.
    remote_target: String,
    acknowledged: bool,
    hung_up: bool,
    /// The final status his `BYE` was answered with, if it ever was.
    bye_answered: Option<u16>,
    cseq: u32,
}

impl SimNode for Callee {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let register = self.register();
                vec![send(self.edge, register)]
            }
            Input::Message {
                from,
                message: Message::Request(request),
            } => self.on_request(from, request),
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            _ => Vec::new(),
        }
    }
}

impl Callee {
    fn on_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        match request.method {
            Method::Invite => {
                self.route_set = request
                    .headers
                    .get_all(&HeaderName::RecordRoute)
                    .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
                    .collect();
                self.remote_target = bare_uri(&header_of(
                    &Message::Request(request.clone()),
                    &HeaderName::Contact,
                ));
                let mut ok = answer(request, 200, "OK");
                // §12.1.1: a UAS echoes the `Record-Route` set into its 2xx, which is how the
                // caller learns her route set, and offers its own `Contact` as the remote target.
                for route in &self.route_set {
                    if let Ok(header) =
                        sipx_sip::Header::build(HeaderName::RecordRoute, route.clone())
                    {
                        ok.headers.push(header);
                    }
                }
                if let Ok(header) =
                    sipx_sip::Header::build(HeaderName::Contact, format!("<{BOB_CONTACT}>"))
                {
                    ok.headers.push(header);
                }
                if let Ok(header) = sipx_sip::Header::build(HeaderName::To, tagged_to(request)) {
                    ok.headers.remove_all(&HeaderName::To);
                    ok.headers.push(header);
                }
                vec![
                    Effect::Note(format!("{} answers 200", self.name)),
                    send(from, Message::Response(ok)),
                ]
            }
            Method::Ack => {
                if self.acknowledged {
                    return Vec::new();
                }
                self.acknowledged = true;
                self.hung_up = true;
                let bye = self.bye();
                vec![
                    Effect::Note(format!("{} acknowledged, hanging up", self.name)),
                    send(self.edge, bye),
                ]
            }
            _ => Vec::new(),
        }
    }

    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let cseq = header_of(&Message::Response(response.clone()), &HeaderName::CSeq);
        if cseq.ends_with("REGISTER") {
            self.registered = response.status.is_success();
            return vec![Effect::Note(format!("{} registered", self.name))];
        }
        if cseq.ends_with("BYE") && !response.status.is_provisional() {
            self.bye_answered = Some(response.status.code());
            return vec![Effect::Note(format!(
                "{} got {} for its BYE",
                self.name,
                response.status.code()
            ))];
        }
        Vec::new()
    }

    fn register(&mut self) -> Message {
        self.cseq += 1;
        Message::Request(
            register_for("bob", BOB_CONTACT, self.cseq, &self.name)
                .max_forwards(70)
                .build(),
        )
    }

    /// The `BYE`, from the other side of the same dialog: the Request-URI is the caller's
    /// `Contact` and the `Route` is the route set the `Record-Route` gave him.
    fn bye(&mut self) -> Message {
        self.cseq += 1;
        let mut builder = RequestBuilder::new(Method::Bye, uri(&self.remote_target))
            .header(HeaderName::CallId, CALL)
            .expect("Call-ID")
            .cseq(self.cseq, &Method::Bye)
            .expect("CSeq")
            .header(HeaderName::From, "<sip:bob@atlanta.example>;tag=bob")
            .expect("From")
            .header(HeaderName::To, "<sip:alice@atlanta.example>;tag=alice")
            .expect("To")
            .header(
                HeaderName::Via,
                format!(
                    "SIP/2.0/UDP {}.sim;branch=z9hG4bK-b{}",
                    self.name, self.cseq
                ),
            )
            .expect("Via")
            .max_forwards(70);
        for route in &self.route_set {
            builder = builder
                .header(HeaderName::Route, route.clone())
                .expect("Route");
        }
        Message::Request(builder.build())
    }
}

fn register_for(user: &str, contact: &str, cseq: u32, device: &str) -> RequestBuilder {
    RequestBuilder::new(Method::Register, uri("sip:atlanta.example"))
        .header(HeaderName::CallId, format!("cf22-reg-{device}"))
        .expect("Call-ID")
        .cseq(cseq, &Method::Register)
        .expect("CSeq")
        .header(
            HeaderName::From,
            format!("<sip:{user}@atlanta.example>;tag=r{device}"),
        )
        .expect("From")
        .header(HeaderName::To, format!("<sip:{user}@atlanta.example>"))
        .expect("To")
        .header(
            HeaderName::Via,
            format!("SIP/2.0/UDP {device}.sim;branch=z9hG4bK-r{cseq}{device}"),
        )
        .expect("Via")
        .header(HeaderName::Contact, format!("<{contact}>;expires=3600"))
        .expect("Contact")
}

/// The request's `To` with a tag, so the `2xx` establishes a dialog.
fn tagged_to(request: &Request) -> String {
    let to = header_of(&Message::Request(request.clone()), &HeaderName::To);
    if to.contains(";tag=") {
        to
    } else {
        format!("{to};tag=bob")
    }
}

fn bare_uri(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_start_matches('<')
        .split('>')
        .next()
        .unwrap_or(value)
        .trim()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// the scenario
// ---------------------------------------------------------------------------------------------

const EDGE_NODE: NodeId = NodeId::from_index(0);
const ALICE: NodeId = NodeId::from_index(1);
const BOB: NodeId = NodeId::from_index(2);

fn scenario(seed: u64) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);

    let mut reachable = HashMap::new();
    reachable.insert(BOB_CONTACT.to_owned(), BOB);
    reachable.insert(ALICE_CONTACT.to_owned(), ALICE);

    sim.add_node(Box::new(Edge::new("edge", reachable)));
    sim.add_node(Box::new(Caller {
        name: "alice".to_owned(),
        edge: EDGE_NODE,
        registered: false,
        route_set: Vec::new(),
        remote_target: String::new(),
        answered: None,
        acknowledged: false,
        gone: false,
        cseq: 0,
    }));
    sim.add_node(Box::new(Callee {
        name: "bob".to_owned(),
        edge: EDGE_NODE,
        registered: false,
        route_set: Vec::new(),
        remote_target: String::new(),
        acknowledged: false,
        hung_up: false,
        bye_answered: None,
        cseq: 0,
    }));
    sim
}

#[test]
fn the_transaction_store_returns_to_zero_after_a_call() {
    let mut sim = scenario(0x4346_2201);

    // The call itself: two registrations, an INVITE, a 200, an ACK, and a hang-up.
    sim.advance(CALL_WINDOW).expect("the call runs");

    // The drain assertion is only worth anything if the call actually happened, so each step of it
    // is asserted before the store is looked at. A scenario that quietly stopped after the
    // REGISTER would otherwise "drain" perfectly.
    let alice = sim.node::<Caller>(ALICE).expect("alice is in the sim");
    let bob = sim.node::<Callee>(BOB).expect("bob is in the sim");
    assert_eq!(
        alice.answered,
        Some(200),
        "the call was not answered\n{}",
        sim.trace().render()
    );
    assert!(
        alice.acknowledged,
        "the caller never acknowledged the 200\n{}",
        sim.trace().render()
    );
    assert!(
        bob.acknowledged && bob.hung_up,
        "the callee never saw the ACK and hung up\n{}",
        sim.trace().render()
    );

    // One absorption window on top, which is what RFC 3261 §17 asks for and all it asks for.
    sim.advance(ABSORPTION + SLACK).expect("the store drains");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge is in the sim");
    let held = edge.outstanding();
    assert_eq!(
        held.total(),
        0,
        "the node still holds {} transaction(s) {:?} after the call ended — \
         {} client, {} server, {} per-transaction entries. RFC 3261 §17's absorption window is \
         64·T1 = {:?}, and everything a completed call leaves behind is collected inside it.\n{}",
        held.total(),
        ABSORPTION + SLACK,
        held.clients,
        held.servers,
        held.peers,
        ABSORPTION,
        sim.trace().render()
    );
}

/// The same store, looked at **before** the window: it is deliberately not empty.
///
/// Without this the assertion above would pass just as well against a node that tore a transaction
/// down the instant it answered — which is the bug §17 exists to prevent, because a retransmission
/// arriving afterwards would then be delivered to the application a second time.
#[test]
fn the_store_is_deliberately_not_empty_before_the_window() {
    let mut sim = scenario(0x4346_2202);
    sim.advance(CALL_WINDOW).expect("the call runs");

    let edge = sim.node::<Edge>(EDGE_NODE).expect("the edge is in the sim");
    let held = edge.outstanding();
    assert!(
        held.total() > 0,
        "a concluded transaction is held for its absorption timer; finding none a second after \
         the call means §17's window is not being kept\n{}",
        sim.trace().render()
    );
}
