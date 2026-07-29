//! CANCEL and Timer C under the harness — `PX-6`'s third acceptance criterion.
//!
//! The unit vectors in `sipx-clstr-proxy` feed the engine one input at a time in an order the test
//! chose. This file does the opposite: it puts the **real** `ResponseContext` behind a sim driver and
//! lets the network decide the order, with jitter, duplication and loss on, across a sweep of seeds.
//! What it is looking for is the case no hand-written ordering thinks of — a `487` that arrives
//! before the `200` to the CANCEL, a duplicated CANCEL, a provisional that overtakes the request
//! that queued a cancel behind it.
//!
//! It is also the first time the engine runs under the harness at all, which is the groundwork
//! `PX-7` builds the full vector report on.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_proxy::{
    BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig, ProxyTimer,
    ResponseContext, Target,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, ResponseBuilder, StatusCode, Uri,
};

const EDGE: &str = "edge-1.example";

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

// ---------------------------------------------------------------------------------------------
// the proxy, as a simulated node
// ---------------------------------------------------------------------------------------------

/// A node that owns a real [`ResponseContext`] and performs its effects on the simulated network.
#[derive(Debug)]
struct ProxyNode {
    name: String,
    context: ResponseContext,
    /// Where each target URI lives, so `Forward` knows which node to send to.
    routes: HashMap<String, NodeId>,
    /// The targets to answer `ResolveTargets` with.
    targets: Vec<Target>,
    /// Who sent us the request, and therefore where responses go.
    upstream: Option<NodeId>,
    /// Which node each branch was forwarded to, for CANCEL.
    branch_nodes: HashMap<BranchId, NodeId>,
    /// The request each branch carried, so a CANCEL can be minted from it (RFC 3261 §9.1).
    branch_requests: HashMap<BranchId, Request>,
    /// Timer ids are per branch; this is the mapping the sim's flat `TimerId` needs.
    timers: Vec<BranchId>,
}

impl ProxyNode {
    fn new(name: &str, routes: HashMap<String, NodeId>, targets: Vec<Target>) -> Self {
        Self {
            name: name.to_owned(),
            context: ResponseContext::new(ProxyConfig::new(
                EDGE,
                Bytes::from_static(b"<sip:edge-1.example;lr>"),
                CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
            )),
            routes,
            targets,
            upstream: None,
            branch_nodes: HashMap::new(),
            branch_requests: HashMap::new(),
            timers: Vec::new(),
        }
    }

    fn timer_id(&mut self, branch: &BranchId) -> TimerId {
        if let Some(index) = self.timers.iter().position(|known| known == branch) {
            return TimerId(index_to_timer(index));
        }
        self.timers.push(branch.clone());
        TimerId(index_to_timer(self.timers.len() - 1))
    }

