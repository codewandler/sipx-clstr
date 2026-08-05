//! `CF-22` — a completed call leaves the node holding nothing, and an unbounded hold fails.
//!
//! One node, one call, and then the question no other check in the gate asks: **does the transaction
//! accounting come back to zero?** RFC 3261 §17 keeps a concluded transaction alive for its
//! absorption timer, so that a retransmission arriving after the final response is answered from the
//! transaction rather than delivered to the application twice. Asserting "empty immediately" would
//! be asserting a bug. What this asserts is that the **worst case the RFC actually permits** later,
//! the store is **empty** — not merely smaller.
//!
//! # The bound is 128·T1, and that took a reverted merge to establish
//!
//! It is tempting to write 64·T1 here, because that is the number in every absorption timer. It is
//! wrong for a **proxied** request, and the check that says 64·T1 rejects correct code. A proxy
//! forwarding to a next hop that never answers spends two windows end to end:
//!
//! - §17.1.2.2 — Timer F = 64·T1 on the non-INVITE **client** transaction (Timer B, likewise 64·T1,
//!   for an INVITE). §16.8 confines Timer C to INVITE, so for a non-INVITE no proxy-level timer can
//!   conclude the branch any sooner.
//! - §16.7 — the proxy MUST forward the *best* final response, and there is none until the branch
//!   concludes. Until then the **server** transaction sits in Trying/Proceeding, which §17.2.2 gives
//!   **no timer at all**. Nothing is wrong and nothing can be collected.
//! - §17.2.2 — Timer J = 64·T1 (Timer H for an INVITE) starts only *when the final response is
//!   sent*, which is after the first window has already elapsed.
//!
//! The pinned kernel implements exactly that — `timing.rs:69` `timeout() = t1 * 64`, `:100`
//! `timer_j(Unreliable) = timeout()` — and [`the_worst_case_the_rfc_permits_is_inside_the_bound`]
//! measures it here rather than taking it on faith: still held at 127·T1, empty by 128·T1 + slack.
//!
//! `PX-13` was reverted on this mistake. Its end-to-end drain took 64.000 s = 128·T1, and
//! `scripts/e2e-call.sh` waited 50 s — strictly between one window and two — so a **correct** fix
//! read as a leak. The threshold there is corrected by this story; the number here is the same
//! number, derived once.
//!
//! # What "leak" means, then
//!
//! Not "slow". **Unbounded** — a transaction no §17 timer will ever collect.
//! [`a_hold_no_timer_collects_is_caught`] injects the real shape of one: a server transaction the
//! application never answers, which §17.2.2 leaves in Trying with no timer, held for the life of the
//! process. The kernel's own endpoint carries a backstop for it (`abandon_unanswered`), documented
//! as *"a backstop against never, not a deadline"*. That is the distinction this file exists to
//! draw, and it is the distinction the first version of it got wrong.
//!
//! # Why here
//!
//! `scripts/e2e-call.sh` is the only thing that watched this, and `CF-15` made it a separate CI job
//! on purpose, so that a red there reads "the end-to-end call broke" rather than "the gate is red"
//! and so `gate.sh` stays runnable without a second checkout. That was a good decision, and this is
//! the hole it leaves: the one check that watches resource lifetime is the one contributors never
//! run. This harness watches the same accounting in virtual time, with no Docker, no PostgreSQL and
//! no external `sipx` CLI, so sixty-four seconds of absorption cost nothing.
//!
//! # What the node here is
//!
//! The kernel's own [`TransactionLayer`] — the same sans-IO state machines `sipx-transport`'s
//! endpoint drives behind `sipx-clstr-node`'s socket — with the real forwarding engine above it and
//! the real registrar beside it. See [`Outstanding`] for exactly what is counted, and for the one
//! part of the endpoint's own count this cannot reproduce.
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

/// Bob's device, as a dialog's remote target: a `Contact`, at the device's own host.
///
/// **The host is what makes it a different location-service key** from his address of record — a
/// canonical AoR carries the host, and `10.0.0.1` is not `atlanta.example` (location-service §3,
/// RFC 3261 §19.1.4). The explicit port is belt to that brace (N7 makes an explicit port and an
/// absent one distinct too), not the mechanism. Either way this is what an ordinary call's remote
/// target looks like, and keeping it off the AoR is what stops the scenario resolving by accident.
const BOB_CONTACT: &str = "sip:bob@10.0.0.1:5061";

