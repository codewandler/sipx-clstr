//! Two edges, one address-of-record, one store: the serialization claim, under the harness.
//!
//! `RG-3`'s second acceptance criterion. A synchronous `read`-then-`commit` in one function can
//! never actually race — the discrete-event scheduler runs one node input to completion before the
//! next — so proving anything here requires modelling what a real driver does: `read` and `commit`
//! are round trips to a store, and the two edges interleave *between* them.
//!
//! So the registrar node below is two-phase: a REGISTER triggers a read and arms a timer for the
//! store's latency; the timer's expiry performs the commit. Two edges handed the same
//! address-of-record at the same virtual instant therefore both read revision *n*, and exactly one
//! of them wins. That is the race the CAS contract exists for, and this is where it happens.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_registrar::{
    BindingSet, CanonicalAor, EdgeContext, InMemoryStore, LocationStore, Outcome, RegisterCommand,
    Revision, TenantPolicy, Timestamp, process, register_command,
};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{
    HeaderName, Message, Method, Request, RequestBuilder, ResponseBuilder, StatusCode, Uri,
};

const TENANT: &str = "t1";
/// This harness runs on the in-memory backend, whose reads cannot fail (§6 K7's failure is a
/// property of a backend that talks to something). `tests/read_faults.rs` owns that path.
const READS: &str = "the in-memory backend always reads";
/// How long a store round trip takes. Any non-zero value is enough to interleave two edges.
const STORE_LATENCY: Duration = Duration::from_millis(5);
const COMMIT_TIMER: TimerId = TimerId(1);

fn aor() -> CanonicalAor {
    CanonicalAor::parse(Bytes::from_static(b"sip:alice@atlanta.example")).expect("a valid AoR")
}

/// A registrar edge whose store access costs virtual time.
#[derive(Debug)]
struct Edge {
    name: String,
    store: Arc<InMemoryStore>,
    policy: TenantPolicy,
    /// The command and the revision it read, waiting for the store round trip to finish.
    pending: Option<Pending>,
    /// How many CAS conflicts this edge absorbed.
    conflicts: usize,
}

#[derive(Debug)]
struct Pending {
    reply_to: NodeId,
    request: Request,
    cmd: RegisterCommand,
    read_at: Revision,
    set: BindingSet,
    attempts: usize,
}

impl Edge {
    fn new(name: &str, store: Arc<InMemoryStore>) -> Self {
        Self {
            name: name.to_owned(),
            store,
            policy: TenantPolicy::default(),
            pending: None,
            conflicts: 0,
        }
    }

    /// Phase one: read, decide, and arm the round trip that will commit.
    fn begin(&mut self, from: NodeId, request: &Request, now: SimTime) -> Vec<Effect> {
        let edge = EdgeContext {
            tenant: TENANT.to_owned(),
            ..EdgeContext::default()
        };
        let clock = Timestamp::from_nanos(now.as_nanos());

        let cmd = match register_command(request, &edge, clock) {
            Ok(cmd) => cmd,
            Err(rejection) => {
                return vec![
                    Effect::Note(format!("reject {}", rejection.status())),
                    reply(from, request, rejection.status()),
                ];
            }
        };

        let (set, read_at) = self.store.read(TENANT, &cmd.aor).expect(READS);
        match process(&cmd, &set, &self.policy) {
            Outcome::Commit { set, .. } => {
                self.pending = Some(Pending {
                    reply_to: from,
                    request: request.clone(),
                    cmd,
                    read_at,
                    set,
                    attempts: 0,
                });
                vec![
                    Effect::Note(format!("read {read_at}")),
                    Effect::SetTimer {
                        timer: COMMIT_TIMER,
                        after: STORE_LATENCY,
                    },
                ]
            }
            // Nothing to write, so nothing to race: answer immediately.
            outcome => vec![
                Effect::Note(format!("no write, {}", outcome.status())),
                reply(from, request, outcome.status()),
            ],
        }
    }

