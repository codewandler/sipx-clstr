//! The probe engine under the harness — `ET-2`'s second half, and `EP-P-2`.
//!
//! The unit vectors feed the engine one input at a time in an order the test chose. This lets the
//! simulated network choose it instead, with jitter and duplication on, and asserts that the verdict
//! is the same and the run replays byte for byte from its seed.
//!
//! That is the property the whole sans-IO shape exists for: a probe's interesting behaviour is what
//! it does when a call *fails*, and those scenarios have to be seeded harness tests before they are a
//! cron job that pages someone at 03:00.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_probe::engine::{
    Effect as ProbeEffect, Input as ProbeInput, ProbeConfig, ProbeEngine, ProbeRun,
    Target as ProbeTarget,
};
use sipx_clstr_probe::{Marker, Step, Verdict};
use sipx_clstr_sim::node::{Effect, Input, SimNode, TimerId, send};
use sipx_clstr_sim::{LinkKind, LinkPolicy, NodeId, Sim, SimTime};
use sipx_sip::{Header, Message, Method, Request, Response, ResponseBuilder, StatusCode};

/// The probe, as a simulated node.
#[derive(Debug)]
struct Probe {
    name: String,
    engine: ProbeEngine,
    /// Where the platform is, from the probe's side of the network.
    edge: NodeId,
    /// The completed run, once there is one.
    run: Option<ProbeRun>,
}

impl Probe {
    fn new(name: &str, edge: NodeId, marker: &Marker) -> Self {
        let config = ProbeConfig::new(
            "sip:probe@test.example",
            "sip:echo@test.example",
            "sip:probe@10.9.9.9",
            ProbeTarget {
                address: "sip.example".to_owned(),
                transport: "UDP".to_owned(),
                zone: "zone-a".to_owned(),
            },
        );
        Self {
            name: name.to_owned(),
            engine: ProbeEngine::new(config, marker.clone()),
            edge,
            run: None,
        }
    }

