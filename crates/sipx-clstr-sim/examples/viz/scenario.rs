//! The showcase scenario: register, then call — with the real registrar and the real forwarding
//! core in the edge, ported from `tests/register_then_call.rs` (`RG-6`'s acceptance scenario).
//!
//! Two of bob's devices register one address-of-record, so the call forks: the page shows two
//! branches, a `200` race, and the losing branch cancelled — the fullest tour of the trace
//! vocabulary a single scenario offers today. The assertions that made this a test stay in the
//! test file; what lives here is the topology and the nodes, trimmed of everything only an
//! assertion needed.
//!
//! The panics are deliberate: builder calls here are infallible for the constant inputs below,
//! and this is a dev tool — a panic kills a demo someone is watching, never a call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

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
use sipx_clstr_sim::viz::{LinkMeta, NodeMeta, Role};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder, StatusCode,
    Uri,
};

const EDGE: &str = "edge-1.example";
const TENANT: &str = "t1";

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

/// Everything the adapter needs beside the `Sim`: the stage description for the `meta` frame.
#[derive(Debug)]
pub(crate) struct BuiltScenario {
    /// The ready-to-run simulation.
    pub sim: Sim,
    /// Nodes with their stage roles, in `Sim::add_node` order.
    pub nodes: Vec<NodeMeta>,
    /// Links, one per pair.
    pub links: Vec<LinkMeta>,
}

/// One edge running the real registrar and proxy; alice and two of bob's devices register, then
/// alice calls bob and the call forks. `policy` is the weather — clean, jittery or storm.
pub(crate) fn register_call(seed: u64, policy: LinkPolicy) -> BuiltScenario {
    let bob_contacts = ["sip:bob@10.0.0.1", "sip:bob@10.0.0.2"];

    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);

    // Bob's devices occupy nodes 2 and 3; the map is built before they are added, which is safe
    // because node ids are positions and the order here is fixed.
    let mut reachable = HashMap::new();
    for (offset, contact) in bob_contacts.iter().enumerate() {
        reachable.insert((*contact).to_owned(), NodeId::from_index(2 + offset));
    }

    sim.add_node(Box::new(Edge::new("edge", reachable)));
    sim.add_node(Box::new(Phone::caller("alice")));
    for contact in bob_contacts {
        sim.add_node(Box::new(Phone::callee(
            &format!("bob-{}", contact.rsplit('@').next().unwrap_or("?")),
            contact,
        )));
    }

    let nodes = vec![
        NodeMeta {
            id: NodeId::from_index(0),
            name: "edge".to_owned(),
            role: Role::Edge,
        },
        NodeMeta {
            id: NodeId::from_index(1),
            name: "alice".to_owned(),
            role: Role::Endpoint,
        },
        NodeMeta {
            id: NodeId::from_index(2),
            name: "bob-10.0.0.1".to_owned(),
            role: Role::Endpoint,
        },
        NodeMeta {
            id: NodeId::from_index(3),
            name: "bob-10.0.0.2".to_owned(),
            role: Role::Endpoint,
        },
    ];
    let links = vec![
        LinkMeta {
            from: NodeId::from_index(0),
            to: NodeId::from_index(1),
            kind: LinkKind::Datagram,
        },
        LinkMeta {
            from: NodeId::from_index(0),
            to: NodeId::from_index(2),
            kind: LinkKind::Datagram,
        },
        LinkMeta {
            from: NodeId::from_index(0),
            to: NodeId::from_index(3),
            kind: LinkKind::Datagram,
        },
    ];

    BuiltScenario { sim, nodes, links }
}

const EDGE_NODE: NodeId = NodeId::from_index(0);

// ---------------------------------------------------------------------------------------------
// the edge: registrar and proxy in one, which is what a small deployment runs
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Edge {
    name: String,
    store: InMemoryStore,
    policy: TenantPolicy,
    /// One response context per proxied request, keyed by `Call-ID`.
    contexts: HashMap<String, ResponseContext>,
    /// Which contact URI reaches which simulated node — the sim's stand-in for a socket address.
    reachable: HashMap<String, NodeId>,
    /// Per proxied request: who sent it, and where each branch went.
    upstream: HashMap<String, NodeId>,
    branch_nodes: HashMap<BranchId, NodeId>,
    /// `Call-ID` per branch, so a response finds its context.
    branch_call: HashMap<BranchId, String>,
    timers: Vec<BranchId>,
    /// The current virtual time, captured on every input — a lookup's expiry is evaluated
    /// against it.
    now: Timestamp,
}

impl Edge {
    fn new(name: &str, reachable: HashMap<String, NodeId>) -> Self {
        Self {
            name: name.to_owned(),
            store: InMemoryStore::new(),
            policy: TenantPolicy::default(),
            contexts: HashMap::new(),
            reachable,
            upstream: HashMap::new(),
            branch_nodes: HashMap::new(),
            branch_call: HashMap::new(),
            timers: Vec::new(),
            now: Timestamp::ZERO,
        }
    }

    fn config() -> ProxyConfig {
        ProxyConfig::new(
            EDGE,
            Bytes::from_static(b"<sip:edge-1.example;lr>"),
            CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
        )
    }

