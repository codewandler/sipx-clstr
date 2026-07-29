//! The scheduler: the loop that makes virtual time pass.
//!
//! Everything else in this crate is machinery this loop drives. Advancing time is popping the
//! event queue; performing an effect is scheduling more events; the trace is written as it goes.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bytes::Bytes;
use sipx_sip::{Limits, Message, parse_datagram};

use crate::fault::{Fault, Schedule};
use crate::net::{Delivery, Link, LinkKind, LinkPolicy, NodeId};
use crate::node::{Effect, Input, SimNode, TimerId};
use crate::queue::EventQueue;
use crate::time::SimTime;
use crate::trace::{self, Trace};
use sipx_transport::timers::TimerQueue;

/// Why a run stopped early.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimError {
    /// The simulation performed more steps than the budget allows.
    ///
    /// Almost always a livelock — two nodes answering each other forever, or a timer that rearms
    /// itself. Reported rather than run to exhaustion, because a test that hangs tells you less
    /// than one that fails.
    #[error("step budget of {budget} exhausted at {at}; the scenario is probably livelocked")]
    StepBudgetExhausted {
        /// The budget that was hit.
        budget: u64,
        /// The virtual time it was hit at.
        at: SimTime,
    },
}

/// A running simulation.
#[derive(Debug)]
pub struct Sim {
    seed: u64,
    now: SimTime,
    nodes: Vec<Box<dyn SimNode>>,
    /// Ordered so that iteration — and therefore the trace — never depends on hash order.
    links: BTreeMap<(NodeId, NodeId), Link>,
    default_kind: LinkKind,
    default_policy: LinkPolicy,
    queue: EventQueue<Event>,
    /// Pending timers, in the kernel's queue (`CF-7`). Its cancellation discipline — a generation
    /// per key, stale entries discarded at pop — is the one this crate used to reimplement.
    timers: TimerQueue<(NodeId, TimerId), SimTime>,
    trace: Trace,
    budget: u64,
    started: bool,
    /// Nodes a `KillNode` fault has stopped. A killed node performs no effects and receives no
    /// inputs; its links are cut as well, so this is belt and braces — but the belt is what makes
    /// "stopped" different from "unreachable", and the two failures behave differently.
    killed: BTreeSet<NodeId>,
    /// Per-node timer rate in per-mille, for nodes a `TimerSkew` fault has touched. Absent means
    /// nominal; the map stays empty in every scenario that never skews a clock.
    skew: BTreeMap<NodeId, u32>,
}

#[derive(Debug)]
enum Event {
    Deliver {
        from: NodeId,
        to: NodeId,
        bytes: Bytes,
    },
    Break {
        node: NodeId,
        peer: NodeId,
    },
    /// A scheduled fault comes due. In the queue rather than applied from outside it, so its
    /// ordering against deliveries and timers is the same insertion-sequence rule as everything
    /// else and a schedule replays identically from its seed.
    Fault(Box<Fault>),
}