/// Alice's device, on the same terms.
const ALICE_CONTACT: &str = "sip:alice@10.0.0.9:5062";

/// T1 — the kernel's default round-trip estimate (`Timers::default`), and the unit every timer in
/// RFC 3261 §17 is a multiple of. Every duration below is written in terms of it, so that the
/// numbers in the assertions are the RFC's numbers rather than seconds someone once measured.
const T1: Duration = Duration::from_millis(500);

/// One absorption window: 64·T1 — 32 s at the default T1.
const ABSORPTION: Duration = T1.saturating_mul(64);

/// The bound the check holds the node to: **two** absorption windows, 128·T1.
///
/// Not one. See this file's header: a proxied request to a silent next hop spends Timer F (or B) on
/// the client transaction *before* §16.7 lets a final response go upstream, and only then does
/// Timer J (or H) start on the server transaction. Both are 64·T1 and they are strictly sequential,
/// so 128·T1 is the worst case RFC 3261 permits — not a slack allowance, a derived number.
/// `PX-13` was reverted for taking 64.000 s against a threshold of 50 s.
///
/// Written as the derivation rather than as `64`, so that the two-windows claim is in the code and
/// not only in the prose above it.
const BOUND: Duration = ABSORPTION.saturating_mul(2);

/// Slack on top of [`BOUND`], so the assertion is not a race with the instant a timer fires.
///
/// T2, the retransmission ceiling — 8·T1. Deliberately small: virtual time is exact, so this only
/// has to clear the instant a timer fires, and a slack large enough to hide a third window would
/// defeat the check.
const SLACK: Duration = T1.saturating_mul(8);

/// How long the call itself is given. Every message in it is one hop on a clean link and lands at
/// `t = 0`, so this is only the point at which the scenario's own preconditions are checked.
const CALL_WINDOW: Duration = Duration::from_secs(1);

/// How far past [`BOUND`] the leak test looks before calling a hold unbounded rather than slow.
///
/// Ten windows. Nothing in §17 runs longer than 64·T1, and the one proxy-level timer that does —
/// Timer C, INVITE-only, cleared when a branch concludes, 240 s by default — is inside this too.
const LONG_AFTER: Duration = ABSORPTION.saturating_mul(20);

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
///
/// **This is the harness's own accounting, not a reading of `Handle::outstanding()`** — that one
/// needs a socket. It is counted on the same principle: the kernel layer's transactions **plus** the
/// per-transaction bookkeeping the driver keeps beside them, because an entry that outlives its
/// transaction is exactly the leak a count of transactions alone cannot see.
///
/// One term of the endpoint's own sum is deliberately absent, and it is worth knowing which.
/// `sipx-transport` also counts `handed_over` — one entry per request given to the application,
/// cleared when the transaction terminates — which is how `Handle::outstanding()` reports three
/// entries for one held server transaction where this reports two. There is nothing to mirror it
/// with: `sipx-clstr-node` keeps a proxied request's context in a **per-request task**
/// (`driver.rs:1153`), not in a map, so the analogue here would be an invention rather than a model.
/// The consequence is a difference in the *number*, never in whether it is zero, and zero is the
/// whole assertion.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Outstanding {
    /// Client transactions the kernel's layer still holds.
    clients: usize,
    /// Server transactions, including the ones held only for their absorption timer.
    servers: usize,
    /// The driver's per-transaction map — where a transaction's messages go. The endpoint's
    /// `destinations`.
    peers: usize,
}

impl Outstanding {
    fn total(self) -> usize {
        self.clients + self.servers + self.peers
    }

    /// The breakdown, for an assertion message that says which resource.
    fn describe(self) -> String {
        format!(
            "{} client, {} server, {} per-transaction entries",
            self.clients, self.servers, self.peers
        )
    }
}