    /// Perform the engine's effects on the network, in the order the engine produced them.
    fn perform(&mut self, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProxyEffect::Respond(response) => {
                    if let Some(upstream) = self.upstream {
                        out.push(Effect::Note(format!("respond {}", response.status.code())));
                        out.push(send(upstream, Message::Response(*response)));
                    }
                }
                ProxyEffect::AnswerCancel => {
                    // C1 — a `200` on the CANCEL's own transaction. Modelled as a note plus a
                    // response to the canceller, which is what the driver does with the CANCEL's
                    // server transaction.
                    out.push(Effect::Note("answered cancel 200".to_owned()));
                }
                ProxyEffect::Forward {
                    branch,
                    request,
                    target,
                } => {
                    let key = String::from_utf8_lossy(&target.uri).into_owned();
                    if let Some(node) = self.routes.get(&key).copied() {
                        self.branch_nodes.insert(branch.clone(), node);
                        self.branch_requests
                            .insert(branch.clone(), (*request).clone());
                        out.push(Effect::Note(format!("forward {branch}")));
                        out.push(send(node, Message::Request(*request)));
                    }
                }
                ProxyEffect::CancelBranch(branch) => {
                    let Some(node) = self.branch_nodes.get(&branch).copied() else {
                        continue;
                    };
                    let Some(original) = self.branch_requests.get(&branch).cloned() else {
                        continue;
                    };
                    out.push(Effect::Note(format!("cancel {branch}")));
                    out.push(send(node, Message::Request(cancel_for(&original))));
                }
                ProxyEffect::ResolveTargets(_) => {
                    // The driver answers immediately here; a real one awaits the location service,
                    // which is `RG-6`'s wiring.
                    let targets = self.targets.clone();
                    let more = self.context.on_input(ProxyInput::TargetsResolved(targets));
                    out.extend(self.perform(more));
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
}

impl SimNode for ProxyNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message { from, message } => match message {
                Message::Request(request) if request.method == Method::Cancel => {
                    // Split across two statements on purpose: `self.context.on_input(…)` inside the
                    // `self.perform(…)` argument list borrows `self` twice.
                    let effects = self.context.on_input(ProxyInput::UpstreamCancelled);
                    let mut out = vec![Effect::Note(format!("cancel from {from}"))];
                    out.extend(self.perform(effects));
                    let _ = request;
                    out
                }
                Message::Request(request) => {
                    self.upstream = Some(from);
                    let effects = self
                        .context
                        .on_input(ProxyInput::Upstream(Box::new(request.clone())));
                    self.perform(effects)
                }
                Message::Response(response) => {
                    // Match the response to its branch by the top `Via` branch — which *is* the
                    // branch id, because the engine mints one from the other.
                    let Some(branch) = top_via_branch(response) else {
                        return Vec::new();
                    };
                    let effects = self.context.on_input(ProxyInput::BranchResponse(
                        Box::new(response.clone()),
                        BranchId(branch),
                    ));
                    self.perform(effects)
                }
            },
            Input::Timer(timer) => {
                let Some(index) = usize::try_from(timer.0).ok() else {
                    return Vec::new();
                };
                let Some(branch) = self.timers.get(index).cloned() else {
                    return Vec::new();
                };
                let effects = self
                    .context
                    .on_input(ProxyInput::TimerFired(ProxyTimer::C, Some(branch)));
                self.perform(effects)
            }
            Input::TransportError { .. } | Input::Started => Vec::new(),
        }
    }
}