    /// Phase two: the round trip landed. Commit, or discover we lost and go round again.
    fn finish(&mut self) -> Vec<Effect> {
        let Some(mut pending) = self.pending.take() else {
            return Vec::new();
        };

        match self.store.commit(
            TENANT,
            &pending.cmd.aor,
            pending.read_at,
            pending.set.clone(),
        ) {
            Ok(revision) => vec![
                Effect::Note(format!("committed {revision}")),
                reply(pending.reply_to, &pending.request, 200),
            ],
            Err(conflict) => {
                self.conflicts += 1;
                pending.attempts += 1;
                if pending.attempts > 3 {
                    // S10.
                    return vec![
                        Effect::Note("exhausted".to_owned()),
                        reply(pending.reply_to, &pending.request, 503),
                    ];
                }

                // Re-read and re-decide against what is actually there now. The command is
                // unchanged — including its `now` — which is what makes re-presenting it safe.
                let (set, read_at) = self.store.read(TENANT, &pending.cmd.aor).expect(READS);
                match process(&pending.cmd, &set, &self.policy) {
                    Outcome::Commit { set, .. } => {
                        self.pending = Some(Pending {
                            read_at,
                            set,
                            ..pending
                        });
                        vec![
                            Effect::Note(format!(
                                "conflict at {}, re-read {read_at}",
                                conflict.current
                            )),
                            Effect::SetTimer {
                                timer: COMMIT_TIMER,
                                after: STORE_LATENCY,
                            },
                        ]
                    }
                    // The re-decision says there is nothing left to do: someone else already
                    // wrote what this command wanted. That is LS-K-2, and it is a success.
                    outcome => vec![
                        Effect::Note(format!("conflict resolved as noop, {}", outcome.status())),
                        reply(pending.reply_to, &pending.request, outcome.status()),
                    ],
                }
            }
        }
    }
}

impl SimNode for Edge {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Message {
                from,
                message: Message::Request(request),
            } => self.begin(from, request, now),
            Input::Timer(COMMIT_TIMER) => self.finish(),
            _ => Vec::new(),
        }
    }
}

fn reply(to: NodeId, request: &Request, status: u16) -> Effect {
    let response = ResponseBuilder::to_request(
        request,
        StatusCode::new(status).expect("a valid status"),
        reason_of(status),
    )
    .expect("a response to a well-formed request")
    .build();
    send(to, Message::Response(response))
}

fn reason_of(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

/// An endpoint that registers once at start and records the status it got.
#[derive(Debug)]
struct Ua {
    name: String,
    edge: NodeId,
    contact: &'static str,
    answered: Option<u16>,
}

impl SimNode for Ua {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => vec![send(self.edge, self.register())],
            Input::Message {
                message: Message::Response(response),
                ..
            } => {
                self.answered = Some(response.status.code());
                vec![Effect::Note(format!("answered {}", response.status.code()))]
            }
            _ => Vec::new(),
        }
    }
}

impl Ua {
    fn register(&self) -> Message {
        let target = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).expect("a URI");
        Message::Request(
            RequestBuilder::new(Method::Register, target)
                .header(HeaderName::CallId, format!("call-{}", self.name))
                .expect("Call-ID")
                .cseq(1, &Method::Register)
                .expect("CSeq")
                .header(
                    HeaderName::From,
                    format!("<sip:{}@atlanta.example>", self.name),
                )
                .expect("From")
                .header(HeaderName::To, "<sip:alice@atlanta.example>")
                .expect("To")
                .header(
                    HeaderName::Contact,
                    format!("<{}>;expires=3600", self.contact),
                )
                .expect("Contact")
                .header(
                    HeaderName::Via,
                    format!("SIP/2.0/UDP {}.sim;branch=z9hG4bK-{}", self.name, self.name),
                )
                .expect("Via")
                .max_forwards(70)
                .build(),
        )
    }
}

fn scenario(seed: u64) -> (Sim, Arc<InMemoryStore>) {
    let store = Arc::new(InMemoryStore::new());
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);

    let edge_a = sim.add_node(Box::new(Edge::new("edge-a", Arc::clone(&store))));
    let edge_b = sim.add_node(Box::new(Edge::new("edge-b", Arc::clone(&store))));
    sim.add_node(Box::new(Ua {
        name: "phone-1".to_owned(),
        edge: edge_a,
        contact: "sip:alice@10.0.0.1",
        answered: None,
    }));
    sim.add_node(Box::new(Ua {
        name: "phone-2".to_owned(),
        edge: edge_b,
        contact: "sip:alice@10.0.0.2",
        answered: None,
    }));
    (sim, store)
}

const EDGE_A: NodeId = NodeId::from_index(0);
const EDGE_B: NodeId = NodeId::from_index(1);
const PHONE_1: NodeId = NodeId::from_index(2);
const PHONE_2: NodeId = NodeId::from_index(3);

