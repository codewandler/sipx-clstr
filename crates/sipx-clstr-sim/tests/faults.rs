//! `CF-4`: fault schedules compose with any scenario, and failing seeds reproduce exactly.
//!
//! The scenario here is deliberately tiny — two chatty nodes and a responder — because what is
//! under test is the *weather*, not a protocol. A fault suite written over the register-then-call
//! pipeline would pass or fail for reasons belonging to `RG-6`, and the first question about a
//! failure would be which layer broke.
//!
//! The property that matters most is the last one: a schedule must not make a run less
//! reproducible than it was without one. A harness whose failure injection is itself
//! non-deterministic turns every red build into a coin flip, which is the opposite of the point.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use sipx_clstr_sim::fault::{Fault, Schedule};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};

const PING: TimerId = TimerId(1);

/// A node that pings its peer on a timer, forever, and counts what comes back.
///
/// Self-rearming on purpose: it keeps producing traffic for the whole run, so a partition in the
/// middle shows up as a gap rather than as a scenario that had already finished.
#[derive(Debug)]
struct Pinger {
    name: String,
    peer: NodeId,
    heard: usize,
    sent: usize,
}

impl SimNode for Pinger {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        match input {
            Input::Started => vec![Effect::SetTimer {
                timer: PING,
                after: Duration::from_secs(1),
            }],
            Input::Timer(PING) => {
                self.sent += 1;
                vec![
                    Effect::Send {
                        to: self.peer,
                        message: Box::new(ping_message()),
                    },
                    Effect::SetTimer {
                        timer: PING,
                        after: Duration::from_secs(1),
                    },
                ]
            }
            Input::Message { .. } => {
                self.heard += 1;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// A node that answers nothing and simply records what reached it.
#[derive(Debug)]
struct Sink {
    name: String,
    received: usize,
}

impl SimNode for Sink {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        if matches!(input, Input::Message { .. }) {
            self.received += 1;
        }
        Vec::new()
    }
}

fn ping_message() -> sipx_sip::Message {
    use sipx_sip::{HeaderName, Method, RequestBuilder, Uri};
    let uri = Uri::parse(bytes::Bytes::from_static(b"sip:sink@example")).expect("a URI");
    sipx_sip::Message::Request(
        RequestBuilder::new(Method::Options, uri)
            .header(HeaderName::CallId, "ping")
            .unwrap()
            .cseq(1, &Method::Options)
            .unwrap()
            .header(HeaderName::From, "<sip:pinger@example>;tag=p")
            .unwrap()
            .header(HeaderName::To, "<sip:sink@example>")
            .unwrap()
            .header(
                HeaderName::Via,
                "SIP/2.0/UDP pinger.example;branch=z9hG4bK-p",
            )
            .unwrap()
            .header(HeaderName::MaxForwards, "70")
            .unwrap()
            .build(),
    )
}

const PINGER: NodeId = NodeId::from_index(0);
const SINK: NodeId = NodeId::from_index(1);

/// One pinger, one sink, a clean datagram link, run for `seconds` under `schedule`.
fn run(seed: u64, schedule: &Schedule, seconds: u64) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);
    sim.add_node(Box::new(Pinger {
        name: "pinger".to_owned(),
        peer: SINK,
        heard: 0,
        sent: 0,
    }));
    sim.add_node(Box::new(Sink {
        name: "sink".to_owned(),
        received: 0,
    }));
    sim.schedule(schedule);
    sim.run_until(SimTime::from_secs(seconds)).expect("settles");
    sim
}

fn received(sim: &Sim) -> usize {
    sim.node::<Sink>(SINK).map_or(0, |sink| sink.received)
}

#[test]
fn a_scenario_with_no_schedule_is_unchanged() {
    // The zero case, asserted rather than assumed: adding the fault machinery must not perturb a
    // scenario that never uses it, or every existing pinned trace would have shifted.
    let sim = run(0xCF04_0001, &Schedule::new(), 10);
    assert_eq!(received(&sim), 10, "one ping a second, all delivered");
    assert!(
        !sim.trace().render().contains("FAULT"),
        "no schedule, no weather in the trace"
    );
}

#[test]
fn a_partition_window_silences_the_link_and_the_heal_restores_it() {
    let schedule = Schedule::new().partition_during(
        SimTime::from_secs(3),
        SimTime::from_secs(6),
        vec![PINGER],
        vec![SINK],
    );
    let sim = run(0xCF04_0002, &schedule, 10);

    // Pings at 1..=10. Those at 3, 4 and 5 fall inside the cut. The ping at 6 survives: the heal
    // was queued during setup, so at the instant they collide it carries the lower insertion
    // sequence and is applied first — the same tie-break every other event obeys.
    assert_eq!(
        received(&sim),
        7,
        "three pings lost to the cut:\n{}",
        sim.trace().render()
    );
    let rendered = sim.trace().render();
    assert!(rendered.contains("FAULT partition"), "{rendered}");
    assert!(rendered.contains("FAULT heal"), "{rendered}");
}