/// A hold injected into the node, so that "unbounded" can be tested rather than argued.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Hold {
    /// Nothing injected. The node behaves.
    None,
    /// A server transaction for this method is created and then **never answered and never
    /// forwarded**.
    ///
    /// This is what an unbounded hold actually looks like, and it is not hypothetical: RFC 3261
    /// §17.2.2 gives a server transaction in Trying no timer at all, because its model is that the
    /// transaction user always responds. An application that drops a method it does not implement,
    /// or that wedges in a handler, leaves exactly this — and nothing in §17 ever collects it, so
    /// the store grows for as long as traffic arrives. The kernel's endpoint carries a backstop for
    /// it (`abandon_unanswered`), which documents itself as "a backstop against never, not a
    /// deadline"; this harness has no such backstop, which is what makes the hold visible here.
    NeverAnswer(Method),
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
    /// What this node has been told to hold on to, if anything.
    hold: Hold,
    /// The current virtual time, captured on every input. A lookup is evaluated against it.
    now: Timestamp,
}

impl Edge {
    fn new(name: &str, reachable: HashMap<String, NodeId>, hold: Hold) -> Self {
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
            hold,
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
        // The injected hold, and it is injected *here* — at the seam where a real application
        // decides what to do with a request — rather than by reaching into the layer, because the
        // hold this models is an application that never answers, not a broken state machine.
        if self.hold == Hold::NeverAnswer(request.method.clone()) {
            return vec![Effect::Note(format!(
                "swallowed {}: nothing will answer this transaction",
                request.method
            ))];
        }
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
                    // §7 L8 — a lookup is fallible, and this harness runs on the in-memory
                    // backend, whose reads cannot fail. The failure input the socket driver feeds
                    // (`ProxyInput::TargetsUnavailable`) is proved by `PB-F-11`.
                    let found = match CanonicalAor::parse(query.uri.clone()) {
                        Ok(aor) => self
                            .store
                            .lookup(TENANT, &aor, self.now)
                            .expect("the in-memory backend always reads"),
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
                // `..`, and it hides `next_hop` — which is `PX-13`'s field, absent on `main`, so a
                // file that named it could not run against both trees and the story's "`PX-13` is
                // green" could not be demonstrated. **What that costs is real and is guarded
                // below**: the target is what goes into the Request-URI and the hop is where the
                // copy is sent, and the two differ for every surviving `Route` and every registered
                // `Path`. Resolving `target.uri` is exactly the mistake `next_hop` exists to
                // prevent, so rather than rely on this scenario never growing a route set, the
                // condition under which the two are the same is asserted rather than assumed.
                ProxyEffect::Forward {
                    branch,
                    request,
                    target,
                    ..
                } => {
                    assert!(
                        request.headers.get(&HeaderName::Route).is_none(),
                        "a forwarded request still carries a Route, so its next hop is that route \
                         and not `{}`. This harness resolves `target.uri` because it must compile \
                         against a tree without `Effect::Forward::next_hop`; the moment a scenario \
                         here has a surviving Route, that shortcut routes wrong. Read the hop, or \
                         keep the scenario route-free.",
                        String::from_utf8_lossy(&target.uri)
                    );
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
                // This scenario mints no affinity token, so no `Route` in it carries one and P2
                // never asks. A note rather than a silent arm: if it ever does fire here, the
                // context is waiting for a verdict nothing will send, which is exactly the held
                // transaction this file exists to catch — and the trace should say why.
                ProxyEffect::VerifyToken { .. } => {
                    out.push(Effect::Note("unexpected token verification".to_owned()));
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
            Input::Started | Input::TransportError { .. }
            // `CF-26`'s connection faults: this node keeps no connection table and no
            // write accounting, so a reconnect, a restart and a stall are nothing to it.
            | Input::Connected { .. }
            | Input::Restarted { .. }
            | Input::WriteStalled { .. }
            | Input::WriteFlushed { .. } => Vec::new(),
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
    /// Whether he answers the `INVITE` at all.
    ///
    /// A device that registers and then goes silent is the ordinary way a proxy meets RFC 3261's
    /// worst case: the branch produces nothing, so the client transaction has to run Timer B to
    /// 64·T1 before §16.7 has a final response to forward, and only then does the server
    /// transaction start its own. Not exotic — a phone on a dropped network looks exactly like it.
    answers: bool,
    registered: bool,
    /// The `Record-Route` list as it arrived on the `INVITE` — his route set (§12.1.1).
    route_set: Vec<String>,
    /// The caller's `Contact` — his remote target.
    remote_target: String,
    /// Whether the `ACK` reached him — which is also "has hung up", because the `BYE` goes out on
    /// the same input. One flag rather than two: they could never disagree, and a second bool that
    /// is always equal to the first is a state machine pretending to be two.
    acknowledged: bool,
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
            Method::Invite if !self.answers => {
                vec![Effect::Note(format!("{} is silent", self.name))]
            }
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

fn scenario(seed: u64, answers: bool, hold: Hold) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);

    let mut reachable = HashMap::new();
    reachable.insert(BOB_CONTACT.to_owned(), BOB);
    reachable.insert(ALICE_CONTACT.to_owned(), ALICE);

    sim.add_node(Box::new(Edge::new("edge", reachable, hold)));
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
        answers,
        registered: false,
        route_set: Vec::new(),
        remote_target: String::new(),
        acknowledged: false,
        bye_answered: None,
        cseq: 0,
    }));
    sim
}