fn index_to_timer(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

fn top_via_branch(response: &sipx_sip::Response) -> Option<String> {
    let value = response.headers.get(&HeaderName::Via)?.value();
    let via = sipx_sip::headers::Via::parse_one(&value).ok()?;
    via.branch()
        .map(|branch| String::from_utf8_lossy(branch).into_owned())
}

/// A CANCEL for a request, per RFC 3261 §9.1: same Request-URI, `Call-ID`, `To`, `From` and `CSeq`
/// number, method `CANCEL`, and **the same top `Via` branch** as the INVITE it stops.
fn cancel_for(invite: &Request) -> Request {
    let mut cancel = RequestBuilder::new(Method::Cancel, invite.uri.clone())
        .cseq(cseq_of(invite), &Method::Cancel)
        .expect("a CSeq")
        .build();
    for name in [
        HeaderName::Via,
        HeaderName::CallId,
        HeaderName::From,
        HeaderName::To,
    ] {
        if let Some(header) = invite.headers.get(&name) {
            cancel.headers.push(header.clone());
        }
    }
    cancel
}

fn cseq_of(request: &Request) -> u32 {
    request
        .headers
        .value(&HeaderName::CSeq)
        .and_then(|value| {
            String::from_utf8_lossy(&value)
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(1)
}

// ---------------------------------------------------------------------------------------------
// endpoints
// ---------------------------------------------------------------------------------------------

/// A callee: rings, or stays silent, and answers a CANCEL with `487`.
#[derive(Debug)]
struct Callee {
    name: String,
    /// Whether to send a `180` on the INVITE. A silent callee is what Timer C exists for.
    rings: bool,
    saw_cancel: bool,
}

impl SimNode for Callee {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } => match request.method {
                Method::Invite if self.rings => {
                    vec![send(from, response_to(request, 180, "Ringing"))]
                }
                Method::Cancel => {
                    self.saw_cancel = true;
                    // The `487` concludes the INVITE; the CANCEL's own `200` is a separate
                    // transaction the proxy does not aggregate.
                    vec![
                        Effect::Note(format!("{} terminated", self.name)),
                        send(from, response_to(request, 487, "Request Terminated")),
                    ]
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }
}

/// The caller: INVITEs, then CANCELs once it has been told the call is ringing.
#[derive(Debug)]
struct Caller {
    name: String,
    proxy: NodeId,
    cancel_after_ringing: bool,
    invite: Option<Request>,
    final_status: Option<u16>,
    cancelled: bool,
}

impl SimNode for Caller {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let invite = Self::invite();
                self.invite = Some(invite.clone());
                vec![send(self.proxy, Message::Request(invite))]
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => {
                let status = response.status.code();
                if response.status.is_provisional() {
                    if self.cancel_after_ringing && !self.cancelled {
                        self.cancelled = true;
                        let Some(invite) = self.invite.clone() else {
                            return Vec::new();
                        };
                        return vec![
                            Effect::Note("caller cancels".to_owned()),
                            send(self.proxy, Message::Request(cancel_for(&invite))),
                        ];
                    }
                    return Vec::new();
                }
                // A late second final is a fork answer (RFC 6026); the first one is the outcome.
                if self.final_status.is_none() {
                    self.final_status = Some(status);
                }
                vec![Effect::Note(format!("caller got {status}"))]
            }
            _ => Vec::new(),
        }
    }
}

impl Caller {
    fn invite() -> Request {
        RequestBuilder::new(Method::Invite, uri("sip:bob@b.example"))
            .header(HeaderName::CallId, "call-cancel")
            .expect("Call-ID")
            .cseq(1, &Method::Invite)
            .expect("CSeq")
            .header(HeaderName::From, "<sip:alice@a.example>;tag=af")
            .expect("From")
            .header(HeaderName::To, "<sip:bob@b.example>")
            .expect("To")
            .header(
                HeaderName::Via,
                "SIP/2.0/UDP alice.example;branch=z9hG4bK-in",
            )
            .expect("Via")
            .max_forwards(70)
            .build()
    }
}

fn response_to(request: &Request, status: u16, reason: &str) -> Message {
    Message::Response(
        ResponseBuilder::to_request(
            request,
            StatusCode::new(status).expect("a valid status"),
            reason.to_owned(),
        )
        .expect("a response")
        .build(),
    )
}

// ---------------------------------------------------------------------------------------------
// scenarios
// ---------------------------------------------------------------------------------------------

const PROXY: NodeId = NodeId::from_index(0);
const CALLER: NodeId = NodeId::from_index(3);

/// One proxy, two callees, one caller. `rings` says which callees answer with a `180`.
fn scenario(seed: u64, policy: LinkPolicy, rings: [bool; 2], cancel: bool) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);

    // The proxy is node 0, so its routes can be built before the callees exist.
    let routes = HashMap::from([
        ("sip:bob@10.0.0.1".to_owned(), NodeId::from_index(1)),
        ("sip:bob@10.0.0.2".to_owned(), NodeId::from_index(2)),
    ]);
    let targets = vec![
        Target {
            uri: Bytes::from_static(b"sip:bob@10.0.0.1"),
            route_set: Vec::new(),
            q: 1_000,
        },
        Target {
            uri: Bytes::from_static(b"sip:bob@10.0.0.2"),
            route_set: Vec::new(),
            q: 1_000,
        },
    ];

    sim.add_node(Box::new(ProxyNode::new("proxy", routes, targets)));
    sim.add_node(Box::new(Callee {
        name: "bob-1".to_owned(),
        rings: rings[0],
        saw_cancel: false,
    }));
    sim.add_node(Box::new(Callee {
        name: "bob-2".to_owned(),
        rings: rings[1],
        saw_cancel: false,
    }));
    sim.add_node(Box::new(Caller {
        name: "alice".to_owned(),
        proxy: PROXY,
        cancel_after_ringing: cancel,
        invite: None,
        final_status: None,
        cancelled: false,
    }));
    sim
}

/// Long enough for a cancelled call to conclude, far short of Timer C's 180 s.
///
/// The bound is the point. `run_until_idle` would run past Timer C, and then a `487` produced by C5
/// — the timer reaping a silent branch — would satisfy an assertion meant to be about the CANCEL.
/// That is exactly how this file's first version passed while the CANCEL was being dropped entirely.
const BEFORE_TIMER_C: Duration = Duration::from_secs(10);

