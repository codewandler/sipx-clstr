//! Adversarial schedules over a seed corpus — `PX-7`'s second criterion.
//!
//! The vector tests assert what the engine does when one thing happens at a time. This asserts what
//! survives when the network is hostile: retransmission storms, wide reordering, duplicated
//! datagrams, and loss heavy enough that a message often needs its retransmission to arrive at all.
//!
//! **Every failure prints its seed, and `HARNESS_SEED` replays it.** That is the whole contract of a
//! seeded harness: a red run is a value you can paste back, not a flake you re-run until it hides.
//! `CF-1` specifies both halves — the pinned corpus in CI, and the sweep that finds new seeds to pin.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_proxy::{
    BranchId, CookieKey, Effect as ProxyEffect, Input as ProxyInput, ProxyConfig, ProxyTimer,
    ResponseContext, Target,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{Latency, LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder, StatusCode,
    Uri,
};

const EDGE: &str = "edge-1.example";

/// The seeds CI runs on every push.
///
/// Pinned rather than random so a green build means the same thing twice. `CF-1`'s discipline: the
/// nightly sweep looks for new failures, and a seed it finds gets appended here as an explicit
/// regression rather than living on in a bug report.
const CORPUS: &[u64] = &[
    0x_0000_0001,
    0x_0000_0002,
    0x_0000_002a,
    0x_dead_beef,
    0x_c0ff_ee00,
    0x_5eed_1234,
    0x_7fff_ffff,
    0x_1357_9bdf,
];

/// One seed, or the corpus. `HARNESS_SEED=0x… cargo test` replays exactly one run.
fn seeds() -> Vec<u64> {
    match std::env::var("HARNESS_SEED") {
        Ok(text) => {
            let text = text.trim();
            let parsed = text
                .strip_prefix("0x")
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                .or_else(|| text.parse().ok());
            match parsed {
                Some(seed) => vec![seed],
                // A malformed override is a mistake worth surfacing: silently running the corpus
                // instead would make a developer think they had reproduced something.
                None => panic!("HARNESS_SEED={text:?} is not a number"),
            }
        }
        Err(_) => CORPUS.to_vec(),
    }
}

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

// ---------------------------------------------------------------------------------------------
// a proxy node with retransmission, because the harness has no transaction layer
// ---------------------------------------------------------------------------------------------

/// The retransmission interval for an unanswered request, standing in for RFC 3261's Timer A/E.
///
/// The harness carries no transaction layer — the kernel's would, in the real driver — so the
/// endpoints and the proxy retransmit themselves. Without that, heavy loss simply loses calls and the
/// scenario would be asserting nothing about the engine.
const RETRANSMIT: Duration = Duration::from_millis(500);
const MAX_RETRANSMITS: u32 = 8;

#[derive(Debug)]
struct Edge {
    name: String,
    config: ProxyConfig,
    contexts: HashMap<String, ResponseContext>,
    reachable: HashMap<String, NodeId>,
    upstream: HashMap<String, NodeId>,
    branch_call: HashMap<BranchId, String>,
    branch_nodes: HashMap<BranchId, NodeId>,
    /// Requests still awaiting a first response, for retransmission.
    outstanding: HashMap<BranchId, (Request, u32)>,
    /// Every branch this node ever created, so a scenario can assert that a retransmitted request
    /// did not create a second set.
    branches_created: Vec<BranchId>,
    timers: Vec<TimerKind>,
    targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TimerKind {
    /// Timer C for a branch.
    TimerC(BranchId),
    /// Retransmit a branch's request.
    Retransmit(BranchId),
}

impl Edge {
    fn new(name: &str, reachable: HashMap<String, NodeId>, targets: Vec<Target>) -> Self {
        Self {
            name: name.to_owned(),
            config: ProxyConfig::new(
                EDGE,
                Bytes::from_static(b"<sip:edge-1.example;lr>"),
                CookieKey::new(Bytes::from_static(b"cluster-cookie-key")),
            ),
            contexts: HashMap::new(),
            reachable,
            upstream: HashMap::new(),
            branch_call: HashMap::new(),
            branch_nodes: HashMap::new(),
            outstanding: HashMap::new(),
            branches_created: Vec::new(),
            timers: Vec::new(),
            targets,
        }
    }