/// A call that is answered, acknowledged and hung up — and asserted to have been, because a drain
/// assertion over a scenario that quietly stopped after the REGISTER would pass perfectly.
fn a_completed_call(seed: u64) -> Sim {
    let mut sim = scenario(seed, true, Hold::None);
    sim.advance(CALL_WINDOW).expect("the call runs");

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
        bob.acknowledged,
        "the callee never saw the ACK and hung up\n{}",
        sim.trace().render()
    );
    sim
}

/// A virtual instant this many after the scenario began.
///
/// Every scenario here starts its call at `t = 0` — one hop, clean link, no latency — so an offset
/// from the start *is* an offset from the call, and every deadline below can be an absolute instant.
/// That matters: a probe expressed as "advance by [`BOUND`]" moves when `BOUND` moves, so it can
/// never catch `BOUND` being wrong, which is the mistake that parked the first version of this file.
fn at(offset: Duration) -> SimTime {
    SimTime::from_nanos(u64::try_from(offset.as_nanos()).unwrap_or(u64::MAX))
}

/// What the node is holding right now.
fn held_by(sim: &Sim) -> Outstanding {
    sim.node::<Edge>(EDGE_NODE)
        .expect("the edge is in the sim")
        .outstanding()
}

/// **The check.** Hold the node to [`BOUND`] and require the accounting to be empty.
///
/// One function, used by every test below and by the gate's `transaction drain` step, so that the
/// assertion which passes for a well-behaved node is character-for-character the one that fails for
/// a leaking one. `what` names the scenario, so the message says what was being drained rather than
/// only that something was not.
fn assert_drains_within_the_bound(sim: &mut Sim, what: &str) {
    sim.run_until(at(BOUND + SLACK)).expect("the sim runs");
    let held = held_by(sim);
    assert_eq!(
        held.total(),
        0,
        "after {what}, the node still holds {} — {}. RFC 3261's worst case for a proxied request \
         to a silent next hop is 128·T1 = {:?} (Timer F or B on the client transaction, and only \
         then Timer J or H on the server transaction, because §16.7 has no final response to \
         forward until the branch concludes and §17.2.2 gives Trying no timer); this waited {:?}. \
         Something is being held that no §17 timer will collect.\n{}",
        held.total(),
        held.describe(),
        BOUND,
        BOUND + SLACK,
        sim.trace().render()
    );
}

/// The gate's headline: a call that completes leaves the node holding nothing.
#[test]
fn a_completed_call_leaves_the_node_holding_nothing() {
    let mut sim = a_completed_call(0x4346_2201);
    assert_drains_within_the_bound(&mut sim, "a completed call");
}

/// The store one second after the call: deliberately **not** empty.
///
/// Without this, the assertion above would pass just as well against a node that tore a transaction
/// down the instant it answered — which is the bug §17 exists to prevent, because a retransmission
/// arriving afterwards would then be delivered to the application a second time.
#[test]
fn the_store_is_deliberately_not_empty_before_the_window() {
    let sim = a_completed_call(0x4346_2202);
    let held = held_by(&sim);
    assert!(
        held.total() > 0,
        "a concluded transaction is held for its absorption timer; finding none a second after \
         the call means §17's window is not being kept\n{}",
        sim.trace().render()
    );
}

