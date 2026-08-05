//! `CF-26`: the harness can injure a *connection*, not only a link.
//!
//! [`owner-rpc` §10](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/owner-rpc.md)
//! names thirteen scenarios the connection-owner RPC must execute. Ten of them are weather this
//! harness already had — a partition, a kill, a broken stream link. Three are not: a client that
//! **reconnects**, an owner that **restarts into a fresh incarnation**, and a client that is
//! **accepted but never reads**. Each of those injures the connection rather than the wire, and
//! `CF-4`'s five faults cannot express any of them — `Partition` and `Heal` leave the connection
//! exactly as they found it, which is why a reconnect scenario written against them resolves to a
//! *live* flow and passes for the wrong reason.
//!
//! **What this file is, and what it is not.** It proves the three faults are expressible,
//! deterministic and observable, against a deliberately small stand-in for the owner's connection
//! table ([affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
//! §12.2 CT2–CT5 and §13.2 RS1–RS5). It is **not** `AF-7`'s implementation and does not claim to
//! be: `owner-rpc` §10's rows are named test functions in `AF-7`, run against the real owner, and
//! the names here are deliberately different so that a report cannot read this file as those rows
//! having landed. What changed is that they are now *writable*.
//!
//! The reference's canonical text form is `AF-6`'s 66-character base64url record. A harness
//! scenario needs the *identity* rather than the encoding, so the four fields ride the Route's
//! user part here in a readable form — `owner-rpc` §5 CR1's shape with `AF-6`'s bytes left to
//! `AF-6`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_sim::fault::{Fault, Schedule};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{HeaderName, Message, Method, RequestBuilder, Uri};

// The nodes, in the order every scenario adds them, so the constants below are the ids.
const OWNER: NodeId = NodeId::from_index(0);
const CLIENT: NodeId = NodeId::from_index(1);
const CALLER: NodeId = NodeId::from_index(2);

/// `owner-rpc` §8: pending delivery writes held for one connection before BQ2 refuses.
const MAX_PENDING_PER_FLOW: usize = 8;

/// `owner-rpc` §8: a write that has not left for the client's socket in this long is
/// `FlowRejected`.
const T_WRITE: Duration = Duration::from_secs(2);

/// The owner's `T_write` timer.
const T_WRITE_TIMER: TimerId = TimerId(1);

/// The caller's "issue the next delivery" timer.
const DELIVER_TIMER: TimerId = TimerId(2);

// ---------------------------------------------------------------------------------------------
// the identity half of a flow reference
// ---------------------------------------------------------------------------------------------

/// The four fields that make a connection nameable (`affinity-token` §11.2).
///
/// Not the wire record — `AF-6` owns that. This is what `resolve` asks its questions about, which
/// is all a harness scenario needs in order to know whether the fault did what it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlowRef {
    node: NodeId,
    incarnation: u32,
    connection: u32,
    generation: u32,
}

impl FlowRef {
    /// The user part of `owner-rpc` §5 CR1's Route. `-` is an RFC 3261 §25.1 `mark`, so this needs
    /// no escaping — the same property CR2 relies on for the real form.
    fn text(self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.node, self.incarnation, self.connection, self.generation
        )
    }

    fn from_route(value: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(value).ok()?;
        let user = text.split("sip:").nth(1)?.split('@').next()?;
        let mut parts = user.split('-');
        let node = parts.next()?.strip_prefix('n')?.parse::<usize>().ok()?;
        Some(Self {
            node: NodeId::from_index(node),
            incarnation: parts.next()?.parse().ok()?,
            connection: parts.next()?.parse().ok()?,
            generation: parts.next()?.parse().ok()?,
        })
    }
}

/// `affinity-token` §13.2, narrowed to the two outcomes these scenarios tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Live,
    FlowDead,
}

/// One row of the owner's connection table (`affinity-token` §12.1).
#[derive(Debug, Clone, Copy)]
struct Row {
    connection: u32,
    generation: u32,
    /// CT5: peer close and transport error both land here; a closed row resolves to nothing.
    closed: bool,
}