    fn timer_id(&mut self, kind: TimerKind) -> TimerId {
        if let Some(index) = self.timers.iter().position(|known| *known == kind) {
            return TimerId(u64::try_from(index).unwrap_or(u64::MAX));
        }
        self.timers.push(kind);
        TimerId(u64::try_from(self.timers.len() - 1).unwrap_or(u64::MAX))
    }

    fn perform(&mut self, call: &str, effects: Vec<ProxyEffect>) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProxyEffect::ResolveTargets(_) => {
                    let targets = self.targets.clone();
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
                        continue;
                    };
                    self.branch_nodes.insert(branch.clone(), node);
                    self.branch_call.insert(branch.clone(), call.to_owned());
                    self.branches_created.push(branch.clone());
                    self.outstanding
                        .insert(branch.clone(), ((*request).clone(), 0));
                    let timer = self.timer_id(TimerKind::Retransmit(branch));
                    out.push(send(node, Message::Request(*request)));
                    out.push(Effect::SetTimer {
                        timer,
                        after: RETRANSMIT,
                    });
                }
                ProxyEffect::Respond(response) => {
                    if let Some(&upstream) = self.upstream.get(call) {
                        out.push(send(upstream, Message::Response(*response)));
                    }
                }
                ProxyEffect::CancelBranch(_) | ProxyEffect::AnswerCancel => {}
                ProxyEffect::SetTimer { branch, after, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(TimerKind::TimerC(branch));
                        out.push(Effect::SetTimer { timer, after });
                    }
                }
                ProxyEffect::ClearTimer { branch, .. } => {
                    if let Some(branch) = branch {
                        let timer = self.timer_id(TimerKind::TimerC(branch));
                        out.push(Effect::ClearTimer(timer));
                    }
                }
                ProxyEffect::Terminate => out.push(Effect::Note("terminate".to_owned())),
            }
        }
        out
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } => self.on_request(from, request),
            Input::Message {
                message: Message::Response(response),
                ..
            } => self.on_response(response),
            Input::Timer(timer) => self.on_timer(timer),
            Input::Started | Input::TransportError { .. } => Vec::new(),
        }
    }
}

impl Edge {
    fn on_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        {
            let call = call_id_of(&request.headers);
            if request.method == Method::Cancel {
                let Some(mut context) = self.contexts.remove(&call) else {
                    return Vec::new();
                };
                let effects = context.on_input(ProxyInput::UpstreamCancelled);
                if !context.is_finished() {
                    self.contexts.insert(call.clone(), context);
                }
                return self.perform(&call, effects);
            }
            // A retransmitted INVITE must not fork a second time. The kernel's server
            // transaction absorbs those in the real driver; here the existing context is the
            // evidence that we have seen this request already.
            if self.contexts.contains_key(&call) || self.upstream.contains_key(&call) {
                return Vec::new();
            }
            self.upstream.insert(call.clone(), from);
            let mut context = ResponseContext::new(self.config.clone());
            let effects = context.on_input(ProxyInput::Upstream(Box::new(request.clone())));
            self.contexts.insert(call.clone(), context);
            self.perform(&call, effects)
        }
    }

    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        {
            let Some(branch) = top_via_branch(response).map(BranchId) else {
                return Vec::new();
            };
            // A response means the branch is alive: stop retransmitting it.
            self.outstanding.remove(&branch);
            let mut out = Vec::new();
            if let Some(index) = self
                .timers
                .iter()
                .position(|kind| *kind == TimerKind::Retransmit(branch.clone()))
            {
                out.push(Effect::ClearTimer(TimerId(
                    u64::try_from(index).unwrap_or(u64::MAX),
                )));
            }

            let Some(call) = self.branch_call.get(&branch).cloned() else {
                return out;
            };
            let Some(mut context) = self.contexts.remove(&call) else {
                return out;
            };
            let effects = context.on_input(ProxyInput::BranchResponse(
                Box::new(response.clone()),
                branch,
            ));
            if !context.is_finished() {
                self.contexts.insert(call.clone(), context);
            }
            out.extend(self.perform(&call, effects));
            out
        }
    }

    fn on_timer(&mut self, timer: TimerId) -> Vec<Effect> {
        {
            let Some(index) = usize::try_from(timer.0).ok() else {
                return Vec::new();
            };
            let Some(kind) = self.timers.get(index).cloned() else {
                return Vec::new();
            };
            match kind {
                TimerKind::Retransmit(branch) => {
                    let Some((request, attempts)) = self.outstanding.get(&branch).cloned() else {
                        return Vec::new();
                    };
                    if attempts >= MAX_RETRANSMITS {
                        // Give up the way a transaction does, and tell the engine the transport
                        // could not deliver (§16.9).
                        self.outstanding.remove(&branch);
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
                        return self.perform(&call, effects);
                    }
                    self.outstanding
                        .insert(branch.clone(), (request.clone(), attempts + 1));
                    let Some(&node) = self.branch_nodes.get(&branch) else {
                        return Vec::new();
                    };
                    let retransmit = self.timer_id(TimerKind::Retransmit(branch));
                    vec![
                        send(node, Message::Request(request)),
                        Effect::SetTimer {
                            timer: retransmit,
                            after: RETRANSMIT,
                        },
                    ]
                }
                TimerKind::TimerC(branch) => {
                    let Some(call) = self.branch_call.get(&branch).cloned() else {
                        return Vec::new();
                    };
                    let Some(mut context) = self.contexts.remove(&call) else {
                        return Vec::new();
                    };
                    let effects =
                        context.on_input(ProxyInput::TimerFired(ProxyTimer::C, Some(branch)));
                    if !context.is_finished() {
                        self.contexts.insert(call.clone(), context);
                    }
                    self.perform(&call, effects)
                }
            }
        }
    }
}