    fn perform(&mut self, effects: Vec<ProbeEffect>, now: SimTime) -> Vec<Effect> {
        let mut out = Vec::new();
        for effect in effects {
            match effect {
                ProbeEffect::Resolve(_) => {
                    // The driver answers resolution; a real one asks a resolver. Fed straight back
                    // rather than through the network, because DNS is not on the SIP link.
                    let more = self.engine.on_input(
                        Duration::from_nanos(now.as_nanos()),
                        ProbeInput::Resolved(Ok(())),
                    );
                    out.extend(self.perform(more, now));
                }
                ProbeEffect::Send(request) => {
                    out.push(Effect::Note(format!("-> {}", method_of(&request))));
                    out.push(send(self.edge, Message::Request(*request)));
                }
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

impl SimNode for Probe {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let clock = Duration::from_nanos(now.as_nanos());
        let effects = match input {
            Input::Started => self.engine.on_input(clock, ProbeInput::Start),
            Input::Message {
                message: Message::Response(response),
                ..
            } => self
                .engine
                .on_input(clock, ProbeInput::Response(Box::new(response.clone()))),
            Input::Timer(timer) => {
                let Some(step) = step_of(timer) else {
                    return Vec::new();
                };
                self.engine.on_input(clock, ProbeInput::TimerFired(step))
            }
            Input::TransportError { .. } => self.engine.on_input(clock, ProbeInput::TransportError),
            Input::Message { .. } => return Vec::new(),
        };
        self.perform(effects, now)
    }
}

fn timer_of(step: Step) -> TimerId {
    TimerId(match step {
        Step::Resolve => 0,
        Step::Register => 1,
        Step::Invite => 2,
        Step::Bye => 3,
    })
}

fn step_of(timer: TimerId) -> Option<Step> {
    match timer.0 {
        0 => Some(Step::Resolve),
        1 => Some(Step::Register),
        2 => Some(Step::Invite),
        3 => Some(Step::Bye),
        _ => None,
    }
}

fn method_of(request: &Request) -> String {
    format!("{:?}", request.method).to_uppercase()
}

/// The platform, reduced to what a probe can observe: it answers, and the echo reflects the marker.
///
/// Deliberately *not* the real proxy and registrar. `CX-3` runs the probe against those; what this
/// scenario is about is the engine's behaviour when the **network** chooses the ordering, and mixing
/// in a second subject would make a failure ambiguous.
#[derive(Debug)]
struct Platform {
    name: String,
    /// Whether the echo reflects the marker. `false` is the design's named failure — the edge answers
    /// but the echo never rang.
    echo_reflects: bool,
    seen: Vec<String>,
}

impl SimNode for Platform {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_input(&mut self, _now: SimTime, input: Input<'_>) -> Vec<Effect> {
        let Input::Message {
            from,
            message: Message::Request(request),
        } = input
        else {
            return Vec::new();
        };
        self.seen.push(method_of(request));

        // An ACK is not answered — it is the end of a transaction, not a request for one.
        if request.method == Method::Ack {
            return Vec::new();
        }

        let mut response =
            ResponseBuilder::to_request(request, StatusCode::new(200).expect("a status"), "OK")
                .expect("a response")
                .build();

        if request.method == Method::Invite && self.echo_reflects {
            copy_marker(request, &mut response);
        }
        vec![send(from, Message::Response(response))]
    }
}

fn copy_marker(request: &Request, response: &mut Response) {
    if let Some(value) = request.headers.value(&sipx_clstr_probe::MARKER_HEADER)
        && let Ok(header) = Header::build(
            sipx_clstr_probe::MARKER_HEADER,
            Bytes::copy_from_slice(value.as_ref()),
        )
    {
        response.headers.push(header);
    }
}

const PROBE: NodeId = NodeId::from_index(1);

fn scenario(seed: u64, policy: LinkPolicy, echo_reflects: bool) -> Sim {
    let mut sim = Sim::new(seed);
    sim.link_default(LinkKind::Datagram, policy);
    let platform = sim.add_node(Box::new(Platform {
        name: "platform".to_owned(),
        echo_reflects,
        seen: Vec::new(),
    }));
    sim.add_node(Box::new(Probe::new(
        "probe",
        platform,
        &Marker::from_token("harness-run"),
    )));
    sim
}

#[test]
fn ep_p_1_a_probe_run_passes_across_the_simulated_network() {
    let mut sim = scenario(0x0e2e_0001, LinkPolicy::CLEAN, true);
    sim.advance(Duration::from_secs(30)).expect("settles");

    let run = sim
        .node::<Probe>(PROBE)
        .and_then(|probe| probe.run.clone())
        .expect("the run should finish");
    assert_eq!(run.verdict, Verdict::Pass, "{}", sim.trace().render());
    assert_eq!(run.steps.len(), 4);
}

#[test]
fn ep_p_2_a_passing_run_replays_byte_for_byte_under_jitter() {
    // The row that needed the harness: the same seed, the same trace, with the network free to
    // reorder and duplicate.
    let policy = LinkPolicy::jittery(1, 40).with_duplication(0.3);
    for seed in 0..8_u64 {
        let mut first = scenario(seed, policy, true);
        let mut second = scenario(seed, policy, true);
        first.advance(Duration::from_secs(30)).expect("runs");
        second.advance(Duration::from_secs(30)).expect("runs");
        assert_eq!(
            first.trace().render(),
            second.trace().render(),
            "seed {seed} diverged"
        );
    }
}

#[test]
fn ep_p_2_the_verdict_is_the_same_however_the_network_behaves() {
    let policy = LinkPolicy::jittery(1, 40).with_duplication(0.3);
    for seed in 0..16_u64 {
        let mut sim = scenario(0x0e2e_0100 + seed, policy, true);
        sim.advance(Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let verdict = sim
            .node::<Probe>(PROBE)
            .and_then(|probe| probe.run.as_ref().map(|run| run.verdict.clone()));
        assert_eq!(
            verdict,
            Some(Verdict::Pass),
            "seed {seed}\n{}",
            sim.trace().render()
        );
    }
}

#[test]
fn the_named_failure_is_detected_across_the_network_too() {
    // The edge answers `200` and the echo never rang. From the probe's side those two `200`s are the
    // same message; only the marker tells them apart.
    let mut sim = scenario(0x0e2e_0002, LinkPolicy::jittery(1, 20), false);
    sim.advance(Duration::from_secs(30)).expect("settles");

    let run = sim
        .node::<Probe>(PROBE)
        .and_then(|probe| probe.run.clone())
        .expect("finished");
    assert!(
        matches!(
            run.verdict,
            Verdict::Fail {
                step: Step::Invite,
                cause: sipx_clstr_probe::Cause::MarkerMismatch
            }
        ),
        "{:?}\n{}",
        run.verdict,
        sim.trace().render()
    );
}

#[test]
// One probe interval of virtual time, sized against the scheduler's 60 s cadence.
#[allow(clippy::duration_suboptimal_units)]
fn a_platform_that_never_answers_times_out_rather_than_hanging() {
    // The probe must conclude within its own timer budget. A probe that hung would be a monitor that
    // stops reporting exactly when the thing it monitors has stopped working.
    let mut sim = Sim::new(0x0e2e_0003);
    sim.link_default(
        LinkKind::Datagram,
        LinkPolicy {
            partitioned: true,
            ..LinkPolicy::CLEAN
        },
    );
    let platform = sim.add_node(Box::new(Platform {
        name: "platform".to_owned(),
        echo_reflects: true,
        seen: Vec::new(),
    }));
    sim.add_node(Box::new(Probe::new(
        "probe",
        platform,
        &Marker::from_token("unreachable"),
    )));

    sim.advance(Duration::from_secs(60)).expect("settles");
    let run = sim
        .node::<Probe>(PROBE)
        .and_then(|probe| probe.run.clone())
        .expect("the run must conclude");
    assert!(
        matches!(
            run.verdict,
            Verdict::Fail {
                step: Step::Register,
                ..
            }
        ),
        "{:?}",
        run.verdict
    );
}

#[test]
fn the_probe_cleans_up_after_itself_over_the_network() {
    // S4 through a real message path: the de-registration reaches the platform.
    let mut sim = scenario(0x0e2e_0004, LinkPolicy::CLEAN, true);
    sim.advance(Duration::from_secs(30)).expect("settles");

    let seen = sim
        .node::<Platform>(NodeId::from_index(0))
        .map(|platform| platform.seen.clone())
        .unwrap_or_default();
    assert_eq!(
        seen.iter().filter(|method| *method == "REGISTER").count(),
        2,
        "one registration and one de-registration: {seen:?}"
    );
    assert!(seen.contains(&"BYE".to_owned()), "{seen:?}");
}