#[test]
fn two_edges_registering_one_aor_serialize_and_neither_binding_is_lost() {
    let (mut sim, store) = scenario(0x5245_4701);
    sim.run_until_idle().expect("the scenario should settle");

    // Both phones got a 200 — the loser retried rather than failing.
    assert_eq!(
        sim.node::<Ua>(PHONE_1).and_then(|ua| ua.answered),
        Some(200),
        "{}",
        sim.trace().render()
    );
    assert_eq!(
        sim.node::<Ua>(PHONE_2).and_then(|ua| ua.answered),
        Some(200)
    );

    // And the race genuinely happened: exactly one edge lost a commit.
    let conflicts = sim.node::<Edge>(EDGE_A).map_or(0, |e| e.conflicts)
        + sim.node::<Edge>(EDGE_B).map_or(0, |e| e.conflicts);
    assert_eq!(
        conflicts,
        1,
        "one edge must have lost the CAS\n{}",
        sim.trace().render()
    );

    // Two bindings, two revisions: no lost update.
    let (set, revision) = store.read(TENANT, &aor()).expect(READS);
    assert_eq!(revision, Revision(2));
    let contacts: Vec<&[u8]> = set.all().iter().map(|b| b.contact.as_ref()).collect();
    assert_eq!(
        contacts,
        [&b"sip:alice@10.0.0.1"[..], &b"sip:alice@10.0.0.2"[..]],
        "both contacts must survive the race"
    );
}

#[test]
fn the_race_and_its_resolution_replay_byte_for_byte() {
    let (mut first, _) = scenario(0x5245_4702);
    let (mut second, _) = scenario(0x5245_4702);
    first.run_until_idle().expect("settles");
    second.run_until_idle().expect("settles");
    assert_eq!(first.trace().render(), second.trace().render());
}

#[test]
fn the_trace_shows_the_conflict_and_the_re_read_that_followed() {
    let (mut sim, _) = scenario(0x5245_4703);
    sim.run_until_idle().expect("settles");
    let notes = sim.trace().notes();
    assert!(
        notes.iter().any(|note| note.starts_with("conflict at")),
        "the losing edge should have recorded its conflict: {notes:?}"
    );
    assert_eq!(
        notes
            .iter()
            .filter(|note| note.starts_with("committed"))
            .count(),
        2,
        "two commits, one per registration: {notes:?}"
    );
}

#[test]
fn an_edge_answers_before_the_store_round_trip_completes_only_when_there_is_nothing_to_write() {
    // A registration that writes must not be acknowledged until the write landed — otherwise a UA
    // is told it is reachable before it is. This asserts the ordering: the 200 comes after the
    // commit, never before.
    let (mut sim, _) = scenario(0x5245_4704);
    sim.run_until_idle().expect("settles");

    let entries = sim.trace().entries();
    let first_commit = entries
        .iter()
        .position(|entry| {
            matches!(&entry.event, sipx_clstr_sim::trace::Event::Note(note) if note.starts_with("committed"))
        })
        .expect("a commit");
    let first_ok = entries
        .iter()
        .position(|entry| {
            matches!(&entry.event, sipx_clstr_sim::trace::Event::Note(note) if note.starts_with("answered"))
        })
        .expect("an answer");
    assert!(
        first_commit < first_ok,
        "the 200 must follow the commit\n{}",
        sim.trace().render()
    );
}

#[test]
fn a_malformed_register_is_refused_without_touching_the_store() {
    /// Sends a REGISTER whose `To` cannot be an address-of-record (a password — §3.2 N10).
    #[derive(Debug)]
    struct BadUa {
        edge: NodeId,
        answered: Option<u16>,
    }

    impl SimNode for BadUa {
        fn name(&self) -> &'static str {
            "bad"
        }

        fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
            match input {
                Input::Started => {
                    let target = Uri::parse(Bytes::from_static(b"sip:atlanta.example")).unwrap();
                    let request = RequestBuilder::new(Method::Register, target)
                        .header(HeaderName::CallId, "bad")
                        .unwrap()
                        .cseq(1, &Method::Register)
                        .unwrap()
                        .header(HeaderName::From, "<sip:x@atlanta.example>")
                        .unwrap()
                        .header(HeaderName::To, "<sip:alice:secret@atlanta.example>")
                        .unwrap()
                        .header(HeaderName::Contact, "<sip:alice@10.0.0.9>")
                        .unwrap()
                        .header(HeaderName::Via, "SIP/2.0/UDP bad.sim;branch=z9hG4bK-bad")
                        .unwrap()
                        .build();
                    vec![send(self.edge, Message::Request(request))]
                }
                Input::Message {
                    message: Message::Response(response),
                    ..
                } => {
                    self.answered = Some(response.status.code());
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
    }

    let store = Arc::new(InMemoryStore::new());
    let mut sim = Sim::new(0x5245_4705);
    let edge = sim.add_node(Box::new(Edge::new("edge", Arc::clone(&store))));

    let ua = sim.add_node(Box::new(BadUa {
        edge,
        answered: None,
    }));
    sim.run_until_idle().expect("settles");

    assert_eq!(sim.node::<BadUa>(ua).and_then(|u| u.answered), Some(400));
    assert_eq!(store.rows(), 0, "a refusal must not create a row");
}
