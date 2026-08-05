//! Probe → edge → location lookup → echo → `200` with the marker → BYE — `ET-3`'s acceptance.
//!
//! Every component here is the **real** one: `sipx-clstr-probe`'s engine, `sipx-clstr-registrar`'s
//! store and REGISTER processing, `sipx-clstr-proxy`'s forwarding core, and the echo endpoint. The
//! only things this file supplies are the driver and the network, which is exactly the seam the
//! sans-IO design exists to create.
//!
//! It is the closest thing to M1's exit criterion that runs without sockets: `CX-3` does the same
//! shape against real phones over real transports.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_probe::echo::{EchoConfig, EchoEndpoint, Effect as EchoEffect, Input as EchoInput};
use sipx_clstr_probe::engine::{
    Effect as ProbeEffect, Input as ProbeInput, ProbeConfig, ProbeEngine, ProbeRun,
    Target as ProbeTarget,
};
use sipx_clstr_probe::{Marker, Step, Verdict};
use sipx_clstr_proxy::{
    BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig, ResponseContext,
    targets_from_lookup,
};
use sipx_clstr_registrar::{
    CanonicalAor, EdgeContext, InMemoryStore, LocationStore, TenantPolicy, Timestamp, apply,
    register_command,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{HeaderName, Message, Method, Request, Response};

const TENANT: &str = "probe-tenant";
const EDGE_HOST: &str = "edge-1.example";

// ---------------------------------------------------------------------------------------------
// the edge: the real registrar and the real forwarding core
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Edge {
    name: String,
    store: InMemoryStore,
    policy: TenantPolicy,
    contexts: HashMap<String, ResponseContext>,
    /// Which contact URI reaches which node.
    reachable: HashMap<String, NodeId>,
    upstream: HashMap<String, NodeId>,
    branch_call: HashMap<BranchId, String>,
    now: Timestamp,
    lookups: usize,
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
            branch_call: HashMap::new(),
            now: Timestamp::ZERO,
            lookups: 0,
        }
    }

    fn proxy_config() -> ProxyConfig {
        ProxyConfig::new(
            EDGE_HOST,
            Bytes::from_static(b"<sip:edge-1.example;lr>"),
            CookieKey::new(Bytes::from_static(b"probe-e2e-key")),
        )
    }

    fn call_id(headers: &sipx_sip::Headers) -> String {
        headers
            .value(&HeaderName::CallId)
            .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
            .unwrap_or_default()
    }

    fn on_register(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        let context = EdgeContext {
            tenant: TENANT.to_owned(),
            ..EdgeContext::default()
        };
        let Ok(cmd) = register_command(request, &context, self.now) else {
            return vec![reply(from, request, 400)];
        };
        let applied = apply(&self.store, &cmd, &self.policy, 3);
        vec![
            Effect::Note(format!("register {}", applied.outcome.status())),
            reply(from, request, applied.outcome.status()),
        ]
    }

    fn on_proxied(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        let call = Self::call_id(&request.headers);
        self.upstream.insert(call.clone(), from);
        let mut context = ResponseContext::new(Self::proxy_config());
        let effects = context.on_input(ProxyInput::Upstream(Box::new(request.clone())));
        self.contexts.insert(call.clone(), context);
        self.perform(&call, effects)
    }

    fn perform(&mut self, call: &str, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProxyEffect::ResolveTargets(query) => {
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
                    self.lookups += 1;
                    out.push(Effect::Note(format!("lookup → {}", found.len())));
                    let targets = targets_from_lookup(&found);
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
                        continue;
                    };
                    self.branch_call.insert(branch, call.to_owned());
                    out.push(send(node, Message::Request(*request)));
                }
                ProxyEffect::Respond(response) => {
                    if let Some(&upstream) = self.upstream.get(call) {
                        out.push(send(upstream, Message::Response(*response)));
                    }
                }
                ProxyEffect::Terminate => out.push(Effect::Note("context done".to_owned())),
                ProxyEffect::SetTimer { branch, after, .. } => {
                    if branch.is_some() {
                        out.push(Effect::SetTimer {
                            timer: TimerId(0),
                            after,
                        });
                    }
                }
                // No token is minted in this scenario, so P2 never asks. Noted rather than absorbed:
                // an unanswered verification request is a context that waits forever.
                ProxyEffect::VerifyToken { .. } => {
                    out.push(Effect::Note("unexpected token verification".to_owned()));
                }
                ProxyEffect::ClearTimer { .. }
                | ProxyEffect::CancelBranch(_)
                | ProxyEffect::AnswerCancel => {}
            }
        }
        out
    }

    fn on_branch_response(&mut self, response: &Response) -> Vec<Effect> {
        let Some(branch) = top_via_branch(response).map(BranchId) else {
            return Vec::new();
        };
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
        if !context.is_finished() {
            self.contexts.insert(call.clone(), context);
        }
        self.perform(&call, effects)
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
                    self.on_register(from, request)
                }
                Message::Request(request) => self.on_proxied(from, request),
                Message::Response(response) => self.on_branch_response(response),
            },
            _ => Vec::new(),
        }
    }
}