#[test]
fn a_cancelled_call_ends_in_487_for_the_caller() {
    let mut sim = scenario(0x4341_4E01, LinkPolicy::CLEAN, [true, true], true);
    sim.advance(BEFORE_TIMER_C).expect("settles");

    assert_eq!(
        sim.node::<Caller>(CALLER).and_then(|c| c.final_status),
        Some(487),
        "{}",
        sim.trace().render()
    );
    // C1 — the CANCEL was acknowledged on its own transaction.
    assert!(
        sim.trace().notes().contains(&"answered cancel 200"),
        "{}",
        sim.trace().render()
    );
    // C3 — both branches were stopped, not just the one that happened to be first.
    assert!(
        sim.node::<Callee>(NodeId::from_index(1))
            .is_some_and(|c| c.saw_cancel)
    );
    assert!(
        sim.node::<Callee>(NodeId::from_index(2))
            .is_some_and(|c| c.saw_cancel)
    );
}

#[test]
fn the_cancel_path_survives_a_sweep_of_adversarial_schedules() {
    // Jitter reorders, duplication retransmits, and the two callees' provisionals race the CANCEL.
    // Every seed must reach the same *outcome* even though the traces differ.
    let policy = LinkPolicy::jittery(1, 40).with_duplication(0.25);
    for seed in 0..24_u64 {
        let mut sim = scenario(0x4341_4E00 + seed, policy, [true, true], true);
        // Bounded well below Timer C, so a 487 here can only have come from the CANCEL.
        sim.advance(BEFORE_TIMER_C)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));

        let status = sim.node::<Caller>(CALLER).and_then(|c| c.final_status);
        assert_eq!(
            status,
            Some(487),
            "seed {seed} ended at {status:?}\n{}",
            sim.trace().render()
        );
    }
}

#[test]
fn every_seed_replays_byte_for_byte() {
    let policy = LinkPolicy::jittery(1, 40).with_duplication(0.25);
    for seed in 0..8_u64 {
        let mut first = scenario(seed, policy, [true, true], true);
        let mut second = scenario(seed, policy, [true, true], true);
        first.run_until_idle().expect("settles");
        second.run_until_idle().expect("settles");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed} diverged"
        );
    }
}

#[test]
fn a_cancel_for_a_branch_that_never_answered_is_queued_until_it_does() {
    // C2/§9.1. `bob-2` stays silent, so its CANCEL cannot go out with the other one — and because
    // it never rings, it never goes out at all. The call still concludes, from the branch that did.
    let mut sim = scenario(0x4341_4E02, LinkPolicy::CLEAN, [true, false], true);
    sim.advance(BEFORE_TIMER_C).expect("settles");

    assert!(
        sim.node::<Callee>(NodeId::from_index(1))
            .is_some_and(|c| c.saw_cancel),
        "the ringing branch is cancelled"
    );
    assert!(
        !sim.node::<Callee>(NodeId::from_index(2))
            .is_some_and(|c| c.saw_cancel),
        "the silent branch must not be sent a CANCEL that could overtake its INVITE\n{}",
        sim.trace().render()
    );
}

#[test]
// The 60 is a step toward Timer C's 180 s and is followed by a 150 that is not a whole minute:
// converting only the 60 would make the two steps look unrelated to the timer they straddle.
#[allow(clippy::duration_suboptimal_units)]
fn timer_c_reaps_a_branch_that_goes_silent_and_the_call_still_concludes() {
    // C5/C6 in virtual time: neither callee ever answers, so the only thing that can conclude the
    // call is Timer C. 180 s of virtual time costs nothing here, which is the whole argument for a
    // virtual clock — the same test against a real clock would take three minutes.
    let mut sim = scenario(0x4341_4E03, LinkPolicy::CLEAN, [false, false], false);
    sim.advance(Duration::from_secs(60)).expect("runs");
    assert_eq!(
        sim.node::<Caller>(CALLER).and_then(|c| c.final_status),
        None,
        "nothing should have concluded yet"
    );

    sim.advance(Duration::from_secs(150)).expect("runs");
    assert_eq!(
        sim.node::<Caller>(CALLER).and_then(|c| c.final_status),
        Some(408),
        "C6: total silence concludes as a timeout\n{}",
        sim.trace().render()
    );
}
