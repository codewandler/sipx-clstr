//! Register, then call — `RG-6`'s acceptance, and the shape of M1's exit.
//!
//! One node running the **real** registrar and the **real** forwarding core: two endpoints register,
//! one calls the other, the node looks the callee up, converts the bindings into forking targets and
//! forks to them. Everything is genuine except the driver, which is this file, and the transport,
//! which is the simulated network. `CX-3` does the same thing over real sockets against real phones.
//!
//! What it proves that the unit vectors cannot: the two halves of M1 agree about the same bytes. A
//! contact stored verbatim by the registrar is the Request-URI the proxy forwards to; a `Path` stored
//! topmost-first is the route set the branch carries; the lookup's order is the fork order.

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
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder, StatusCode,
    Uri,
};

const EDGE: &str = "edge-1.example";
const TENANT: &str = "t1";

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

// ---------------------------------------------------------------------------------------------
// the node: registrar and proxy in one, which is what a small deployment runs
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
    /// Every lookup this node performed, for the scenario to assert on.
    lookups: Vec<usize>,
    /// The current virtual time, captured on every input.
    ///
    /// A lookup needs the *real* `now`: expiry is evaluated against it, so a far-future value does
    /// not mean "ignore expiry" — it means every binding has already lapsed, and the lookup returns
    /// nothing. That mistake cost this file four failing tests.
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
            lookups: Vec::new(),
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

        // The complete active set goes back in the `200`, per §5.6. Modelled as a note here rather
        // than rendered into Contact headers: the endpoints in this scenario do not read it, and
        // asserting on the note keeps the scenario about `RG-6` rather than about header rendering.
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
                    // §16.5 — the location service answers. This is the RG-6 seam: bindings in,
                    // forking targets out, with the lookup's own order preserved.
                    let found = match CanonicalAor::parse(query.uri.clone()) {
                        Ok(aor) => self.store.lookup(TENANT, &aor, self.now),
                        Err(_) => Vec::new(),
                    };
                    self.lookups.push(found.len());
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
                    target,
                } => {
                    let key = String::from_utf8_lossy(&target.uri).into_owned();
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
// endpoints
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Phone {
    /// The label this node appears under in traces. Distinct from `user` on purpose: two devices
    /// share one address-of-record, so the trace needs to tell them apart while SIP must not.
    name: String,
    /// The user part of this phone's address-of-record.
    user: String,
    edge: NodeId,
    /// The contacts to register, in one REGISTER. Several means the callee forks.
    contacts: Vec<&'static str>,
    /// A `Path` to offer, exercising RFC 3327 through the whole pipeline.
    path: Option<&'static str>,
    /// Who to call once registered, if anyone.
    calls: Option<&'static str>,
    registered: bool,
    invited: bool,
    answered: Option<u16>,
    /// The route set the INVITE arrived with, so the scenario can check `Path` came through.
    arrived_routes: Vec<String>,
    cseq: u32,
}

impl SimNode for Phone {
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

impl Phone {
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
                    send(self.edge, invite),
                ];
            }
            return vec![Effect::Note(format!("{} registered", self.name))];
        }

        if response.status.is_provisional() {
            return Vec::new();
        }
        if self.answered.is_none() {
            self.answered = Some(response.status.code());
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
        self.arrived_routes = request
            .headers
            .get_all(&HeaderName::Route)
            .map(|header| String::from_utf8_lossy(&header.value()).trim().to_owned())
            .collect();
        vec![
            Effect::Note(format!("{} ringing", self.name)),
            reply(from, request, 200, "OK"),
        ]
    }

    fn register(&mut self) -> Message {
        self.cseq += 1;
        let mut builder = RequestBuilder::new(Method::Register, uri("sip:atlanta.example"))
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
            .expect("Via");

        for contact in &self.contacts {
            builder = builder
                .header(HeaderName::Contact, format!("<{contact}>;expires=3600"))
                .expect("Contact");
        }
        if let Some(path) = self.path {
            builder = builder
                .header(HeaderName::Supported, "path")
                .expect("Supported")
                .header(HeaderName::Path, path)
                .expect("Path");
        }
        Message::Request(builder.build())
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

// ---------------------------------------------------------------------------------------------
// scenarios
// ---------------------------------------------------------------------------------------------

const EDGE_NODE: NodeId = NodeId::from_index(0);
const ALICE: NodeId = NodeId::from_index(1);
const BOB_1: NodeId = NodeId::from_index(2);

/// One edge; alice calls bob. `bob_contacts` decides whether the call forks.
fn scenario(
    seed: u64,
    policy: LinkPolicy,
    bob_contacts: Vec<&'static str>,
    bob_path: Option<&'static str>,
) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);

    // Bob's devices occupy nodes 2..; the map is built before they are added, which is safe because
    // node ids are positions and the order here is fixed.
    let mut reachable = HashMap::new();
    for (offset, contact) in bob_contacts.iter().enumerate() {
        reachable.insert((*contact).to_owned(), NodeId::from_index(2 + offset));
    }

    sim.add_node(Box::new(Edge::new("edge", reachable)));
    sim.add_node(Box::new(Phone {
        name: "alice".to_owned(),
        user: "alice".to_owned(),
        edge: EDGE_NODE,
        contacts: vec!["sip:alice@10.0.0.9"],
        path: None,
        calls: Some("bob"),
        registered: false,
        invited: false,
        answered: None,
        arrived_routes: Vec::new(),
        cseq: 0,
    }));
    for (index, contact) in bob_contacts.into_iter().enumerate() {
        sim.add_node(Box::new(Phone {
            // Two devices, one address-of-record — which is the whole point of forking.
            name: format!("bob-{}", index + 1),
            user: "bob".to_owned(),
            edge: EDGE_NODE,
            contacts: vec![contact],
            path: bob_path,
            calls: None,
            registered: false,
            invited: false,
            answered: None,
            arrived_routes: Vec::new(),
            cseq: 0,
        }));
    }
    sim
}