fn top_via_branch(response: &Response) -> Option<String> {
    let value = response.headers.get(&HeaderName::Via)?.value();
    sipx_sip::headers::Via::parse_one(&value)
        .ok()?
        .branch()
        .map(|branch| String::from_utf8_lossy(branch).into_owned())
}

fn reply(to: NodeId, request: &Request, status: u16) -> Effect {
    let response = sipx_sip::ResponseBuilder::to_request(
        request,
        sipx_sip::StatusCode::new(status).expect("a status"),
        "OK".to_owned(),
    )
    .expect("a response")
    .build();
    send(to, Message::Response(response))
}

// ---------------------------------------------------------------------------------------------
// the echo, as a node
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Echo {
    name: String,
    endpoint: EchoEndpoint,
    edge: NodeId,
}

impl SimNode for Echo {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let (effects, from) = match input {
            Input::Started => (self.endpoint.on_input(&EchoInput::Start), self.edge),
            Input::Message {
                from,
                message: Message::Request(request),
            } => (self.endpoint.on_input(&EchoInput::Request(request)), from),
            Input::Timer(_) => (self.endpoint.on_input(&EchoInput::RefreshDue), self.edge),
            // The `200` to its own REGISTER needs nothing done about it, and a transport error on a
            // registration is handled by the refresh timer coming round again.
            Input::Message { .. } | Input::TransportError { .. } => return Vec::new(),
        };

        let mut out = Vec::new();
        for effect in effects {
            match effect {
                EchoEffect::Send(request) => out.push(send(self.edge, Message::Request(*request))),
                EchoEffect::Respond(response) => {
                    out.push(send(from, Message::Response(*response)));
                }
                EchoEffect::SetRefresh(after) => out.push(Effect::SetTimer {
                    timer: TimerId(0),
                    after,
                }),
                EchoEffect::Refused(refusal) => {
                    out.push(Effect::Note(format!("echo refused {refusal:?}")));
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// the probe, as a node
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Probe {
    name: String,
    engine: ProbeEngine,
    edge: NodeId,
    run: Option<ProbeRun>,
}

impl Probe {
    fn perform(&mut self, effects: Vec<ProbeEffect>, now: SimTime) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProbeEffect::Resolve(_) => {
                    let more = self.engine.on_input(
                        Duration::from_nanos(now.as_nanos()),
                        ProbeInput::Resolved(Ok(())),
                    );
                    out.extend(self.perform(more, now));
                }
                ProbeEffect::Send(request) => out.push(send(self.edge, Message::Request(*request))),
                ProbeEffect::SetTimer { step, after } => out.push(Effect::SetTimer {
                    timer: timer_of(step),
                    after,
                }),
                ProbeEffect::ClearTimer(step) => out.push(Effect::ClearTimer(timer_of(step))),
                ProbeEffect::Finish(run) => {
                    out.push(Effect::Note(format!("verdict {}", run.verdict)));
                    self.run = Some(*run);
                }
            }
        }
        out
    }
}

/// The timer that starts the run, distinct from the plan's four.
const START_TIMER: TimerId = TimerId(9);

fn timer_of(step: Step) -> TimerId {
    TimerId(match step {
        Step::Resolve => 0,
        Step::Register => 1,
        Step::Invite => 2,
        Step::Bye => 3,
    })
}

impl SimNode for Probe {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let clock = Duration::from_nanos(now.as_nanos());
        let effects = match input {
            // The probe waits before starting, so the echo's registration is in the store by the
            // time the probe's call needs looking up. A real deployment gets that from the echo
            // having been up for hours; here it is one second, expressed as the probe's own timer
            // rather than as a second node whose only job is to poke it.
            Input::Started => {
                return vec![Effect::SetTimer {
                    timer: START_TIMER,
                    after: Duration::from_secs(1),
                }];
            }
            Input::Timer(START_TIMER) => self.engine.on_input(clock, ProbeInput::Start),
            Input::Timer(timer) => {
                let step = match timer.0 {
                    0 => Step::Resolve,
                    1 => Step::Register,
                    2 => Step::Invite,
                    3 => Step::Bye,
                    _ => return Vec::new(),
                };
                self.engine.on_input(clock, ProbeInput::TimerFired(step))
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => self
                .engine
                .on_input(clock, ProbeInput::Response(Box::new(response.clone()))),
            _ => return Vec::new(),
        };
        self.perform(effects, now)
    }
}

// ---------------------------------------------------------------------------------------------
// the scenario
// ---------------------------------------------------------------------------------------------

const EDGE: NodeId = NodeId::from_index(0);
const ECHO: NodeId = NodeId::from_index(1);
const PROBE: NodeId = NodeId::from_index(2);

fn scenario(seed: u64, policy: LinkPolicy, marker: &Marker) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);

    let reachable = HashMap::from([("sip:echo@10.8.8.8".to_owned(), ECHO)]);
    sim.add_node(Box::new(Edge::new("edge", reachable)));
    sim.add_node(Box::new(Echo {
        name: "echo".to_owned(),
        endpoint: EchoEndpoint::new(EchoConfig::new(
            "sip:echo@probe.example",
            "sip:echo@10.8.8.8",
            "sip:probe.example",
        )),
        edge: EDGE,
    }));

    let mut probe = ProbeEngine::new(
        ProbeConfig::new(
            "sip:probe@probe.example",
            "sip:echo@probe.example",
            "sip:probe@10.9.9.9",
            ProbeTarget {
                address: "probe.example".to_owned(),
                transport: "UDP".to_owned(),
                zone: "zone-a".to_owned(),
            },
        )
        .sent_by("probe.sim"),
        marker.clone(),
    );
    // Nothing has run yet; the engine is idle until the starter's timer fires.
    let _ = &mut probe;
    sim.add_node(Box::new(Probe {
        name: "probe".to_owned(),
        engine: probe,
        edge: EDGE,
        run: None,
    }));
    sim
}