    fn call_id(message: &Message) -> String {
        message
            .headers()
            .value(&HeaderName::CallId)
            .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------ registrar --------------

    fn on_register(&mut self, from: NodeId, request: &Request, now: SimTime) -> Vec<Effect> {
        let context = EdgeContext {
            tenant: TENANT.to_owned(),
            ..EdgeContext::default()
        };
        let clock = Timestamp::from_nanos(now.as_nanos());

        let cmd = match register_command(request, &context, clock) {
            Ok(cmd) => cmd,
            Err(rejection) => {
                return vec![reply(from, request, rejection.status(), "Bad Request")];
            }
        };

        let applied = apply(&self.store, &cmd, &self.policy, 3);
        let status = applied.outcome.status();
        let listed = applied
            .outcome
            .accepted()
            .map_or(0, |accepted| accepted.contacts.len());

        vec![
            Effect::Note(format!("registered {status}, set now {listed}")),
            reply(from, request, status, "OK"),
        ]
    }

    // ------------------------------------------------------------------ proxy ------------------

    fn on_proxied_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        let call = Self::call_id(&Message::Request(request.clone()));
        self.upstream.insert(call.clone(), from);
        let mut context = ResponseContext::new(Self::config());
        let effects = context.on_input(ProxyInput::Upstream(Box::new(request.clone())));
        self.contexts.insert(call.clone(), context);
        self.perform(&call, effects)
    }

    fn on_branch_response(&mut self, response: &Response) -> Vec<Effect> {
        let Some(branch) = top_via_branch(response) else {
            return Vec::new();
        };
        let branch = BranchId(branch);
        let Some(call) = self.branch_call.get(&branch).cloned() else {
            return Vec::new();
        };
        let Some(mut context) = self.contexts.remove(&call) else {
            return Vec::new();
        };
        let effects = context.on_input(ProxyInput::BranchResponse(
            Box::new(response.clone()),
            branch,
        ));
        let finished = context.is_finished();
        if !finished {
            self.contexts.insert(call.clone(), context);
        }
        self.perform(&call, effects)
    }

    /// Perform the engine's effects, in order.
    fn perform(&mut self, call: &str, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProxyEffect::ResolveTargets(query) => {
                    // §16.5 — the location service answers. Bindings in, forking targets out,
                    // with the lookup's own order preserved.
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
                    out.extend(self.perform(call, more));
                }
                ProxyEffect::Forward {
                    branch,
                    request,
                    next_hop,
                    ..
                } => {
                    // F7's next hop, the way the real driver reads it: the target is what went into
                    // the Request-URI, and the hop is where the copy actually goes. They differ as
                    // soon as a `Route` survives or a registration carries a `Path`, and a harness
                    // that keyed on the target would model a driver nobody ships.
                    let key = String::from_utf8_lossy(&next_hop).into_owned();
                    let Some(node) = self.reachable.get(&key).copied() else {
                        out.push(Effect::Note(format!("unreachable {key}")));
                        continue;
                    };
                    self.branch_nodes.insert(branch.clone(), node);
                    self.branch_call.insert(branch.clone(), call.to_owned());
                    out.push(Effect::Note(format!("forward to {key}")));
                    out.push(send(node, Message::Request(*request)));
                }
                ProxyEffect::Respond(response) => {
                    if let Some(&upstream) = self.upstream.get(call) {
                        out.push(Effect::Note(format!("respond {}", response.status.code())));
                        out.push(send(upstream, Message::Response(*response)));
                    }
                }
                ProxyEffect::CancelBranch(branch) => {
                    out.push(Effect::Note(format!("cancel {branch}")));
                }
                ProxyEffect::AnswerCancel => out.push(Effect::Note("answered cancel".to_owned())),
                // This scenario mints no affinity token, so P2 never asks. Noted rather than
                // absorbed, and the note is what the HUD would show: an unanswered verification
                // request is a context that waits forever.
                ProxyEffect::VerifyToken { .. } => {
                    out.push(Effect::Note("unexpected token verification".to_owned()));
                }
                ProxyEffect::SetTimer { branch, after, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(&branch);
                        out.push(Effect::SetTimer { timer, after });
                    }
                }
                ProxyEffect::ClearTimer { branch, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(&branch);
                        out.push(Effect::ClearTimer(timer));
                    }
                }
                ProxyEffect::Terminate => out.push(Effect::Note("terminate".to_owned())),
            }
        }
        out
    }

    fn timer_id(&mut self, branch: &BranchId) -> TimerId {
        if let Some(index) = self.timers.iter().position(|known| known == branch) {
            return TimerId(u64::try_from(index).unwrap_or(u64::MAX));
        }
        self.timers.push(branch.clone());
        TimerId(u64::try_from(self.timers.len() - 1).unwrap_or(u64::MAX))
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        self.now = Timestamp::from_nanos(now.as_nanos());
        match input {
            Input::Message { from, message } => match message {
                Message::Request(request) if request.method == Method::Register => {
                    self.on_register(from, request, now)
                }
                Message::Request(request) => self.on_proxied_request(from, request),
                Message::Response(response) => self.on_branch_response(response),
            },
            Input::Timer(timer) => {
                let Some(index) = usize::try_from(timer.0).ok() else {
                    return Vec::new();
                };
                let Some(branch) = self.timers.get(index).cloned() else {
                    return Vec::new();
                };
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
                self.perform(&call, effects)
            }
            Input::Started | Input::TransportError { .. } => Vec::new(),
        }
    }
}