// ---------------------------------------------------------------------------------------------
// the nodes
// ---------------------------------------------------------------------------------------------

/// A stand-in for the node that owns the client's connection.
///
/// It keeps the parts of the table the three faults are about — the incarnation, one row per
/// connected peer, and the count of writes that have not left — and answers a delivery the way
/// `owner-rpc` §7 AN3 says: cause 1 for a dead flow, cause 2 for a refusal.
#[derive(Debug)]
struct Owner {
    name: String,
    /// CT2: an **input** to the table, never a clock the table reads.
    incarnation: u32,
    rows: BTreeMap<NodeId, Row>,
    /// The generation each slot last carried, so CT3's "previous generation + 1" survives a close.
    generations: BTreeMap<u32, u32>,
    next_slot: u32,
    /// Writes issued toward the client that have not left this node (`owner-rpc` §8 BQ2).
    pending: usize,
    client: NodeId,
}

impl Owner {
    fn new(client: NodeId) -> Self {
        Self {
            name: "owner".to_owned(),
            incarnation: 1,
            rows: BTreeMap::new(),
            generations: BTreeMap::new(),
            next_slot: 1,
            pending: 0,
            client,
        }
    }

    /// The reference this node would mint for a peer's current connection (FM2: every field comes
    /// from the row, never from a message).
    fn reference_for(&self, peer: NodeId) -> Option<FlowRef> {
        self.rows.get(&peer).map(|row| FlowRef {
            node: NodeId::from_index(0),
            incarnation: self.incarnation,
            connection: row.connection,
            generation: row.generation,
        })
    }

    /// CT3: take a free slot, bump that slot's generation, state `Open`.
    fn accept(&mut self, peer: NodeId) -> Effect {
        // CT6: a closed row's slot goes back on the free list immediately, and these scenarios
        // have one client, so it comes back to the slot it left. Safety is CT3's generation bump,
        // never a quarantine delay.
        let reused = self.rows.get(&peer).map(|row| row.connection);
        let connection = reused.unwrap_or_else(|| {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        });
        let generation = self.generations.get(&connection).copied().unwrap_or(0) + 1;
        self.generations.insert(connection, generation);
        self.rows.insert(
            peer,
            Row {
                connection,
                generation,
                closed: false,
            },
        );
        Effect::Note(format!("accepted {peer} as {connection}/{generation}"))
    }

    /// `affinity-token` §13.2, steps in order, first match wins.
    fn resolve(&self, id: FlowRef) -> Resolution {
        // RS1: a different node, or a previous run of this one.
        if id.node != NodeId::from_index(0) || id.incarnation != self.incarnation {
            return Resolution::FlowDead;
        }
        let Some(row) = self.rows.get(&self.client) else {
            return Resolution::FlowDead; // RS2: no row at this slot.
        };
        if row.connection != id.connection || row.generation != id.generation {
            return Resolution::FlowDead; // RS2/RS3: the slot was reused.
        }
        if row.closed {
            return Resolution::FlowDead; // RS4.
        }
        Resolution::Live // RS5.
    }
}

impl SimNode for Owner {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            // The node came back as a new process: whatever it held is gone, and CT2's incarnation
            // arrives with the restart rather than being read off a clock.
            Input::Restarted { incarnation } => {
                self.rows.clear();
                self.generations.clear();
                self.next_slot = 1;
                self.pending = 0;
                self.incarnation = incarnation;
                vec![Effect::Note(format!(
                    "restarted at incarnation {incarnation}"
                ))]
            }

            // CT3: a new connection, and therefore a new generation. Every reference to the one it
            // replaced died at this instant (RS3).
            Input::Connected { peer } => vec![self.accept(peer)],

            // CT5: transport error closes the row.
            Input::TransportError { peer } => {
                if let Some(row) = self.rows.get_mut(&peer) {
                    row.closed = true;
                }
                self.pending = 0;
                vec![
                    Effect::ClearTimer(T_WRITE_TIMER),
                    Effect::Note(format!("closed {peer}")),
                ]
            }