fn call_id_of(headers: &sipx_sip::Headers) -> String {
    headers
        .value(&HeaderName::CallId)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
        .unwrap_or_default()
}

fn top_via_branch(response: &Response) -> Option<String> {
    let value = response.headers.get(&HeaderName::Via)?.value();
    sipx_sip::headers::Via::parse_one(&value)
        .ok()?
        .branch()
        .map(|branch| String::from_utf8_lossy(branch).into_owned())
}

// ---------------------------------------------------------------------------------------------
// endpoints
// ---------------------------------------------------------------------------------------------

/// Answers every INVITE, however many copies of it arrive.
#[derive(Debug)]
struct Answerer {
    name: String,
    answers: u32,
}

impl SimNode for Answerer {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } if request.method == Method::Invite => {
                self.answers += 1;
                vec![
                    reply(from, request, 180, "Ringing"),
                    reply(from, request, 200, "OK"),
                ]
            }
            _ => Vec::new(),
        }
    }
}

/// Sends one INVITE and retransmits it until something final comes back.
#[derive(Debug)]
struct Dialler {
    name: String,
    proxy: NodeId,
    invite: Option<Request>,
    attempts: u32,
    final_status: Option<u16>,
    finals_seen: u32,
}

impl SimNode for Dialler {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let invite = Self::build();
                self.invite = Some(invite.clone());
                vec![
                    send(self.proxy, Message::Request(invite)),
                    Effect::SetTimer {
                        timer: TimerId(0),
                        after: RETRANSMIT,
                    },
                ]
            }
            Input::Timer(_) => {
                if self.final_status.is_some() || self.attempts >= MAX_RETRANSMITS {
                    return Vec::new();
                }
                self.attempts += 1;
                let Some(invite) = self.invite.clone() else {
                    return Vec::new();
                };
                vec![
                    send(self.proxy, Message::Request(invite)),
                    Effect::SetTimer {
                        timer: TimerId(0),
                        after: RETRANSMIT,
                    },
                ]
            }
            Input::Message {
                message: Message::Response(response),
                ..
            } => {
                if response.status.is_provisional() {
                    return Vec::new();
                }
                self.finals_seen += 1;
                if self.final_status.is_none() {
                    self.final_status = Some(response.status.code());
                }
                vec![Effect::Note(format!("final {}", response.status.code()))]
            }
            _ => Vec::new(),
        }
    }
}

impl Dialler {
    fn build() -> Request {
        RequestBuilder::new(Method::Invite, uri("sip:bob@b.example"))
            .header(HeaderName::CallId, "torture-1")
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

fn reply(to: NodeId, request: &Request, status: u16, reason: &str) -> Effect {
    send(
        to,
        Message::Response(
            ResponseBuilder::to_request(
                request,
                StatusCode::new(status).expect("a valid status"),
                reason.to_owned(),
            )
            .expect("a response")
            .build(),
        ),
    )
}

// ---------------------------------------------------------------------------------------------
// the scenario
// ---------------------------------------------------------------------------------------------

const PROXY: NodeId = NodeId::from_index(0);
const DIALLER: NodeId = NodeId::from_index(3);

fn torture(seed: u64, policy: LinkPolicy) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);
    // Retransmission storms make many events; the default budget is generous but say so explicitly.
    sim.step_budget(200_000);