fn top_via_branch(response: &Response) -> Option<String> {
    let value = response.headers.get(&HeaderName::Via)?.value();
    let via = sipx_sip::headers::Via::parse_one(&value).ok()?;
    via.branch()
        .map(|branch| String::from_utf8_lossy(branch).into_owned())
}

fn reply(to: NodeId, request: &Request, status: u16, reason: &str) -> Effect {
    let response = ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason.to_owned(),
    )
    .expect("a response")
    .build();
    send(to, Message::Response(response))
}

// ---------------------------------------------------------------------------------------------
// the phones
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Phone {
    /// The label this node appears under in traces. Distinct from `user` on purpose: two devices
    /// share one address-of-record, so the trace needs to tell them apart while SIP must not.
    name: String,
    /// The user part of this phone's address-of-record.
    user: String,
    /// The contact to register.
    contact: String,
    /// Who to call once registered, if anyone.
    calls: Option<&'static str>,
    registered: bool,
    invited: bool,
    cseq: u32,
}

impl Phone {
    fn caller(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            user: name.to_owned(),
            contact: format!("sip:{name}@10.0.0.9"),
            calls: Some("bob"),
            registered: false,
            invited: false,
            cseq: 0,
        }
    }

    fn callee(name: &str, contact: &str) -> Self {
        Self {
            name: name.to_owned(),
            user: "bob".to_owned(),
            contact: contact.to_owned(),
            calls: None,
            registered: false,
            invited: false,
            cseq: 0,
        }
    }

    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let cseq = response
            .headers
            .value(&HeaderName::CSeq)
            .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
            .unwrap_or_default();

        if cseq.ends_with("REGISTER") {
            self.registered = response.status.is_success();
            if let Some(peer) = self.calls
                && self.registered
                && !self.invited
            {
                self.invited = true;
                let invite = self.invite(peer);
                return vec![
                    Effect::Note(format!("{} registered", self.name)),
                    send(EDGE_NODE, invite),
                ];
            }
            return vec![Effect::Note(format!("{} registered", self.name))];
        }

        if response.status.is_provisional() {
            return Vec::new();
        }
        vec![Effect::Note(format!(
            "{} got {}",
            self.name,
            response.status.code()
        ))]
    }

    fn on_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        if request.method != Method::Invite {
            return Vec::new();
        }
        vec![
            Effect::Note(format!("{} ringing", self.name)),
            reply(from, request, 200, "OK"),
        ]
    }

    fn register(&mut self) -> Message {
        self.cseq += 1;
        Message::Request(
            RequestBuilder::new(Method::Register, uri("sip:atlanta.example"))
                .header(HeaderName::CallId, format!("reg-{}", self.name))
                .expect("Call-ID")
                .cseq(self.cseq, &Method::Register)
                .expect("CSeq")
                .header(
                    HeaderName::From,
                    format!("<sip:{}@atlanta.example>;tag=t", self.user),
                )
                .expect("From")
                .header(
                    HeaderName::To,
                    format!("<sip:{}@atlanta.example>", self.user),
                )
                .expect("To")
                .header(
                    HeaderName::Via,
                    format!(
                        "SIP/2.0/UDP {}.sim;branch=z9hG4bK-r{}",
                        self.name, self.cseq
                    ),
                )
                .expect("Via")
                .header(
                    HeaderName::Contact,
                    format!("<{}>;expires=3600", self.contact),
                )
                .expect("Contact")
                .build(),
        )
    }

    fn invite(&mut self, peer: &str) -> Message {
        self.cseq += 1;
        Message::Request(
            RequestBuilder::new(Method::Invite, uri(&format!("sip:{peer}@atlanta.example")))
                .header(HeaderName::CallId, format!("call-{}", self.name))
                .expect("Call-ID")
                .cseq(self.cseq, &Method::Invite)
                .expect("CSeq")
                .header(
                    HeaderName::From,
                    format!("<sip:{}@atlanta.example>;tag=c", self.user),
                )
                .expect("From")
                .header(HeaderName::To, format!("<sip:{peer}@atlanta.example>"))
                .expect("To")
                .header(
                    HeaderName::Via,
                    format!(
                        "SIP/2.0/UDP {}.sim;branch=z9hG4bK-c{}",
                        self.name, self.cseq
                    ),
                )
                .expect("Via")
                .max_forwards(70)
                .build(),
        )
    }
}

impl SimNode for Phone {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let register = self.register();
                vec![send(EDGE_NODE, register)]
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            Input::Message {
                from,
                message: Message::Request(request),
            } => self.on_request(from, request),
            _ => Vec::new(),
        }
    }
}