/// Run the scenario to completion.
// One probe interval of virtual time, sized against the scheduler's 60 s cadence, so it stays in
// the same unit as the cadence it is chosen to cover.
#[allow(clippy::duration_suboptimal_units)]
fn run(seed: u64, policy: LinkPolicy) -> Sim {
    let marker = Marker::from_token("e2e-marker");
    let mut sim = scenario(seed, policy, &marker);
    sim.advance(Duration::from_secs(60))
        .expect("the run settles");
    sim
}

#[test]
fn a_probe_run_traverses_the_real_platform_and_passes() {
    let sim = run(0x_0e33_0001, LinkPolicy::CLEAN);

    let probe = sim.node::<Probe>(PROBE).expect("the probe");
    let outcome = probe.run.as_ref().expect("the run should finish");
    assert_eq!(
        outcome.verdict,
        Verdict::Pass,
        "probe → edge → lookup → echo → 200 with marker → BYE\n{}",
        sim.trace().render()
    );

    // The path really was traversed: the edge looked the echo up rather than the probe reaching it
    // directly, and the echo answered a marked call.
    let edge = sim.node::<Edge>(EDGE).expect("the edge");
    assert!(edge.lookups > 0, "the location service was consulted");
    let echo = sim.node::<Echo>(ECHO).expect("the echo");
    assert_eq!(echo.endpoint.answered(), 1);
    assert_eq!(echo.endpoint.refused(), 0);
}

#[test]
fn the_end_to_end_run_replays_byte_for_byte_from_its_seed() {
    let policy = LinkPolicy::jittery(1, 25);
    for seed in 0..6_u64 {
        let first = run(seed, policy);
        let second = run(seed, policy);
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed} diverged"
        );
    }
}

#[test]
fn an_unmarked_call_to_the_echo_is_refused_even_though_it_arrived_through_the_platform() {
    // §9 E5, through the real path: the test tenant is not a bypass. The platform routed the call
    // correctly and the echo still refused it, which is the division of responsibility the spec
    // describes — the platform decides *reachability*, the echo decides *whether this is a probe*.
    let marker = Marker::from_token("e2e-marker");
    let mut sim = scenario(0x_0e33_0003, LinkPolicy::CLEAN, &marker);
    sim.advance(Duration::from_secs(1)).expect("echo registers");

    // An INVITE with no marker, injected as though a stranger had dialled the echo's address.
    let stranger = sipx_sip::RequestBuilder::new(
        Method::Invite,
        sipx_sip::Uri::parse(Bytes::from_static(b"sip:echo@10.8.8.8")).expect("a URI"),
    )
    .header(HeaderName::CallId, "stranger")
    .expect("Call-ID")
    .cseq(1, &Method::Invite)
    .expect("CSeq")
    .header(HeaderName::From, "<sip:someone@elsewhere.example>;tag=s")
    .expect("From")
    .header(HeaderName::To, "<sip:echo@probe.example>")
    .expect("To")
    .header(HeaderName::Via, "SIP/2.0/UDP elsewhere;branch=z9hG4bK-s")
    .expect("Via")
    .build();

    sim.inject(EDGE, ECHO, &Message::Request(stranger));
    sim.advance(Duration::from_secs(5)).expect("settles");

    let echo = sim.node::<Echo>(ECHO).expect("the echo");
    assert_eq!(
        echo.endpoint.refused(),
        1,
        "the stranger's call is refused\n{}",
        sim.trace().render()
    );
    // And the probe's own call, running in the same scenario, is answered — which is the stronger
    // statement: the echo is *distinguishing*, not simply closed.
    assert_eq!(echo.endpoint.answered(), 1);
}