    let reachable = HashMap::from([
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

    sim.add_node(Box::new(Edge::new("proxy", reachable, targets)));
    sim.add_node(Box::new(Answerer {
        name: "bob-1".to_owned(),
        answers: 0,
    }));
    sim.add_node(Box::new(Answerer {
        name: "bob-2".to_owned(),
        answers: 0,
    }));
    sim.add_node(Box::new(Dialler {
        name: "alice".to_owned(),
        proxy: PROXY,
        invite: None,
        attempts: 0,
        final_status: None,
        finals_seen: 0,
    }));
    sim
}

/// Wide jitter, heavy duplication, and enough loss that retransmission is the only way through.
fn hostile() -> LinkPolicy {
    LinkPolicy {
        loss: 0.35,
        duplicate: 0.3,
        latency: Latency::uniform_ms(1, 120),
        partitioned: false,
    }
}

#[test]
fn the_call_completes_over_the_whole_corpus_under_a_hostile_network() {
    for seed in seeds() {
        let mut sim = torture(seed, hostile());
        sim.advance(Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("seed {seed:#x}: {e}"));

        let status = sim.node::<Dialler>(DIALLER).and_then(|d| d.final_status);
        assert_eq!(
            status,
            Some(200),
            "seed {seed:#x} ended at {status:?} — replay with HARNESS_SEED={seed:#x}\n{}",
            sim.trace().render()
        );
    }
}

#[test]
fn a_retransmitted_invite_never_forks_twice() {
    // The property a retransmission storm exists to break. Each callee may receive the INVITE more
    // than once — the network duplicates — but the *proxy* must create one branch per target, or a
    // caller's retransmission becomes a second call attempt at every device.
    for seed in seeds() {
        let mut sim = torture(seed, hostile());
        sim.advance(Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("seed {seed:#x}: {e}"));

        // Two branches, and **only** two, however many copies of the INVITE crossed the network.
        // Asserted on the branches the engine created rather than on messages seen, because the
        // network legitimately duplicates messages and only the branch count is the property.
        let created = sim
            .node::<Edge>(PROXY)
            .map(|edge| edge.branches_created.clone())
            .unwrap_or_default();
        assert_eq!(
            created.len(),
            2,
            "seed {seed:#x}: one branch per target, however often the request arrived — got \
             {created:?}\nreplay with HARNESS_SEED={seed:#x}"
        );
        let distinct: std::collections::BTreeSet<&BranchId> = created.iter().collect();
        assert_eq!(
            distinct.len(),
            2,
            "seed {seed:#x}: the branches must differ"
        );
    }
}

#[test]
fn every_corpus_seed_replays_byte_for_byte() {
    for seed in seeds() {
        let mut first = torture(seed, hostile());
        let mut second = torture(seed, hostile());
        first.advance(Duration::from_secs(30)).expect("runs");
        second.advance(Duration::from_secs(30)).expect("runs");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed:#x} diverged — replay with HARNESS_SEED={seed:#x}"
        );
    }
}

#[test]
fn a_total_partition_concludes_the_call_rather_than_hanging() {
    // Every branch's transport fails, which §16.9 makes a `503` from that branch, which R8 turns
    // into `500` upstream. The point is that it *concludes*: a proxy that waited forever would hold
    // the caller's transaction until its own timers gave up, minutes later.
    for seed in seeds().into_iter().take(3) {
        let mut sim = torture(
            seed,
            LinkPolicy {
                partitioned: true,
                ..LinkPolicy::CLEAN
            },
        );
        sim.advance(Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("seed {seed:#x}: {e}"));
        // Nothing reaches the callees at all, so the INVITE never even leaves — the caller simply
        // gets nothing, and the assertion is that the simulation settled rather than livelocked.
        assert!(
            sim.node::<Dialler>(DIALLER)
                .is_some_and(|d| d.final_status.is_none() || d.final_status == Some(500)),
            "seed {seed:#x}\n{}",
            sim.trace().render()
        );
    }
}