            // The write is still in this node's buffer: the client is not reading its socket.
            Input::WriteStalled { peer } => {
                if peer != self.client {
                    return Vec::new();
                }
                self.pending += 1;
                let mut effects = vec![Effect::Note(format!("write stalled ({})", self.pending))];
                if self.pending == 1 {
                    effects.push(Effect::SetTimer {
                        timer: T_WRITE_TIMER,
                        after: T_WRITE,
                    });
                }
                effects
            }

            Input::WriteFlushed { peer } => {
                if peer != self.client {
                    return Vec::new();
                }
                self.pending = self.pending.saturating_sub(1);
                let mut effects = vec![Effect::Note(format!("write flushed ({})", self.pending))];
                if self.pending == 0 {
                    effects.push(Effect::ClearTimer(T_WRITE_TIMER));
                }
                effects
            }

            // OW7: the write did not complete within `T_write`.
            Input::Timer(T_WRITE_TIMER) => {
                vec![Effect::Note("flow-rejected cause=2 t_write".to_owned())]
            }

            Input::Message { from, message } => {
                if from == self.client {
                    // The client's own traffic is what tells this node the connection exists.
                    return if self.rows.contains_key(&from) {
                        Vec::new()
                    } else {
                        vec![self.accept(from)]
                    };
                }
                self.deliver(message)
            }

            // CH7 for the first: a node never dials at start-up, and the client has not arrived
            // yet. Every other timer is the caller's.
            Input::Started | Input::Timer(_) => Vec::new(),
        }
    }
}

impl Owner {
    /// `owner-rpc` §6: a fixed sequence, and the first failing step wins.
    fn deliver(&mut self, message: &Message) -> Vec<Effect> {
        // OW2: the Route carries the reference.
        let Some(route) = message.headers().value(&HeaderName::Route) else {
            return vec![Effect::Note("403 no flow route".to_owned())];
        };
        let Some(id) = FlowRef::from_route(&route) else {
            return vec![Effect::Note("403 no flow route".to_owned())];
        };

        // OW4/OW5.
        if self.resolve(id) == Resolution::FlowDead {
            return vec![Effect::Note("flow-dead cause=1".to_owned())];
        }

        // OW6: within `max_pending_per_flow`, or BQ2 refuses immediately rather than blocking.
        if self.pending >= MAX_PENDING_PER_FLOW {
            return vec![Effect::Note("flow-rejected cause=2 bound".to_owned())];
        }

        // OW7: write, once.
        vec![
            send(self.client, message.clone()),
            Effect::Note("delivered".to_owned()),
        ]
    }
}

/// The client: it registers so the owner has a connection to name, and counts what arrives.
///
/// It registers again whenever its flow goes away — on a new connection and on a transport error
/// — which is what an outbound client does (RFC 5626 §4.3) and what makes "the owner has a row
/// for this client again" true after the harness has injured the connection.
#[derive(Debug)]
struct Client {
    name: String,
    owner: NodeId,
    received: Vec<String>,
}