#[test]
fn killing_a_node_stops_it_producing_anything_further() {
    let schedule = Schedule::new().at(SimTime::from_secs(4), Fault::KillNode(PINGER));
    let sim = run(0xCF04_0003, &schedule, 10);

    assert!(sim.is_killed(PINGER));
    assert_eq!(
        received(&sim),
        3,
        "pings at 1, 2 and 3 only:\n{}",
        sim.trace().render()
    );
    // The distinction the story cares about: a killed node's timers are gone, so nothing fires
    // after the kill. An isolated node would keep timing out.
    let rendered = sim.trace().render();
    let after_kill: Vec<&str> = rendered
        .lines()
        .filter(|line| line.contains("pinger") && line.contains("timer"))
        .filter(|line| {
            line.split_whitespace()
                .next()
                .and_then(|at| at.split('.').next())
                .and_then(|secs| secs.trim().parse::<u64>().ok())
                .is_some_and(|secs| secs > 4)
        })
        .map(str::trim)
        .collect();
    assert!(
        after_kill.is_empty(),
        "a killed node's timers must not fire: {after_kill:?}"
    );
}

#[test]
fn timer_skew_changes_what_a_node_waits_for() {
    // A clock running at half speed waits twice as long, so half as many pings fit in the run.
    let schedule = Schedule::new().at(
        SimTime::START,
        Fault::TimerSkew {
            node: PINGER,
            per_mille: 2000,
        },
    );
    let sim = run(0xCF04_0004, &schedule, 10);
    // Five, not four, and the difference is the contract: nodes are started *before* the queue is
    // drained, so the timer armed in response to `Started` is armed at nominal rate and fires at
    // 1. Everything the node arms after that is skewed — 3, 5, 7, 9. A fault at `START` is weather
    // that arrives at a running scenario, not a property a node is born with.
    assert_eq!(
        received(&sim),
        5,
        "the first interval is nominal, then 3, 5, 7, 9:\n{}",
        sim.trace().render()
    );
}

#[test]
fn a_lossy_policy_can_be_scheduled_mid_run() {
    let schedule = Schedule::new().at(
        SimTime::from_secs(5),
        Fault::SetLinkPolicy {
            from: PINGER,
            to: SINK,
            policy: LinkPolicy::CLEAN.with_loss(1.0),
        },
    );
    let sim = run(0xCF04_0005, &schedule, 10);
    assert_eq!(
        received(&sim),
        4,
        "everything from second 5 is lost:\n{}",
        sim.trace().render()
    );
}

#[test]
fn schedules_compose_and_the_merged_one_is_the_two_applied() {
    let kill = Schedule::new().at(SimTime::from_secs(7), Fault::KillNode(PINGER));
    let cut = Schedule::new().partition_during(
        SimTime::from_secs(2),
        SimTime::from_secs(4),
        vec![PINGER],
        vec![SINK],
    );

    let merged = run(0xCF04_0006, &cut.clone().merge(kill.clone()), 10);
    // Applying them as two separate `schedule` calls must be indistinguishable from merging
    // first, because composition is concatenation and the queue does not care who queued what.
    let mut separately = Sim::new(0xCF04_0006);
    separately.link_default(LinkKind::Datagram, LinkPolicy::CLEAN);
    separately.add_node(Box::new(Pinger {
        name: "pinger".to_owned(),
        peer: SINK,
        heard: 0,
        sent: 0,
    }));
    separately.add_node(Box::new(Sink {
        name: "sink".to_owned(),
        received: 0,
    }));
    separately.schedule(&cut);
    separately.schedule(&kill);
    separately
        .run_until(SimTime::from_secs(10))
        .expect("settles");

    assert_eq!(
        merged.trace().render(),
        separately.trace().render(),
        "merging two schedules and queuing them separately must be the same run"
    );
}

#[test]
fn a_scheduled_run_replays_byte_for_byte_from_its_seed() {
    // The criterion with teeth. A fault schedule must not make a run less reproducible than it was
    // without one — otherwise a failing seed is a story rather than a bug report.
    let schedule = || {
        Schedule::new()
            .partition_during(
                SimTime::from_secs(2),
                SimTime::from_secs(5),
                vec![PINGER],
                vec![SINK],
            )
            .at(
                SimTime::from_secs(6),
                Fault::SetLinkPolicy {
                    from: PINGER,
                    to: SINK,
                    policy: LinkPolicy::CLEAN.with_loss(0.5).with_duplication(0.25),
                },
            )
            .after(Duration::from_secs(9), Fault::KillNode(PINGER))
    };

    let first = run(0xCF04_5EED, &schedule(), 12);
    let second = run(0xCF04_5EED, &schedule(), 12);
    assert_eq!(
        first.trace().render(),
        second.trace().render(),
        "same seed, same schedule, same trace"
    );

    // And a different seed must actually explore something else, or the loss knob above is not
    // reaching the generator and the reproduction above proves nothing.
    let elsewhere = run(0xCF04_0FF5, &schedule(), 12);
    assert_ne!(
        first.trace().render(),
        elsewhere.trace().render(),
        "a different seed must produce a different run, or the faults are not seeded at all"
    );
}