/// **The bound is 128·T1 because the RFC permits 128·T1**, and here is the case that spends it.
///
/// Bob registers and then goes silent, so the branch produces nothing: the client transaction runs
/// Timer B to 64·T1, only then does §16.7 have a final response to forward upstream, and only then
/// does the server transaction start Timer H for its own 64·T1. It empties at **64.000 s exactly**,
/// which is 128·T1 and is the number `PX-13`'s end-to-end run produced. A one-window bound calls
/// that a leak; it is not one.
///
/// Both halves are asserted, at **absolute** instants rather than offsets from [`BOUND`]: still held
/// at 127·T1, empty by 128·T1 + slack. That is what makes this a measurement of the bound rather
/// than a restatement of it — set `BOUND` to one window and this test goes red, which is exactly
/// what the first version of this file failed to do.
#[test]
fn the_worst_case_the_rfc_permits_is_inside_the_bound() {
    let mut sim = scenario(0x4346_2203, false, Hold::None);
    sim.advance(CALL_WINDOW).expect("the call runs");
    assert_eq!(
        sim.node::<Caller>(ALICE).and_then(|alice| alice.answered),
        None,
        "the callee is supposed to be silent, so nothing should have answered alice yet\n{}",
        sim.trace().render()
    );

    // 127·T1. Timer H has not fired, so anything asserting a one-window bound is red here.
    sim.run_until(at(ABSORPTION.saturating_mul(2).saturating_sub(T1)))
        .expect("the sim runs");
    let held = held_by(&sim);
    assert!(
        held.total() > 0,
        "a branch to a silent next hop is still being absorbed at 127·T1, and finding nothing here \
         means the bound below is not measuring two windows at all — {}\n{}",
        held.describe(),
        sim.trace().render()
    );

    assert_drains_within_the_bound(&mut sim, "a call to a callee that never answered");
}

/// **The failing-first proof, and the distinction the story is about.**
///
/// A server transaction the application never answers. §17.2.2 gives one in Trying no timer at all,
/// so nothing in the RFC will ever collect it — this is a leak in the sense that matters, as against
/// the merely-slow case above. The check is the same function, so this is the assertion from
/// [`a_completed_call_leaves_the_node_holding_nothing`] doing its job rather than a second opinion.
///
/// It is checked twice: at the bound, where the check would fire, and ten windows out, which is what
/// separates *unbounded* from *slow*. Nothing in §17 runs past 64·T1, and the one proxy-level timer
/// that runs longer — Timer C, 240 s — is INVITE-only and inside [`LONG_AFTER`] anyway.
#[test]
fn a_hold_no_timer_collects_is_caught() {
    // The same call as the headline test, with one fault injected into it: the node takes the
    // hang-up and never answers it. Everything before the BYE is untouched, so what this measures
    // is the hold and not a scenario that fell over early.
    let mut sim = scenario(0x4346_2204, true, Hold::NeverAnswer(Method::Bye));
    sim.advance(CALL_WINDOW).expect("the call runs");
    let alice = sim.node::<Caller>(ALICE).expect("alice is in the sim");
    let bob = sim.node::<Callee>(BOB).expect("bob is in the sim");
    assert_eq!(
        alice.answered,
        Some(200),
        "the call was not answered, so the hold below is not being injected into a call\n{}",
        sim.trace().render()
    );
    assert!(
        bob.acknowledged,
        "the callee never hung up, so no BYE was there to swallow\n{}",
        sim.trace().render()
    );

    sim.run_until(at(BOUND + SLACK)).expect("the sim runs");
    let held = held_by(&sim);
    assert!(
        held.total() > 0,
        "the injected hold was collected, so this proves nothing about the check. A server \
         transaction the application never answers has no §17 timer and must still be held.\n{}",
        sim.trace().render()
    );

    // Unbounded, not slow. If this drains, the hold was a long timer and the check above is only
    // measuring patience.
    sim.run_until(at(BOUND + LONG_AFTER)).expect("the sim runs");
    let later = held_by(&sim);
    assert!(
        later.total() > 0,
        "the injected hold drained {LONG_AFTER:?} after the call, so it was bounded after all and \
         is not the leak this test claims to inject\n{}",
        sim.trace().render()
    );
    assert_eq!(
        later.total(),
        held.total(),
        "an unbounded hold does not shrink: {} at the bound, {} ten windows later\n{}",
        held.describe(),
        later.describe(),
        sim.trace().render()
    );
}