#[test]
fn a_call_reaches_a_registered_contact_through_one_node() {
    let mut sim = scenario(
        0x5247_0601,
        LinkPolicy::CLEAN,
        vec!["sip:bob@10.0.0.1"],
        None,
    );
    sim.run_until_idle().expect("settles");

    assert_eq!(
        sim.node::<Phone>(ALICE).and_then(|p| p.answered),
        Some(200),
        "{}",
        sim.trace().render()
    );
    // The lookup found exactly the binding the REGISTER created — the two halves agree.
    assert_eq!(
        sim.node::<Edge>(EDGE_NODE).map(|e| e.lookups.clone()),
        Some(vec![1])
    );
}

#[test]
fn two_registered_contacts_make_the_call_fork_to_both() {
    // §7 L4 into §16.6: equal `q` is one parallel fork group, and the engine forks the whole group.
    let mut sim = scenario(
        0x5247_0602,
        LinkPolicy::CLEAN,
        vec!["sip:bob@10.0.0.1", "sip:bob@10.0.0.2"],
        None,
    );
    sim.run_until_idle().expect("settles");

    assert_eq!(
        sim.node::<Edge>(EDGE_NODE).map(|e| e.lookups.clone()),
        Some(vec![2]),
        "both bindings should have been found"
    );
    let forwards = sim
        .trace()
        .notes()
        .iter()
        .filter(|note| note.starts_with("forward to"))
        .count();
    assert_eq!(forwards, 2, "{}", sim.trace().render());
    assert_eq!(sim.node::<Phone>(ALICE).and_then(|p| p.answered), Some(200));
}

#[test]
fn a_registered_path_becomes_the_route_set_the_invite_carries() {
    // RFC 3327 end to end: the callee registered a `Path`, and the branch toward it must carry that
    // route set. This is the assertion that proves the two crates agree about the same bytes rather
    // than each being right on its own.
    let mut sim = scenario(
        0x5247_0603,
        LinkPolicy::CLEAN,
        vec!["sip:bob@10.0.0.1"],
        Some("<sip:p1.example;lr>"),
    );
    sim.run_until_idle().expect("settles");

    let routes = sim
        .node::<Phone>(BOB_1)
        .map(|p| p.arrived_routes.clone())
        .unwrap_or_default();
    assert!(
        routes.iter().any(|route| route.contains("p1.example")),
        "the registered Path should route the branch: {routes:?}\n{}",
        sim.trace().render()
    );
}

#[test]
fn a_call_to_someone_who_never_registered_is_480() {
    // §7: the location service returns an empty set and the proxy decides the response. A registrar
    // that answered on the proxy's behalf would put that decision in the wrong layer.
    let mut sim = Sim::new(0x5247_0604);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);
    sim.add_node(Box::new(Edge::new("edge", HashMap::new())));
    sim.add_node(Box::new(Phone {
        name: "alice".to_owned(),
        user: "alice".to_owned(),
        edge: EDGE_NODE,
        contacts: vec!["sip:alice@10.0.0.9"],
        path: None,
        calls: Some("nobody"),
        registered: false,
        invited: false,
        answered: None,
        arrived_routes: Vec::new(),
        cseq: 0,
    }));
    sim.run_until_idle().expect("settles");

    assert_eq!(
        sim.node::<Phone>(ALICE).and_then(|p| p.answered),
        Some(480),
        "{}",
        sim.trace().render()
    );
}

#[test]
fn the_whole_pipeline_replays_byte_for_byte_under_jitter() {
    let policy = LinkPolicy::jittery(1, 30).with_duplication(0.2);
    for seed in 0..8_u64 {
        let mut first = scenario(
            seed,
            policy,
            vec!["sip:bob@10.0.0.1", "sip:bob@10.0.0.2"],
            None,
        );
        let mut second = scenario(
            seed,
            policy,
            vec!["sip:bob@10.0.0.1", "sip:bob@10.0.0.2"],
            None,
        );
        first.advance(Duration::from_secs(5)).expect("runs");
        second.advance(Duration::from_secs(5)).expect("runs");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed} diverged"
        );
    }
}

#[test]
fn the_call_completes_across_a_sweep_of_adversarial_schedules() {
    let policy = LinkPolicy::jittery(1, 30).with_duplication(0.2);
    for seed in 0..16_u64 {
        let mut sim = scenario(
            0x5247_0700 + seed,
            policy,
            vec!["sip:bob@10.0.0.1", "sip:bob@10.0.0.2"],
            None,
        );
        // Bounded well below Timer C, so a 200 here came from a callee rather than from a timer.
        sim.advance(Duration::from_secs(10))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert_eq!(
            sim.node::<Phone>(ALICE).and_then(|p| p.answered),
            Some(200),
            "seed {seed}\n{}",
            sim.trace().render()
        );
    }
}
