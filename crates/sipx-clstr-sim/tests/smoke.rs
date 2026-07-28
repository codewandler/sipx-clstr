//! The harness's own smoke test: one node, two endpoints, a registration and a call.
//!
//! The logic here is **stubs**, deliberately. `RG-3` brings the real registrar and `PX-5` the real
//! forwarding core, and when they do, these two node types are deleted rather than adapted. What
//! this scenario proves is the *harness*: that a message serialized on one node parses on another,
//! that a call completes across virtual time, that the same seed replays byte for byte, and that
//! loss and jitter are things a scenario can turn on and reason about.
//!
//! It is also the shape every later scenario takes, so it is worth reading as an example.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_sim::node::{Effect, Input, SimNode, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    Header, HeaderName, Message, Method, Request, RequestBuilder, Response, ResponseBuilder,
    StatusCode, Uri,
};

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

fn header_text(message: &Message, name: &HeaderName) -> Option<String> {
    message
        .headers()
        .value(name)
        .map(|value| String::from_utf8_lossy(&value).trim().to_owned())
}

/// The user part of a `sip:` URI — the stub's stand-in for `RG-1`'s canonical address-of-record form, which is
/// a great deal more careful than this and is `RG-3`'s job to implement.
fn user_of(uri: &Uri) -> String {
    uri.user()
        .map(|user| String::from_utf8_lossy(user).into_owned())
        .unwrap_or_default()
}

/// Remove the topmost `Via`, by rebuilding the header collection around it.
///
/// This is the operation sipx `S-15` exists to make unnecessary: `Headers` can push and it can
/// remove *every* occurrence of a name, but it cannot remove the first one, so popping one `Via`
/// means copying all the others. Correct, and O(n) clones per hop. When `S-15` lands this becomes
/// a call.
fn pop_top_via(response: &mut Response) {
    let mut rebuilt = sipx_sip::Headers::new();
    let mut popped = false;
    for existing in response.headers.iter() {
        if !popped && existing.name() == &HeaderName::Via {
            popped = true;
        } else {
            rebuilt.push(existing.clone());
        }
    }
    response.headers = rebuilt;
}

// ---------------------------------------------------------------------------------------------
// a stub edge: registrar and forwarder in one, with none of the rules
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct StubEdge {
    name: String,
    /// Where each registered user can be reached. The real one is a `LocationStore` with a
    /// compare-and-swap contract; this is a map.
    bindings: HashMap<String, NodeId>,
    /// Which node a branch's response should go back to. The real proxy gets this from the
    /// response context its server transaction owns.
    upstream: HashMap<String, NodeId>,
    next_branch: u64,
}

impl StubEdge {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            bindings: HashMap::new(),
            upstream: HashMap::new(),
            next_branch: 0,
        }
    }

    fn respond(request: &Request, code: u16, reason: &str) -> Response {
        ResponseBuilder::to_request(
            request,
            StatusCode::new(code).expect("a valid status code"),
            reason.to_owned(),
        )
        .expect("a response to a well-formed request")
        .build()
    }

    fn on_request(&mut self, from: NodeId, request: &Request) -> Vec<Effect> {
        if request.method == Method::Register {
            let contact = header_text(&Message::Request(request.clone()), &HeaderName::To)
                .unwrap_or_default();
            let user = contact
                .trim_start_matches('<')
                .trim_end_matches('>')
                .rsplit_once('@')
                .and_then(|(left, _)| left.rsplit_once(':'))
                .map(|(_, user)| user.to_owned())
                .unwrap_or_default();
            self.bindings.insert(user.clone(), from);
            return vec![
                Effect::Note(format!("bound {user}")),
                send(from, Message::Response(Self::respond(request, 200, "OK"))),
            ];
        }

        // ACK for a 2xx is a separate transaction end to end: forwarded, never answered.
        let target = self.bindings.get(&user_of(&request.uri)).copied();
        let Some(target) = target else {
            if request.method == Method::Ack {
                return vec![Effect::Note("ack for an unknown target".to_owned())];
            }
            return vec![send(
                from,
                Message::Response(Self::respond(request, 404, "Not Found")),
            )];
        };

        let branch = format!("z9hG4bK-sim-{}", self.next_branch);
        self.next_branch += 1;
        self.upstream.insert(branch.clone(), from);

        let mut forwarded = request.clone();
        // Push our own `Via`, which is what makes the response find its way back. The real engine
        // also decrements `Max-Forwards`, applies `Route`, and computes an RFC 5393 loop-detection
        // cookie into this branch — all of it `PX-5`'s work, none of it here.
        forwarded.headers.push_front(
            Header::build(
                HeaderName::Via,
                format!("SIP/2.0/UDP {}.sim;branch={branch}", self.name),
            )
            .expect("a well-formed Via"),
        );

        vec![
            Effect::Note(format!("forward {} to {target}", user_of(&request.uri))),
            send(target, Message::Request(forwarded)),
        ]
    }

    fn on_response(&mut self, response: &Response) -> Vec<Effect> {
        let branch = header_text(&Message::Response(response.clone()), &HeaderName::Via)
            .and_then(|via| {
                via.split(";branch=")
                    .nth(1)
                    .map(|rest| rest.split(';').next().unwrap_or(rest).to_owned())
            })
            .unwrap_or_default();

        let Some(&upstream) = self.upstream.get(&branch) else {
            // The gap sipx `T-18` is about, in miniature: a response whose context is gone. A
            // stateful proxy is required to forward it statelessly rather than drop it, and this
            // stub cannot, because it has nowhere to send it.
            return vec![Effect::Note("response with no context".to_owned())];
        };

        let mut forwarded = response.clone();
        pop_top_via(&mut forwarded);
        vec![send(upstream, Message::Response(forwarded))]
    }
}