impl Sim {
    /// A simulation with no nodes, at time zero.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            now: SimTime::START,
            nodes: Vec::new(),
            links: BTreeMap::new(),
            default_kind: LinkKind::Datagram,
            default_policy: LinkPolicy::CLEAN,
            queue: EventQueue::new(),
            timers: TimerQueue::new(),
            killed: BTreeSet::new(),
            skew: BTreeMap::new(),
            trace: Trace::new(),
            // Generous, but finite. A correct scenario settles in tens of steps; this exists to
            // turn a livelock into a failing test rather than a hung CI job.
            budget: 100_000,
            started: false,
        }
    }

    /// The master seed this run was started from. Report it when an assertion fails: it is the
    /// whole of what a reader needs to reproduce the run.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The current virtual time.
    #[must_use]
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// Set the kind and policy every link gets unless the scenario says otherwise.
    pub fn link_default(&mut self, kind: LinkKind, policy: LinkPolicy) -> &mut Self {
        self.default_kind = kind;
        self.default_policy = policy;
        self
    }

    /// Cap how many events a run may perform before it is declared livelocked.
    pub fn step_budget(&mut self, budget: u64) -> &mut Self {
        self.budget = budget;
        self
    }

    /// Add a node. Nodes keep the order they were added in, which is the order inputs are
    /// delivered in when several are due at once.
    pub fn add_node(&mut self, node: Box<dyn SimNode>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// Give one direction of one link an explicit kind and policy.
    ///
    /// Directional on purpose: a partition is rarely symmetric in the failures worth simulating,
    /// and a one-way link is the cheapest way to reproduce the half-open connection that makes
    /// distributed systems interesting. Use [`Sim::link`] for the symmetric case.
    pub fn link_one_way(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: LinkKind,
        policy: LinkPolicy,
    ) -> &mut Self {
        let label = link_label(self.name_of(from), self.name_of(to));
        self.links
            .insert((from, to), Link::new(kind, policy, self.seed, &label));
        self
    }

    /// Give both directions of a link the same kind and policy.
    pub fn link(&mut self, a: NodeId, b: NodeId, kind: LinkKind, policy: LinkPolicy) -> &mut Self {
        self.link_one_way(a, b, kind, policy);
        self.link_one_way(b, a, kind, policy);
        self
    }

    /// Cut or restore one direction of a link.
    ///
    /// The whole of what a fault schedule needs from the network (`CF-4` adds the scheduling).
    /// A partition and a node kill are both this, applied to a set of links.
    pub fn set_partitioned(&mut self, from: NodeId, to: NodeId, partitioned: bool) -> &mut Self {
        let label = link_label(self.name_of(from), self.name_of(to));
        let (kind, policy) = self
            .links
            .get(&(from, to))
            .map_or((self.default_kind, self.default_policy), |link| {
                (link.kind, link.policy)
            });
        let policy = LinkPolicy {
            partitioned,
            ..policy
        };
        self.links
            .entry((from, to))
            .or_insert_with(|| Link::new(kind, policy, self.seed, &label))
            .policy = policy;
        self
    }

    /// Queue a fault schedule (`CF-4`).
    ///
    /// Entries become queue events at their instants, so they interleave with deliveries and
    /// timers under the one ordering rule. Calling this more than once is the same as merging the
    /// schedules: composition is concatenation, and nothing here cares which call an entry came
    /// from.
    pub fn schedule(&mut self, schedule: &Schedule) -> &mut Self {
        for (at, fault) in schedule.entries() {
            self.queue
                .schedule_at(*at, Event::Fault(Box::new(fault.clone())));
        }
        self
    }

    /// Whether a `KillNode` fault has stopped this node.
    #[must_use]
    pub fn is_killed(&self, node: NodeId) -> bool {
        self.killed.contains(&node)
    }

    /// Apply one fault, now.
    fn apply(&mut self, fault: &Fault) {
        // Recorded before it takes effect, so the trace reads as cause then consequence.
        let anchor = match fault {
            Fault::KillNode(node) | Fault::TimerSkew { node, .. } => *node,
            Fault::Partition { a, b } | Fault::Heal { a, b } => {
                *a.first().or_else(|| b.first()).unwrap_or(&NodeId(0))
            }
            Fault::SetLinkPolicy { from, .. } => *from,
        };
        let name = self.name_of(anchor).to_owned();
        self.trace.record(
            self.now,
            anchor,
            &name,
            trace::Event::Fault(fault.summary()),
        );

        match fault {
            Fault::KillNode(node) => {
                self.killed.insert(*node);
                // Every link, both ways. A node that is down is unreachable from either side, and
                // cutting only outbound would leave it silently absorbing traffic.
                let peers: Vec<NodeId> = (0..self.nodes.len()).map(NodeId).collect();
                for peer in peers {
                    if peer == *node {
                        continue;
                    }
                    self.set_partitioned(*node, peer, true);
                    self.set_partitioned(peer, *node, true);
                }
                // The timers it was waiting for never fire. This is what separates a kill from an
                // isolation: an isolated node keeps running and keeps timing out. `forget_matching`
                // drops the generations outright, so the entries still in the queue match nothing
                // and are discarded when they surface.
                self.timers.forget_matching(|(owner, _)| owner == node);
            }
            Fault::Partition { a, b } => self.cut_between(a, b, true),
            Fault::Heal { a, b } => self.cut_between(a, b, false),
            Fault::SetLinkPolicy { from, to, policy } => {
                let label = link_label(self.name_of(*from), self.name_of(*to));
                let kind = self
                    .links
                    .get(&(*from, *to))
                    .map_or(self.default_kind, |link| link.kind);
                self.links
                    .entry((*from, *to))
                    .or_insert_with(|| Link::new(kind, *policy, self.seed, &label))
                    .policy = *policy;
            }
            Fault::TimerSkew { node, per_mille } => {
                self.skew.insert(*node, *per_mille);
            }
        }
    }

    /// Cut or reconnect every link crossing between two groups, both directions.
    fn cut_between(&mut self, a: &[NodeId], b: &[NodeId], partitioned: bool) {
        for from in a {
            for to in b {
                self.set_partitioned(*from, *to, partitioned);
                self.set_partitioned(*to, *from, partitioned);
            }
        }
    }

    /// A duration as this node's clock measures it.
    ///
    /// Skew is applied to what a node *waits for*, not to what it thinks the time is: RFC 3261's
    /// timers are durations, and the failure worth simulating is two nodes disagreeing about how
    /// long a second is.
    fn skewed(&self, node: NodeId, after: Duration) -> Duration {
        match self.skew.get(&node) {
            None | Some(1000) => after,
            Some(per_mille) => {
                let nanos = after.as_nanos() * u128::from(*per_mille) / 1000;
                Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
            }
        }
    }

    /// Everything that has happened.
    #[must_use]
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// Look inside a node — for the assertions that read better as state than as trace queries.
    #[must_use]
    pub fn node<T: SimNode>(&self, id: NodeId) -> Option<&T> {
        let node: &dyn std::any::Any = self.nodes.get(id.index())?.as_ref();
        node.downcast_ref::<T>()
    }

    /// Tell every node the scenario has begun, in the order they were added.
    ///
    /// Idempotent: calling it twice does nothing the second time, so a scenario that runs in
    /// stages cannot accidentally restart its endpoints.
    pub fn start(&mut self) -> Result<(), SimError> {
        if self.started {
            return Ok(());
        }
        self.started = true;
        for index in 0..self.nodes.len() {
            let id = NodeId(index);
            let name = self.name_of(id).to_owned();
            self.trace
                .record(self.now, id, &name, trace::Event::Started);
            let effects = self.deliver(id, Input::Started);
            self.perform(id, effects);
        }
        Ok(())
    }

    /// Hand a message to a node as though it had arrived from `from`, without a link in between.
    ///
    /// The way a scenario replays a spec vector: message in, effects asserted out, no weather.
    pub fn inject(&mut self, from: NodeId, to: NodeId, message: &Message) {
        let name = self.name_of(to).to_owned();
        self.trace.record(
            self.now,
            to,
            &name,
            trace::Event::Received {
                from,
                summary: trace::summarize(message),
            },
        );
        let effects = self.deliver(to, Input::Message { from, message });
        self.perform(to, effects);
    }

    /// Run until nothing is left to do.
    pub fn run_until_idle(&mut self) -> Result<(), SimError> {
        self.run_until(SimTime::from_nanos(u64::MAX))
    }

    /// Run until virtual time reaches `deadline`, or the queue empties, whichever comes first.
    ///
    /// Time does not advance past the deadline: an event scheduled beyond it stays queued, so a
    /// scenario can look at the world mid-flight and then carry on.
    pub fn run_until(&mut self, deadline: SimTime) -> Result<(), SimError> {
        self.start()?;
        let mut steps = 0_u64;
        loop {
            // Two queues since `CF-7`: the kernel's holds pending timers, this crate's holds the
            // things it cannot — deliveries, breaks and faults all carry payloads. Whichever is due
            // first wins; at a tie the precedence below reproduces exactly what the single queue's
            // insertion-sequence tie-break used to produce, so no seed changed meaning.
            let next = match (self.timers.next_deadline(), self.queue.next_deadline()) {
                (None, None) => break,
                (Some(timer_at), None) => timer_at,
                (None, Some(event_at)) => event_at,
                (Some(timer_at), Some(event_at)) => timer_at.min(event_at),
            };
            if next > deadline {
                break;
            }
            steps += 1;
            if steps > self.budget {
                return Err(SimError::StepBudgetExhausted {
                    budget: self.budget,
                    at: next,
                });
            }
            self.now = next;

            // A fault due at this instant goes first. Under the single queue it was scheduled
            // during setup and so carried the lowest insertion sequence of anything at its
            // deadline; keeping that is what makes "the weather changes the world, then the world
            // runs" true — a kill at T stops the timer at T, rather than racing it.
            let fault_first = matches!(self.queue.peek(), Some(Event::Fault(_)))
                && self.queue.next_deadline() == Some(next);

            let fired = if fault_first {
                Vec::new()
            } else {
                self.timers.take_due(next)
            };

            if fired.is_empty() {
                let Some((at, event)) = self.queue.pop() else {
                    break;
                };
                self.now = at;
                self.handle(event);
            } else {
                for (node, timer) in fired {
                    let name = self.name_of(node).to_owned();
                    self.trace
                        .record(self.now, node, &name, trace::Event::TimerFired(timer));
                    let effects = self.deliver(node, Input::Timer(timer));
                    self.perform(node, effects);
                }
            }
        }
        if self.now < deadline && deadline.as_nanos() != u64::MAX {
            // Nothing is pending, so the clock may jump straight to where the scenario asked to
            // be. An idle cluster fast-forwards through silence for free.
            self.now = deadline;
        }
        Ok(())
    }

    /// Run forward by a duration.
    pub fn advance(&mut self, by: Duration) -> Result<(), SimError> {
        let deadline = self.now.saturating_add(by);
        self.run_until(deadline)
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Deliver { from, to, bytes } => self.handle_delivery(from, to, bytes),
            Event::Fault(fault) => self.apply(&fault),
            Event::Break { node, peer } => {
                let name = self.name_of(node).to_owned();
                self.trace
                    .record(self.now, node, &name, trace::Event::Broken { to: peer });
                let effects = self.deliver(node, Input::TransportError { peer });
                self.perform(node, effects);
            }
        }
    }

    fn handle_delivery(&mut self, from: NodeId, to: NodeId, bytes: Bytes) {
        let name = self.name_of(to).to_owned();
        match parse_datagram(bytes, &Limits::default()) {
            Ok(message) => {
                self.trace.record(
                    self.now,
                    to,
                    &name,
                    trace::Event::Received {
                        from,
                        summary: trace::summarize(&message),
                    },
                );
                let effects = self.deliver(
                    to,
                    Input::Message {
                        from,
                        message: &message,
                    },
                );
                self.perform(to, effects);
            }
            Err(error) => {
                // Never silent. A message that serialized wrongly and a network with no traffic
                // on it look identical from a node's point of view, and only one of them is a bug.
                self.trace.record(
                    self.now,
                    to,
                    &name,
                    trace::Event::Malformed {
                        from,
                        reason: error.to_string(),
                    },
                );
            }
        }
    }

    fn deliver(&mut self, id: NodeId, input: Input<'_>) -> Vec<Effect> {
        // A killed node is not merely unreachable: it is not running, so it produces nothing even
        // for an input that somehow reached it.
        if self.killed.contains(&id) {
            return Vec::new();
        }
        let now = self.now;
        self.nodes
            .get_mut(id.index())
            .map_or_else(Vec::new, |node| node.on_input(now, input))
    }

    fn perform(&mut self, node: NodeId, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send { to, message } => self.send(node, to, message.as_ref()),
                Effect::SetTimer { timer, after } => {
                    let after = self.skewed(node, after);
                    let at = self.now.saturating_add(after);
                    self.timers.set((node, timer), self.now, after);
                    let name = self.name_of(node).to_owned();
                    self.trace
                        .record(self.now, node, &name, trace::Event::TimerSet { timer, at });
                }
                Effect::ClearTimer(timer) => {
                    self.timers.clear(&(node, timer));
                    let name = self.name_of(node).to_owned();
                    self.trace
                        .record(self.now, node, &name, trace::Event::TimerCleared(timer));
                }
                Effect::Note(what) => {
                    let name = self.name_of(node).to_owned();
                    self.trace
                        .record(self.now, node, &name, trace::Event::Note(what));
                }
            }
        }
    }

    fn send(&mut self, from: NodeId, to: NodeId, message: &Message) {
        let summary = trace::summarize(message);
        let name = self.name_of(from).to_owned();
        self.trace.record(
            self.now,
            from,
            &name,
            trace::Event::Sent {
                to,
                summary: summary.clone(),
            },
        );

        // Serialize here and re-parse on arrival, exactly as a wire would. A node that builds a
        // message it cannot itself write out is a bug this catches and a shared-object handoff
        // would not.
        let bytes = message.to_bytes();
        let now = self.now;
        let decision = self.link_between(from, to).decide(now);

        match decision {
            Delivery::Dropped => {
                self.trace
                    .record(self.now, from, &name, trace::Event::Dropped { to, summary });
            }
            Delivery::Broken => {
                self.queue.schedule_at(
                    self.now,
                    Event::Break {
                        node: from,
                        peer: to,
                    },
                );
            }
            Delivery::Once(after) => {
                self.queue
                    .schedule_after(self.now, after, Event::Deliver { from, to, bytes });
            }
            Delivery::Twice(first, second) => {
                self.trace.record(
                    self.now,
                    from,
                    &name,
                    trace::Event::Duplicated {
                        to,
                        summary: summary.clone(),
                    },
                );
                self.queue.schedule_after(
                    self.now,
                    first,
                    Event::Deliver {
                        from,
                        to,
                        bytes: bytes.clone(),
                    },
                );
                self.queue
                    .schedule_after(self.now, second, Event::Deliver { from, to, bytes });
            }
        }
    }

    fn link_between(&mut self, from: NodeId, to: NodeId) -> &mut Link {
        let kind = self.default_kind;
        let policy = self.default_policy;
        let seed = self.seed;
        // The label must not depend on when the link was created, or a scenario that happens to
        // send in a different order would draw different randomness for the same topology.
        let label = link_label(
            self.nodes.get(from.index()).map_or("?", |n| n.name()),
            self.nodes.get(to.index()).map_or("?", |n| n.name()),
        );
        self.links
            .entry((from, to))
            .or_insert_with(|| Link::new(kind, policy, seed, &label))
    }

    fn name_of(&self, id: NodeId) -> &str {
        self.nodes.get(id.index()).map_or("?", |node| node.name())
    }
}