impl SimNode for Client {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started | Input::Connected { .. } | Input::TransportError { .. } => {
                vec![send(self.owner, hello())]
            }
            Input::Message { message, .. } => {
                self.received.push(call_id(message));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// The caller edge: it issues `count` deliveries against one reference, one per tick.
#[derive(Debug)]
struct Caller {
    name: String,
    owner: NodeId,
    flow: FlowRef,
    first_at: Duration,
    every: Duration,
    remaining: usize,
    issued: usize,
}

impl SimNode for Caller {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => vec![Effect::SetTimer {
                timer: DELIVER_TIMER,
                after: self.first_at,
            }],
            Input::Timer(DELIVER_TIMER) => {
                if self.remaining == 0 {
                    return Vec::new();
                }
                self.remaining -= 1;
                self.issued += 1;
                let call = format!("d{}", self.issued);
                let mut effects = vec![send(self.owner, delivery(self.flow, &call))];
                if self.remaining > 0 {
                    effects.push(Effect::SetTimer {
                        timer: DELIVER_TIMER,
                        after: self.every,
                    });
                }
                effects
            }
            _ => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// messages
// ---------------------------------------------------------------------------------------------

fn uri(text: &str) -> Uri {
    Uri::parse(Bytes::copy_from_slice(text.as_bytes())).expect("a valid URI")
}

fn hello() -> Message {
    Message::Request(
        RequestBuilder::new(Method::Register, uri("sip:owner.example"))
            .header(HeaderName::CallId, "hello")
            .unwrap()
            .cseq(1, &Method::Register)
            .unwrap()
            .header(HeaderName::From, "<sip:client@example.test>;tag=c")
            .unwrap()
            .header(HeaderName::To, "<sip:client@example.test>")
            .unwrap()
            .header(HeaderName::Via, "SIP/2.0/TCP client.test;branch=z9hG4bK-h")
            .unwrap()
            .header(HeaderName::MaxForwards, "70")
            .unwrap()
            .build(),
    )
}

/// `owner-rpc` §5 CR1: the reference travels in the user part of a Route, and CR3 leaves the
/// Request-URI as the target.
fn delivery(flow: FlowRef, call: &str) -> Message {
    Message::Request(
        RequestBuilder::new(Method::Invite, uri("sip:client@example.test"))
            .header(
                HeaderName::Route,
                format!("<sip:{}@owner.example;lr>", flow.text()),
            )
            .unwrap()
            .header(HeaderName::CallId, call.to_owned())
            .unwrap()
            .cseq(1, &Method::Invite)
            .unwrap()
            .header(HeaderName::From, "<sip:caller@example.test>;tag=a")
            .unwrap()
            .header(HeaderName::To, "<sip:client@example.test>")
            .unwrap()
            .header(HeaderName::Via, "SIP/2.0/TCP caller.test;branch=z9hG4bK-d")
            .unwrap()
            .header(HeaderName::MaxForwards, "70")
            .unwrap()
            .build(),
    )
}

/// The pinned scenario's traffic: one method, one call id, so the vector's message lines are the
/// same width and a diff points at the schedule rather than at a header.
fn ping() -> Message {
    Message::Request(
        RequestBuilder::new(Method::Options, uri("sip:b@example.test"))
            .header(HeaderName::CallId, "ping")
            .unwrap()
            .cseq(1, &Method::Options)
            .unwrap()
            .header(HeaderName::From, "<sip:a@example.test>;tag=a")
            .unwrap()
            .header(HeaderName::To, "<sip:b@example.test>")
            .unwrap()
            .header(HeaderName::Via, "SIP/2.0/TCP a.test;branch=z9hG4bK-p")
            .unwrap()
            .header(HeaderName::MaxForwards, "70")
            .unwrap()
            .build(),
    )
}

fn call_id(message: &Message) -> String {
    message.headers().value(&HeaderName::CallId).map_or_else(
        || "-".to_owned(),
        |value| String::from_utf8_lossy(&value).trim().to_owned(),
    )
}

// ---------------------------------------------------------------------------------------------
// the topology every scenario runs on
// ---------------------------------------------------------------------------------------------

/// The reference the owner mints for the client's first connection, which every scenario asserts
/// before it injures anything: incarnation 1, slot 1, generation 1.
const FIRST: FlowRef = FlowRef {
    node: OWNER,
    incarnation: 1,
    connection: 1,
    generation: 1,
};

/// What the caller edge issues: one reference, `count` deliveries, on a fixed cadence.
#[derive(Debug, Clone, Copy)]
struct Deliveries {
    flow: FlowRef,
    first_at: Duration,
    every: Duration,
    count: usize,
}

/// Owner, client and caller on stream links — `owner-rpc` CH1 and AM3 permit nothing else between
/// nodes, and a client flow that could lose a message silently would make the answers untellable.
fn topology(seed: u64, policy: LinkPolicy, deliveries: Deliveries) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Stream, policy);
    // Asserted rather than commented: the constants above are positions, and a node inserted in
    // the wrong order would make every scenario in this file address the wrong peer.
    assert_eq!(sim.add_node(Box::new(Owner::new(CLIENT))), OWNER);
    assert_eq!(
        sim.add_node(Box::new(Client {
            name: "client".to_owned(),
            owner: OWNER,
            received: Vec::new(),
        })),
        CLIENT
    );
    assert_eq!(
        sim.add_node(Box::new(Caller {
            name: "caller".to_owned(),
            owner: OWNER,
            flow: deliveries.flow,
            first_at: deliveries.first_at,
            every: deliveries.every,
            remaining: deliveries.count,
            issued: 0,
        })),
        CALLER
    );
    sim
}

fn answers(sim: &Sim) -> Vec<&str> {
    sim.trace()
        .notes()
        .into_iter()
        .filter(|note| {
            note.starts_with("flow-dead")
                || note.starts_with("flow-rejected")
                || *note == "delivered"
        })
        .collect()
}

fn received(sim: &Sim) -> Vec<String> {
    sim.node::<Client>(CLIENT)
        .map(|client| client.received.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// owner-rpc §10, the three rows the harness could not reach
// ---------------------------------------------------------------------------------------------

/// `owner-rpc` §10 `owner_rpc_flow_dead_on_reconnect`, executed against the harness's own model.
///
/// The claim: the slot is reused with a bumped generation (`affinity-token` RS3), and the delivery
/// that names the connection the client *had* is answered cause 1 — a dead flow — rather than
/// written into the socket the client came back on.
///
/// The control run is the half that makes this non-vacuous: the same scenario without the fault
/// delivers, so a green assertion below is the reconnect and not the scenario never having worked.
#[test]
fn a_reconnect_leaves_the_reference_naming_a_connection_that_is_gone() {
    let deliveries = || Deliveries {
        flow: FIRST,
        first_at: Duration::from_secs(3),
        every: Duration::from_secs(1),
        count: 1,
    };

    let mut sim = topology(0xCF26_0001, LinkPolicy::CLEAN, deliveries());
    sim.schedule(&Schedule::new().at(
        SimTime::from_secs(2),
        Fault::Reconnect {
            a: CLIENT,
            b: OWNER,
        },
    ));

    sim.run_until(SimTime::from_secs(1)).expect("settles");
    assert_eq!(
        sim.node::<Owner>(OWNER)
            .and_then(|o| o.reference_for(CLIENT)),
        Some(FIRST),
        "the reference the delivery names is the one the owner minted"
    );

    sim.run_until(SimTime::from_secs(5)).expect("settles");
    assert_eq!(
        answers(&sim),
        ["flow-dead cause=1"],
        "the reconnect bumped the generation, so RS3 answers cause 1:\n{}",
        sim.trace().render()
    );
    assert!(
        received(&sim).is_empty(),
        "nothing was written into the connection that replaced it"
    );

    // Without the fault the very same delivery is written — otherwise the assertion above would
    // hold for a scenario that never delivered anything at all.
    let mut control = topology(0xCF26_0001, LinkPolicy::CLEAN, deliveries());
    control.run_until(SimTime::from_secs(5)).expect("settles");
    assert_eq!(answers(&control), ["delivered"]);
    assert_eq!(received(&control), ["d1"]);
}

/// `owner-rpc` §10 `owner_rpc_flow_dead_on_owner_restart`.
///
/// RS1, reached by a different fact than the row above: the reference was minted against the
/// owner's previous run. This is the row that would pass if `incarnation` were dropped from the
/// reference, which is why it is not the same test as the reconnect.
#[test]
fn a_restart_takes_a_fresh_incarnation_so_the_old_reference_is_dead() {
    let deliveries = || Deliveries {
        flow: FIRST,
        first_at: Duration::from_secs(3),
        every: Duration::from_secs(1),
        count: 1,
    };

    let mut sim = topology(0xCF26_0002, LinkPolicy::CLEAN, deliveries());
    sim.schedule(&Schedule::new().at(SimTime::from_secs(2), Fault::RestartNode(OWNER)));

    sim.run_until(SimTime::from_secs(1)).expect("settles");
    assert_eq!(sim.incarnation(OWNER), 1);
    assert_eq!(
        sim.node::<Owner>(OWNER)
            .and_then(|o| o.reference_for(CLIENT)),
        Some(FIRST)
    );

    sim.run_until(SimTime::from_secs(5)).expect("settles");
    assert_eq!(
        sim.incarnation(OWNER),
        2,
        "CT2: strictly greater than the run it replaced"
    );
    assert_eq!(
        answers(&sim),
        ["flow-dead cause=1"],
        "RS1: a reference from the previous incarnation:\n{}",
        sim.trace().render()
    );
    assert!(received(&sim).is_empty());

    let mut control = topology(0xCF26_0002, LinkPolicy::CLEAN, deliveries());
    control.run_until(SimTime::from_secs(5)).expect("settles");
    assert_eq!(answers(&control), ["delivered"]);
}

/// The distinction the restart row exists for, asserted rather than assumed.
///
/// A restart that re-issued slot 1 generation 1 would make the pre-restart reference resolve to
/// the connection accepted *after* the restart — `affinity-token` FR-9's stale-but-plausible hit.
/// The harness must therefore make the incarnation move, not just the table empty.
#[test]
fn a_restart_re_issues_the_same_slot_under_a_different_incarnation() {
    let mut sim = topology(
        0xCF26_0003,
        LinkPolicy::CLEAN,
        Deliveries {
            flow: FIRST,
            first_at: Duration::from_secs(30),
            every: Duration::from_secs(1),
            count: 0,
        },
    );
    sim.schedule(&Schedule::new().at(SimTime::from_secs(2), Fault::RestartNode(OWNER)));
    sim.run_until(SimTime::from_secs(5)).expect("settles");

    assert_eq!(
        sim.node::<Owner>(OWNER)
            .and_then(|o| o.reference_for(CLIENT)),
        Some(FlowRef {
            incarnation: 2,
            ..FIRST
        }),
        "the same slot and generation come back — only the incarnation tells them apart:\n{}",
        sim.trace().render()
    );
}

/// `owner-rpc` §10 `owner_rpc_rejected_when_flow_queue_full`.
///
/// The client is accepted and never drains. `max_pending_per_flow` fills, and the ninth delivery
/// is refused immediately — BQ2's "never a block" — rather than making the owner hold requests on
/// a stalled client's behalf. When the client reads again, the eight that were held arrive in the
/// order they were written.
#[test]
fn a_non_reading_client_overflows_the_per_flow_bound() {
    let mut sim = topology(
        0xCF26_0004,
        LinkPolicy::CLEAN,
        Deliveries {
            flow: FIRST,
            first_at: Duration::from_secs(1),
            every: Duration::from_millis(100),
            count: 10,
        },
    );
    sim.schedule(
        &Schedule::new()
            .at(SimTime::from_millis(500), Fault::StopReading(CLIENT))
            .at(SimTime::from_millis(2200), Fault::ResumeReading(CLIENT)),
    );

    sim.run_until(SimTime::from_millis(2100)).expect("settles");
    assert_eq!(
        answers(&sim),
        [
            "delivered",
            "delivered",
            "delivered",
            "delivered",
            "delivered",
            "delivered",
            "delivered",
            "delivered",
            "flow-rejected cause=2 bound",
            "flow-rejected cause=2 bound",
        ],
        "eight writes fit the bound; the ninth and tenth are refused:\n{}",
        sim.trace().render()
    );
    assert_eq!(
        sim.held_writes(CLIENT),
        MAX_PENDING_PER_FLOW,
        "the eight are held at the client, not on the wire"
    );
    assert!(
        received(&sim).is_empty(),
        "a client that is not reading receives nothing"
    );

    sim.run_until(SimTime::from_millis(2500)).expect("settles");
    assert_eq!(
        received(&sim),
        ["d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8"],
        "reading again drains what was held, in the order it was written"
    );
    assert_eq!(sim.held_writes(CLIENT), 0);
}

/// `owner-rpc` §10 `owner_rpc_rejected_when_write_stalls`.
///
/// Cause 2 again, reached by the timer rather than by the bound. The two paths must not be one
/// row: a bound that is never hit and a timer that never fires look identical in a report.
#[test]
fn a_non_reading_client_makes_t_write_expire() {
    let deliveries = || Deliveries {
        flow: FIRST,
        first_at: Duration::from_secs(1),
        every: Duration::from_secs(1),
        count: 1,
    };

    let mut sim = topology(0xCF26_0005, LinkPolicy::CLEAN, deliveries());
    sim.schedule(&Schedule::new().at(SimTime::from_millis(500), Fault::StopReading(CLIENT)));
    sim.run_until(SimTime::from_secs(5)).expect("settles");

    assert_eq!(
        answers(&sim),
        ["delivered", "flow-rejected cause=2 t_write"],
        "one write, held past T_write:\n{}",
        sim.trace().render()
    );
    assert!(received(&sim).is_empty());

    // The bound was never approached, so this row cannot be the previous one wearing a hat.
    assert_eq!(sim.held_writes(CLIENT), 1);

    let mut control = topology(0xCF26_0005, LinkPolicy::CLEAN, deliveries());
    control.run_until(SimTime::from_secs(5)).expect("settles");
    assert_eq!(
        answers(&control),
        ["delivered"],
        "a reading client flushes the write and T_write never fires"
    );
}

// ---------------------------------------------------------------------------------------------
// determinism
// ---------------------------------------------------------------------------------------------

/// The criterion with teeth: a new fault must not make a run less reproducible than it was
/// without one.
#[test]
fn the_new_faults_replay_byte_for_byte_from_their_seed() {
    let schedule = || {
        Schedule::new()
            .at(
                SimTime::from_secs(1),
                Fault::Reconnect {
                    a: CLIENT,
                    b: OWNER,
                },
            )
            .at(SimTime::from_millis(1500), Fault::StopReading(CLIENT))
            .at(SimTime::from_millis(2500), Fault::ResumeReading(CLIENT))
            .at(SimTime::from_secs(3), Fault::RestartNode(OWNER))
    };
    let run = |seed: u64| {
        let mut sim = topology(
            seed,
            // Jitter, so the run has something to draw and a seed something to change.
            LinkPolicy::jittery(1, 40),
            Deliveries {
                flow: FIRST,
                first_at: Duration::from_millis(1800),
                every: Duration::from_millis(200),
                count: 4,
            },
        );
        sim.schedule(&schedule());
        sim.run_until(SimTime::from_secs(6)).expect("settles");
        sim.trace().render()
    };

    assert_eq!(
        run(0xCF26_5EED),
        run(0xCF26_5EED),
        "same seed, same schedule, same trace"
    );
    assert_ne!(
        run(0xCF26_5EED),
        run(0xCF26_0FF5),
        "a different seed must explore something else, or the schedule never reached the generator"
    );
}

/// The pinned vector: one run, one seed, one rendered trace, written out.
///
/// A replay test proves two runs agree with each other; only a literal proves they still agree
/// with what was reviewed. Every one of the four new fault variants appears here, so a change that
/// perturbs any of them fails on the first line that differs.
#[test]
fn a_pinned_schedule_renders_the_trace_it_was_pinned_at() {
    let mut sim = Sim::new(0xCF26_D1CE);
    sim.link_default(LinkKind::Stream, LinkPolicy::CLEAN);
    sim.add_node(Box::new(Watcher {
        name: "a".to_owned(),
        peer: Some(NodeId::from_index(1)),
    }));
    sim.add_node(Box::new(Watcher {
        name: "b".to_owned(),
        peer: None,
    }));
    sim.schedule(
        &Schedule::new()
            .at(
                SimTime::from_millis(500),
                Fault::RestartNode(NodeId::from_index(1)),
            )
            .at(
                SimTime::from_millis(1500),
                Fault::Reconnect {
                    a: NodeId::from_index(0),
                    b: NodeId::from_index(1),
                },
            )
            .at(
                SimTime::from_millis(2500),
                Fault::StopReading(NodeId::from_index(1)),
            )
            .at(
                SimTime::from_millis(3500),
                Fault::ResumeReading(NodeId::from_index(1)),
            ),
    );
    sim.run_until(SimTime::from_millis(3900)).expect("settles");

    assert_eq!(sim.trace().render(), PINNED, "the pinned trace moved");
}

/// A node that says what the harness told it, once a second, so a schedule has something to bite.
#[derive(Debug)]
struct Watcher {
    name: String,
    peer: Option<NodeId>,
}

impl SimNode for Watcher {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let tick = |extra: Vec<Effect>| {
            let mut effects = extra;
            effects.push(Effect::SetTimer {
                timer: DELIVER_TIMER,
                after: Duration::from_secs(1),
            });
            effects
        };
        match input {
            Input::Started => {
                if self.peer.is_some() {
                    tick(Vec::new())
                } else {
                    Vec::new()
                }
            }
            Input::Restarted { incarnation } => {
                vec![Effect::Note(format!("restarted {incarnation}"))]
            }
            Input::Timer(_) => match self.peer {
                Some(peer) => tick(vec![send(peer, ping())]),
                None => Vec::new(),
            },
            Input::Connected { peer } => vec![Effect::Note(format!("connected {peer}"))],
            Input::TransportError { peer } => vec![Effect::Note(format!("lost {peer}"))],
            Input::WriteStalled { peer } => vec![Effect::Note(format!("stalled {peer}"))],
            Input::WriteFlushed { peer } => vec![Effect::Note(format!("flushed {peer}"))],
            Input::Message { .. } => Vec::new(),
        }
    }
}

/// The trace above, as reviewed.
///
/// Two things in it are the behaviour rather than the accident, and are worth reading twice. At
/// `0.500000` the restart tells **nobody** its connection broke: `a` has not written yet, so there
/// is no link between them and therefore no connection to lose — the harness creates a link when
/// one is first used, which is as close as it gets to a connection existing. At `3.000000` the
/// send is followed by `note stalled n1` and by no arrival at all, and the message reaches `b`
/// only at `3.500000`, after the resume and after `a` has been told the write went out.
const PINNED: &str = concat!(
    "      0.000000    0 a          started\n",
    "      0.000000    1 a          timer t2 set for       1.000000\n",
    "      0.000000    2 b          started\n",
    // RestartNode: a new incarnation, and the node lights up again.
    "      0.500000    3 b          ** FAULT restart n1\n",
    "      0.500000    4 b          started\n",
    "      0.500000    5 b          note restarted 2\n",
    "      1.000000    6 a          timer t2 fired\n",
    "      1.000000    7 a          -> n1 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
    "      1.000000    8 a          timer t2 set for       2.000000\n",
    "      1.000000    9 b          <- n0 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
    // Reconnect: each end loses the connection it had, then gets the one that replaced it.
    "      1.500000   10 a          ** FAULT reconnect n0|n1\n",
    "      1.500000   11 a          !! n1 link broken\n",
    "      1.500000   12 a          note lost n1\n",
    "      1.500000   13 b          !! n0 link broken\n",
    "      1.500000   14 b          note lost n0\n",
    "      1.500000   15 a          note connected n1\n",
    "      1.500000   16 b          note connected n0\n",
    "      2.000000   17 a          timer t2 fired\n",
    "      2.000000   18 a          -> n1 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
    "      2.000000   19 a          timer t2 set for       3.000000\n",
    "      2.000000   20 b          <- n0 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
    // StopReading: the write leaves `a` and arrives nowhere.
    "      2.500000   21 b          ** FAULT stop-reading n1\n",
    "      3.000000   22 a          timer t2 fired\n",
    "      3.000000   23 a          -> n1 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
    "      3.000000   24 a          timer t2 set for       4.000000\n",
    "      3.000000   25 a          note stalled n1\n",
    // ResumeReading: it goes out, and only then arrives.
    "      3.500000   26 b          ** FAULT resume-reading n1\n",
    "      3.500000   27 a          note flushed n1\n",
    "      3.500000   28 b          <- n0 OPTIONS sip:b@example.test [ping #1 OPTIONS]\n",
);