impl SimNode for StubEdge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message { from, message } => match message {
                Message::Request(request) => self.on_request(from, request),
                Message::Response(response) => self.on_response(response),
            },
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// a stub endpoint
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Registers, then calls the other endpoint.
    Caller,
    /// Registers and waits to be called.
    Callee,
}

#[derive(Debug)]
struct StubUa {
    name: String,
    role: Role,
    edge: NodeId,
    peer: String,
    registered: bool,
    call_answered: bool,
    call_ended: bool,
    cseq: u32,
}

impl StubUa {
    fn new(name: &str, role: Role, edge: NodeId, peer: &str) -> Self {
        Self {
            name: name.to_owned(),
            role,
            edge,
            peer: peer.to_owned(),
            registered: false,
            call_answered: false,
            call_ended: false,
            cseq: 0,
        }
    }

    fn request(&mut self, method: &Method, target: &str) -> Message {
        self.cseq += 1;
        Message::Request(
            RequestBuilder::new(method.clone(), uri(target))
                .header(HeaderName::CallId, format!("call-{}", self.name))
                .expect("a valid Call-ID")
                .cseq(self.cseq, method)
                .expect("a valid CSeq")
                .header(
                    HeaderName::From,
                    format!("<sip:{}@sim.test>;tag={}", self.name, self.name),
                )
                .expect("a valid From")
                .header(HeaderName::To, format!("<{target}>"))
                .expect("a valid To")
                .header(
                    HeaderName::Via,
                    format!("SIP/2.0/UDP {}.sim;branch=z9hG4bK-{}", self.name, self.cseq),
                )
                .expect("a valid Via")
                .max_forwards(70)
                .build(),
        )
    }
}

impl SimNode for StubUa {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => {
                let target = format!("sip:{}@sim.test", self.name);
                let register = self.request(&Method::Register, &target);
                vec![send(self.edge, register)]
            }

            Input::Message {
                message: Message::Response(response),
                ..
            } => {
                if !response.status.is_success() {
                    return vec![Effect::Note(format!("failed {}", response.status.code()))];
                }
                let cseq = header_text(&Message::Response(response.clone()), &HeaderName::CSeq)
                    .unwrap_or_default();

                if cseq.ends_with("REGISTER") {
                    self.registered = true;
                    if self.role == Role::Caller {
                        let target = format!("sip:{}@sim.test", self.peer);
                        let invite = self.request(&Method::Invite, &target);
                        return vec![
                            Effect::Note("registered".to_owned()),
                            send(self.edge, invite),
                        ];
                    }
                    return vec![Effect::Note("registered".to_owned())];
                }

                if cseq.ends_with("INVITE") {
                    self.call_answered = true;
                    let target = format!("sip:{}@sim.test", self.peer);
                    // ACK then BYE. The ACK for a 2xx is end to end, which is why it goes through
                    // the edge as an ordinary request rather than being absorbed by it.
                    let ack = self.request(&Method::Ack, &target);
                    let bye = self.request(&Method::Bye, &target);
                    return vec![
                        Effect::Note("answered".to_owned()),
                        send(self.edge, ack),
                        send(self.edge, bye),
                    ];
                }

                if cseq.ends_with("BYE") {
                    self.call_ended = true;
                    return vec![Effect::Note("hung up".to_owned())];
                }

                Vec::new()
            }

            Input::Message {
                from,
                message: Message::Request(request),
            } => match request.method {
                Method::Ack => vec![Effect::Note("acked".to_owned())],
                Method::Bye => {
                    self.call_ended = true;
                    vec![
                        Effect::Note("released".to_owned()),
                        send(
                            from,
                            Message::Response(StubEdge::respond(request, 200, "OK")),
                        ),
                    ]
                }
                Method::Invite => vec![
                    Effect::Note("ringing".to_owned()),
                    send(
                        from,
                        Message::Response(StubEdge::respond(request, 200, "OK")),
                    ),
                ],
                _ => vec![send(
                    from,
                    Message::Response(StubEdge::respond(request, 200, "OK")),
                )],
            },

            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the scenario
// ---------------------------------------------------------------------------------------------

/// One edge, two endpoints, both registering, one calling the other.
fn scenario(seed: u64, policy: LinkPolicy) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);
    let edge = sim.add_node(Box::new(StubEdge::new("edge")));
    sim.add_node(Box::new(StubUa::new("alice", Role::Caller, edge, "bob")));
    sim.add_node(Box::new(StubUa::new("bob", Role::Callee, edge, "alice")));
    sim
}