fn link_label(from: &str, to: &str) -> String {
    format!("link:{from}->{to}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use sipx_sip::{HeaderName, Method, RequestBuilder, Uri};

    /// A node that answers every request with a `200`, and counts what it saw.
    #[derive(Debug)]
    struct Echo {
        name: String,
        seen: usize,
    }

    impl SimNode for Echo {
        fn name(&self) -> &str {
            &self.name
        }

        fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
            match input {
                Input::Message {
                    from,
                    message: Message::Request(request),
                } => {
                    self.seen += 1;
                    let response = sipx_sip::ResponseBuilder::to_request(
                        request,
                        sipx_sip::StatusCode::new(200).unwrap(),
                        "OK",
                    )
                    .unwrap()
                    .build();
                    vec![crate::node::send(from, Message::Response(response))]
                }
                _ => Vec::new(),
            }
        }
    }

    /// A node that sends one request at start and records the status it gets back.
    #[derive(Debug)]
    struct Caller {
        name: String,
        peer: NodeId,
        answered: Option<u16>,
    }

    impl SimNode for Caller {
        fn name(&self) -> &str {
            &self.name
        }

        fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
            match input {
                Input::Started => vec![crate::node::send(self.peer, options("call-1"))],
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

    fn options(call_id: &str) -> Message {
        let uri = Uri::parse(Bytes::from_static(b"sip:echo@example.test")).unwrap();
        Message::Request(
            RequestBuilder::new(Method::Options, uri)
                .header(HeaderName::CallId, call_id.to_owned())
                .unwrap()
                .cseq(1, &Method::Options)
                .unwrap()
                .header(HeaderName::From, "<sip:caller@example.test>;tag=a")
                .unwrap()
                .header(HeaderName::To, "<sip:echo@example.test>")
                .unwrap()
                .header(HeaderName::Via, "SIP/2.0/UDP caller.test;branch=z9hG4bK1")
                .unwrap()
                .build(),
        )
    }

    fn round_trip(seed: u64, policy: LinkPolicy) -> Sim {
        let mut sim = Sim::new(seed);
        sim.link_default(LinkKind::Datagram, policy);
        let echo = sim.add_node(Box::new(Echo {
            name: "echo".to_owned(),
            seen: 0,
        }));
        sim.add_node(Box::new(Caller {
            name: "caller".to_owned(),
            peer: echo,
            answered: None,
        }));
        sim
    }

    #[test]
    fn a_request_crosses_the_network_and_is_answered() {
        let mut sim = round_trip(1, LinkPolicy::CLEAN);
        sim.run_until_idle().unwrap();
        assert_eq!(sim.trace().notes(), ["answered 200"]);
        assert_eq!(sim.node::<Echo>(NodeId(0)).map(|e| e.seen), Some(1));
    }

    #[test]
    fn the_same_seed_produces_a_byte_identical_trace() {
        // The claim the whole harness rests on. Jitter and duplication are on, so the run has
        // plenty of opportunity to differ if anything reads a clock or walks a hash map.
        let policy = LinkPolicy::jittery(1, 40).with_duplication(0.5);
        let mut first = round_trip(0x00C0_FFEE, policy);
        let mut second = round_trip(0x00C0_FFEE, policy);
        first.run_until_idle().unwrap();
        second.run_until_idle().unwrap();
        assert_eq!(first.trace().render(), second.trace().render());
        assert!(first.trace().render().contains("duplicated"));
    }

    #[test]
    fn a_different_seed_produces_a_different_trace() {
        let policy = LinkPolicy::jittery(1, 40);
        let mut a = round_trip(1, policy);
        let mut b = round_trip(2, policy);
        a.run_until_idle().unwrap();
        b.run_until_idle().unwrap();
        assert_ne!(a.trace().render(), b.trace().render());
    }

    #[test]
    fn time_only_moves_when_the_simulation_advances() {
        let mut sim = round_trip(3, LinkPolicy::jittery(10, 10));
        sim.start().unwrap();
        // The request is on the link, due in 10ms. Until time is advanced, nothing has arrived.
        assert_eq!(sim.now(), SimTime::START);
        assert!(sim.trace().received_by(NodeId(0)).is_empty());
        sim.advance(Duration::from_millis(5)).unwrap();
        assert!(sim.trace().received_by(NodeId(0)).is_empty());
        sim.advance(Duration::from_millis(10)).unwrap();
        assert_eq!(sim.trace().received_by(NodeId(0)).len(), 1);
    }

    #[test]
    fn an_idle_simulation_fast_forwards_to_the_deadline() {
        let mut sim = round_trip(4, LinkPolicy::CLEAN);
        sim.run_until_idle().unwrap();
        let settled = sim.now();
        sim.advance(Duration::from_secs(3_600)).unwrap();
        assert_eq!(
            sim.now(),
            settled.saturating_add(Duration::from_secs(3_600))
        );
    }

    #[test]
    fn a_lost_request_is_traced_rather_than_vanishing() {
        let mut sim = round_trip(5, LinkPolicy::CLEAN.with_loss(1.0));
        sim.run_until_idle().unwrap();
        let dropped = sim
            .trace()
            .count(|entry| matches!(entry.event, trace::Event::Dropped { .. }));
        assert_eq!(dropped, 1);
        assert!(sim.trace().notes().is_empty());
    }

    #[test]
    fn a_cut_stream_link_reports_a_transport_error_to_the_sender() {
        let mut sim = Sim::new(6);
        sim.link_default(
            LinkKind::Stream,
            LinkPolicy {
                partitioned: true,
                ..LinkPolicy::CLEAN
            },
        );
        let echo = sim.add_node(Box::new(Echo {
            name: "echo".to_owned(),
            seen: 0,
        }));
        sim.add_node(Box::new(Caller {
            name: "caller".to_owned(),
            peer: echo,
            answered: None,
        }));
        sim.run_until_idle().unwrap();
        assert_eq!(
            sim.trace()
                .count(|entry| matches!(entry.event, trace::Event::Broken { .. })),
            1
        );
    }

    #[test]
    fn a_livelocked_scenario_fails_instead_of_hanging() {
        /// Two of these answer each other forever.
        #[derive(Debug)]
        struct PingPong {
            name: String,
            peer: Option<NodeId>,
        }

        impl SimNode for PingPong {
            fn name(&self) -> &str {
                &self.name
            }

            fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
                let peer = match input {
                    Input::Started => self.peer,
                    Input::Message { from, .. } => Some(from),
                    _ => None,
                };
                peer.map(|to| vec![crate::node::send(to, options("loop"))])
                    .unwrap_or_default()
            }
        }

        let mut sim = Sim::new(7);
        sim.step_budget(500);
        let a = sim.add_node(Box::new(PingPong {
            name: "a".to_owned(),
            peer: None,
        }));
        sim.add_node(Box::new(PingPong {
            name: "b".to_owned(),
            peer: Some(a),
        }));

        match sim.run_until_idle() {
            Err(SimError::StepBudgetExhausted { budget, .. }) => assert_eq!(budget, 500),
            other => panic!("expected a budget failure, got {other:?}"),
        }
    }

    #[test]
    fn a_reset_timer_fires_once_at_the_later_deadline() {
        #[derive(Debug)]
        struct Rearming {
            name: String,
            fired: usize,
        }

        impl SimNode for Rearming {
            fn name(&self) -> &str {
                &self.name
            }

            fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
                match input {
                    Input::Started => vec![
                        Effect::SetTimer {
                            timer: TimerId(1),
                            after: Duration::from_millis(100),
                        },
                        // Reset before the first ever fires. The stale entry must be discarded.
                        Effect::SetTimer {
                            timer: TimerId(1),
                            after: Duration::from_millis(300),
                        },
                    ],
                    Input::Timer(_) => {
                        self.fired += 1;
                        vec![Effect::Note("fired".to_owned())]
                    }
                    _ => Vec::new(),
                }
            }
        }

        let mut sim = Sim::new(8);
        sim.add_node(Box::new(Rearming {
            name: "node".to_owned(),
            fired: 0,
        }));
        sim.advance(Duration::from_millis(200)).unwrap();
        assert_eq!(sim.node::<Rearming>(NodeId(0)).map(|n| n.fired), Some(0));
        sim.advance(Duration::from_millis(200)).unwrap();
        assert_eq!(sim.node::<Rearming>(NodeId(0)).map(|n| n.fired), Some(1));
    }

    #[test]
    fn a_cleared_timer_never_fires() {
        #[derive(Debug)]
        struct Clearing {
            name: String,
            fired: usize,
        }

        impl SimNode for Clearing {
            fn name(&self) -> &str {
                &self.name
            }

            fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
                match input {
                    Input::Started => vec![
                        Effect::SetTimer {
                            timer: TimerId(9),
                            after: Duration::from_millis(50),
                        },
                        Effect::ClearTimer(TimerId(9)),
                    ],
                    Input::Timer(_) => {
                        self.fired += 1;
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
        }

        let mut sim = Sim::new(9);
        sim.add_node(Box::new(Clearing {
            name: "node".to_owned(),
            fired: 0,
        }));
        sim.advance(Duration::from_secs(10)).unwrap();
        assert_eq!(sim.node::<Clearing>(NodeId(0)).map(|n| n.fired), Some(0));
    }

    #[test]
    fn an_unparseable_arrival_is_traced_not_swallowed() {
        #[derive(Debug)]
        struct Garbage {
            name: String,
            peer: Option<NodeId>,
        }

        impl SimNode for Garbage {
            fn name(&self) -> &str {
                &self.name
            }

            fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
                match (input, self.peer) {
                    (Input::Started, Some(to)) => {
                        // A request whose start line is fine but whose framing is not: the body
                        // is shorter than Content-Length claims.
                        let uri = Uri::parse(Bytes::from_static(b"sip:x@y.test")).unwrap();
                        let mut request = RequestBuilder::new(Method::Options, uri).build();
                        request.headers.push(
                            sipx_sip::Header::build(HeaderName::ContentLength, "999").unwrap(),
                        );
                        vec![crate::node::send(to, Message::Request(request))]
                    }
                    _ => Vec::new(),
                }
            }
        }

        let mut sim = Sim::new(10);
        let sink = sim.add_node(Box::new(Garbage {
            name: "sink".to_owned(),
            peer: None,
        }));
        sim.add_node(Box::new(Garbage {
            name: "source".to_owned(),
            peer: Some(sink),
        }));
        sim.run_until_idle().unwrap();
        assert_eq!(
            sim.trace()
                .count(|entry| matches!(entry.event, trace::Event::Malformed { .. })),
            1,
            "{}",
            sim.trace().render()
        );
    }
}