const EDGE: NodeId = NodeId::from_index(0);
const ALICE: NodeId = NodeId::from_index(1);
const BOB: NodeId = NodeId::from_index(2);

#[test]
fn two_endpoints_register_and_a_call_completes_through_one_node() {
    let mut sim = scenario(0x5157_0001, LinkPolicy::CLEAN);
    sim.run_until_idle().expect("the scenario should settle");

    let alice = sim.node::<StubUa>(ALICE).expect("alice");
    let bob = sim.node::<StubUa>(BOB).expect("bob");
    assert!(
        alice.registered,
        "alice did not register\n{}",
        sim.trace().render()
    );
    assert!(bob.registered, "bob did not register");
    assert!(alice.call_answered, "the call was never answered");
    assert!(alice.call_ended && bob.call_ended, "the call never ended");

    let edge = sim.node::<StubEdge>(EDGE).expect("edge");
    assert_eq!(edge.bindings.len(), 2, "both endpoints should be bound");

    // Bob saw the INVITE, the ACK and the BYE — the call really crossed the node rather than
    // being answered by it.
    let seen = sim.trace().received_by(BOB);
    assert!(
        seen.iter().any(|line| line.starts_with("INVITE")),
        "{seen:?}"
    );
    assert!(seen.iter().any(|line| line.starts_with("ACK")), "{seen:?}");
    assert!(seen.iter().any(|line| line.starts_with("BYE")), "{seen:?}");
}

#[test]
fn the_whole_scenario_replays_byte_for_byte_from_its_seed() {
    // With jitter and duplication on, so there is plenty for a nondeterministic implementation to
    // get wrong: reordering, retransmission-shaped duplicates, and interleaved endpoints.
    let policy = LinkPolicy::jittery(1, 30).with_duplication(0.3);
    let mut first = scenario(0x5157_0002, policy);
    let mut second = scenario(0x5157_0002, policy);
    first.run_until_idle().expect("settles");
    second.run_until_idle().expect("settles");

    let (a, b) = (first.trace().render(), second.trace().render());
    assert_eq!(a, b, "same seed produced different traces");
    assert!(
        a.contains("duplicated"),
        "the seed should have duplicated something"
    );
}

#[test]
fn a_lossy_network_is_visible_in_the_trace_rather_than_silent() {
    // No retransmission anywhere in these stubs — the transaction machines that would retry are
    // `PX-5`'s. So loss here means an unfinished call, and the point is that the trace *says* so
    // instead of the scenario just quietly not completing.
    let mut sim = scenario(0x5157_0003, LinkPolicy::CLEAN.with_loss(0.5));
    sim.run_until_idle().expect("settles");

    let dropped = sim
        .trace()
        .count(|entry| matches!(entry.event, sipx_clstr_sim::trace::Event::Dropped { .. }));
    assert!(dropped > 0, "half the datagrams should not have survived");

    let alice = sim.node::<StubUa>(ALICE).expect("alice");
    assert!(
        !alice.call_ended,
        "a call should not complete through this much loss"
    );
}

#[test]
fn latency_puts_the_call_where_the_clock_says_it_should_be() {
    // Ten milliseconds each way, so every hop is countable and the total is not a mystery.
    let mut sim = scenario(0x5157_0004, LinkPolicy::jittery(10, 10));
    sim.run_until(SimTime::from_millis(25))
        .expect("runs to the deadline");
    assert!(
        !sim.node::<StubUa>(ALICE).expect("alice").call_answered,
        "the call cannot be answered before the messages have arrived\n{}",
        sim.trace().render()
    );

    sim.advance(Duration::from_secs(1)).expect("settles");
    assert!(sim.node::<StubUa>(ALICE).expect("alice").call_ended);
}

#[test]
fn an_unroutable_target_is_refused_rather_than_dropped() {
    let mut sim = Sim::new(0x5157_0005);
    let edge = sim.add_node(Box::new(StubEdge::new("edge")));
    sim.add_node(Box::new(StubUa::new("alice", Role::Caller, edge, "nobody")));
    sim.run_until_idle().expect("settles");

    // Alice registers, then calls someone who never did. The edge answers 404, and the trace
    // records the refusal — the failure mode worth distinguishing from silence.
    assert!(
        sim.trace().notes().contains(&"failed 404"),
        "{}",
        sim.trace().render()
    );
}
