//! The `EP-*` vector tables of
//! [e2e-probe §10](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md),
//! row by row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bytes::Bytes;
use sipx_clstr_probe::engine::{Effect, Input, ProbeConfig, ProbeEngine, ProbeRun, Target};
use sipx_clstr_probe::{Cause, Marker, ProbeFault, Step, Verdict};
use sipx_sip::{Header, HeaderName, Request, Response, ResponseBuilder, StatusCode};

fn target() -> Target {
    Target {
        address: "sip.example".to_owned(),
        transport: "UDP".to_owned(),
        zone: "zone-a".to_owned(),
    }
}

fn config() -> ProbeConfig {
    ProbeConfig::new(
        "sip:probe@test.example",
        "sip:echo@test.example",
        "sip:probe@10.9.9.9",
        target(),
    )
}

fn marker() -> Marker {
    Marker::from_token("run-1")
}

fn engine() -> ProbeEngine {
    ProbeEngine::new(config(), marker())
}

/// What a peer does at a moment in virtual time.
///
/// A script rather than pre-built `Input`s, because a response must echo the `CSeq` of the request
/// that provoked it — as every real peer does — and only the driver knows what that was. Building
/// responses without one made these vectors unable to see a correlation bug the harness caught.
#[derive(Debug, Clone)]
enum Act {
    Start,
    Resolved(bool),
    /// Answer the outstanding request with this status, optionally reflecting a marker.
    Reply(u16, Option<Marker>),
    /// Answer with `503` and a `Retry-After`, which is what shedding looks like.
    Shed(u64),
    Timer(Step),
    TransportError,
    LocalResourceUnavailable,
}

/// Drives the engine and plays a peer: it remembers the `CSeq` of the last request that expects an
/// answer, and echoes it, exactly as a real peer would.
struct Peer {
    outstanding_cseq: u32,
    sent: Vec<Request>,
    finished: Option<ProbeRun>,
}

impl Peer {
    fn new() -> Self {
        Self {
            outstanding_cseq: 0,
            sent: Vec::new(),
            finished: None,
        }
    }

    fn absorb(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Send(request) => {
                    // An ACK expects no answer, so it does not become what a reply is addressed to.
                    if request.method != sipx_sip::Method::Ack
                        && let Some(cseq) = cseq_of(&request)
                    {
                        self.outstanding_cseq = cseq;
                    }
                    self.sent.push(*request);
                }
                Effect::Finish(run) => self.finished = Some(*run),
                _ => {}
            }
        }
    }

    fn response(&self, status: u16, with_marker: Option<&Marker>) -> Box<Response> {
        let mut built = ResponseBuilder::new(StatusCode::new(status).expect("a status"), "x")
            .expect("a response")
            .build();
        built.headers.push(
            Header::build(
                HeaderName::CSeq,
                format!("{} REGISTER", self.outstanding_cseq),
            )
            .expect("a CSeq"),
        );
        if let Some(marker) = with_marker {
            built.headers.push(
                Header::build(sipx_clstr_probe::MARKER_HEADER, marker.header_bytes())
                    .expect("a header"),
            );
        }
        Box::new(built)
    }
}

fn cseq_of(request: &Request) -> Option<u32> {
    let value = request.headers.value(&HeaderName::CSeq)?;
    String::from_utf8_lossy(&value)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Play a script against an engine, returning the finished run.
fn play(engine: &mut ProbeEngine, script: Vec<(u64, Act)>) -> Option<ProbeRun> {
    let mut peer = Peer::new();
    for (millis, act) in script {
        let now = Duration::from_millis(millis);
        let input = match act {
            Act::Start => Input::Start,
            Act::Resolved(ok) => Input::Resolved(if ok { Ok(()) } else { Err(()) }),
            Act::Reply(status, marker) => Input::Response(peer.response(status, marker.as_ref())),
            Act::Shed(retry_after) => {
                let mut built = peer.response(503, None);
                built.headers.push(
                    Header::build(
                        HeaderName::Other(Bytes::from_static(b"Retry-After")),
                        retry_after.to_string(),
                    )
                    .expect("a header"),
                );
                Input::Response(built)
            }
            Act::Timer(step) => Input::TimerFired(step),
            Act::TransportError => Input::TransportError,
            Act::LocalResourceUnavailable => Input::LocalResourceUnavailable,
        };
        let effects = engine.on_input(now, input);
        peer.absorb(effects);
    }
    peer.finished
}

/// Play a script and also return everything the probe sent.
fn play_capturing(
    engine: &mut ProbeEngine,
    script: Vec<(u64, Act)>,
) -> (Option<ProbeRun>, Vec<Request>) {
    let mut peer = Peer::new();
    for (millis, act) in script {
        let now = Duration::from_millis(millis);
        let input = match act {
            Act::Start => Input::Start,
            Act::Resolved(ok) => Input::Resolved(if ok { Ok(()) } else { Err(()) }),
            Act::Reply(status, marker) => Input::Response(peer.response(status, marker.as_ref())),
            Act::Shed(retry_after) => {
                let mut built = peer.response(503, None);
                built.headers.push(
                    Header::build(
                        HeaderName::Other(Bytes::from_static(b"Retry-After")),
                        retry_after.to_string(),
                    )
                    .expect("a header"),
                );
                Input::Response(built)
            }
            Act::Timer(step) => Input::TimerFired(step),
            Act::TransportError => Input::TransportError,
            Act::LocalResourceUnavailable => Input::LocalResourceUnavailable,
        };
        let effects = engine.on_input(now, input);
        peer.absorb(effects);
    }
    (peer.finished, peer.sent)
}

/// Every act of a run that succeeds all the way through.
fn happy_path() -> Vec<(u64, Act)> {
    vec![
        (0, Act::Start),
        (10, Act::Resolved(true)),
        (20, Act::Reply(200, None)),
        (40, Act::Reply(200, Some(marker()))),
        (60, Act::Reply(200, None)),
        // The de-registration that S4 requires.
        (70, Act::Reply(200, None)),
    ]
}

fn deregistered(sent: &[Request]) -> bool {
    sent.iter().any(|request| {
        request.method == sipx_sip::Method::Register
            && String::from_utf8_lossy(
                &request
                    .headers
                    .value(&HeaderName::Contact)
                    .unwrap_or_default(),
            )
            .contains("expires=0")
    })
}

// -------------------------------------------------------------------------- EP-P --------------

#[test]
fn ep_p_1_every_step_succeeds_and_the_marker_comes_back() {
    let mut engine = engine();
    let run = play(&mut engine, happy_path()).expect("the run should finish");

    assert_eq!(run.verdict, Verdict::Pass, "{run:?}");
    let attempted: Vec<Step> = run.steps.iter().map(|outcome| outcome.step).collect();
    assert_eq!(
        attempted,
        [Step::Resolve, Step::Register, Step::Invite, Step::Bye],
        "four steps, in order"
    );
    assert!(run.steps.iter().all(|outcome| outcome.succeeded));
    assert!(!run.cleanup_failed);
}

#[test]
fn ep_p_1_the_registration_is_removed_whatever_happens() {
    // S4. A probe that accumulates bindings degrades the thing it measures.
    let mut engine = engine();
    let (_, sent) = play_capturing(&mut engine, happy_path());
    assert!(deregistered(&sent), "the run must remove its own binding");
}

#[test]
fn ep_p_1_per_step_latency_is_recorded() {
    // §3 S5 — the latency of every attempted step, because a failure's latency is diagnostic.
    let mut engine = engine();
    let run = play(&mut engine, happy_path()).expect("finished");
    let invite = run
        .steps
        .iter()
        .find(|outcome| outcome.step == Step::Invite)
        .expect("an invite step");
    assert_eq!(invite.elapsed, Duration::from_millis(20));
}

#[test]
fn a_duplicated_response_for_an_earlier_step_is_not_the_current_steps_answer() {
    // The defect the harness found at seed 5: with the duplication UDP produces routinely, a second
    // `200` to REGISTER arrived while the probe was waiting on INVITE and was consumed as the
    // INVITE's answer — reporting a `MarkerMismatch` the probe manufactured. A response must be
    // matched to the request that provoked it.
    let mut engine = engine();
    let mut peer = Peer::new();

    peer.absorb(engine.on_input(Duration::ZERO, Input::Start));
    peer.absorb(engine.on_input(Duration::from_millis(10), Input::Resolved(Ok(()))));

    // Answer the REGISTER, and keep a copy of that very response.
    let register_answer = peer.response(200, None);
    peer.absorb(engine.on_input(
        Duration::from_millis(20),
        Input::Response(register_answer.clone()),
    ));

    // The duplicate arrives while INVITE is outstanding. It must be ignored.
    let effects = engine.on_input(Duration::from_millis(25), Input::Response(register_answer));
    assert!(
        effects.is_empty(),
        "a stale duplicate must not conclude the current step: {effects:?}"
    );

    // And the run still passes when the INVITE is genuinely answered.
    peer.absorb(engine.on_input(
        Duration::from_millis(30),
        Input::Response(peer.response(200, Some(&marker()))),
    ));
    peer.absorb(engine.on_input(
        Duration::from_millis(40),
        Input::Response(peer.response(200, None)),
    ));
    peer.absorb(engine.on_input(
        Duration::from_millis(50),
        Input::Response(peer.response(200, None)),
    ));
    assert_eq!(peer.finished.map(|run| run.verdict), Some(Verdict::Pass));
}

// -------------------------------------------------------------------------- EP-F --------------

#[test]
fn ep_f_1_a_name_that_does_not_resolve_fails_before_any_sip_is_sent() {
    let mut engine = engine();
    let (run, sent) = play_capturing(
        &mut engine,
        vec![(0, Act::Start), (10, Act::Resolved(false))],
    );
    let run = run.expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Resolve,
            cause: Cause::ResolutionFailed
        }
    );
    assert!(sent.is_empty(), "no REGISTER may be attempted (S1)");
    assert_eq!(run.steps.len(), 1, "only the step that ran is recorded");
}

#[test]
fn ep_f_2_a_register_timeout_stops_the_plan() {
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (5_010, Act::Timer(Step::Register)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Register,
            cause: Cause::Timeout
        }
    );
    assert!(
        !run.steps.iter().any(|outcome| outcome.step == Step::Invite),
        "no INVITE may be attempted"
    );
}

#[test]
fn ep_f_3_a_503_to_register_carries_the_status() {
    // The status is carried because 403 and 503 are different incidents.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(503, None)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Register,
            cause: Cause::Rejected { status: 503 }
        }
    );
}

#[test]
fn ep_f_4_a_480_to_invite_fails_and_sends_no_bye() {
    // No dialog exists, so there is nothing to hang up.
    let mut engine = engine();
    let (run, sent) = play_capturing(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(480, None)),
            (40, Act::Reply(200, None)),
        ],
    );
    let run = run.expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Invite,
            cause: Cause::Rejected { status: 480 }
        }
    );
    assert!(
        !sent.iter().any(|r| r.method == sipx_sip::Method::Bye),
        "no dialog, so no BYE"
    );
}

#[test]
fn ep_f_5_a_200_with_no_marker_is_a_mismatch_and_the_dialog_is_still_ended() {
    // S3 — a dialog exists, so the probe still hangs up. A probe that abandons dialogs becomes the
    // outage it was watching for.
    let mut engine = engine();
    let (run, sent) = play_capturing(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(200, None)), // answered, unmarked
            (40, Act::Reply(200, None)), // the BYE
            (50, Act::Reply(200, None)), // the de-registration
        ],
    );
    let run = run.expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Invite,
            cause: Cause::MarkerMismatch
        }
    );
    assert_eq!(
        sent.iter()
            .filter(|r| r.method == sipx_sip::Method::Bye)
            .count(),
        1,
        "the established dialog is still ended (S3), and exactly once"
    );
}

#[test]
fn ep_f_6_another_runs_marker_is_a_mismatch() {
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(200, Some(Marker::from_token("run-2")))),
            (40, Act::Reply(200, None)),
            (50, Act::Reply(200, None)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Invite,
            cause: Cause::MarkerMismatch
        }
    );
}

#[test]
fn ep_f_7_the_edge_answers_but_the_echo_never_rang() {
    // The design's named scenario, and the marker is the only thing that detects it: from the
    // probe's side a `200` from the edge and a `200` from the echo are the same message.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(200, None)),
            (40, Act::Reply(200, None)),
            (50, Act::Reply(200, None)),
        ],
    )
    .expect("finished");
    assert!(
        matches!(
            run.verdict,
            Verdict::Fail {
                step: Step::Invite,
                cause: Cause::MarkerMismatch
            }
        ),
        "{:?}",
        run.verdict
    );
}

#[test]
fn ep_f_8_a_bye_timeout_after_a_good_call_is_a_bye_failure() {
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(200, Some(marker()))),
            (5_030, Act::Timer(Step::Bye)),
            (5_040, Act::Reply(200, None)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Bye,
            cause: Cause::Timeout
        }
    );
}

#[test]
fn ep_f_9_a_refused_connection_is_unreachable_not_a_timeout() {
    // The transport said no, which is different from silence and reads differently in an incident.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::TransportError),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Register,
            cause: Cause::Unreachable
        }
    );
}

#[test]
fn ep_f_10_a_shed_probe_is_its_own_cause_not_an_ordinary_503() {
    // §8 B4. Shedding is the platform working as designed under load; silence means the listener is
    // gone. They must not read alike, or an operator learns to ignore both.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Shed(30)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Register,
            cause: Cause::Shed {
                retry_after: Some(Duration::from_secs(30))
            }
        }
    );
}

// -------------------------------------------------------------------------- EP-I --------------

#[test]
fn ep_i_1_a_challenge_the_probe_cannot_answer_is_inconclusive_not_an_outage() {
    // V3 — the rule the three-valued taxonomy exists for. A broken prober is not an outage.
    let mut engine = ProbeEngine::new(config().without_credentials(), marker());
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(401, None)),
        ],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Inconclusive {
            fault: ProbeFault::BadCredentials
        }
    );
    assert!(!run.verdict.is_platform_failure());
    assert!(!run.verdict.counts_as_platform_result());
}

#[test]
fn ep_i_2_a_repeated_challenge_after_answering_one_is_the_platforms_fault() {
    // V5 — the distinction is whether a credential was offered at all.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(407, None)),
            (30, Act::Reply(407, None)),
            (40, Act::Reply(200, None)),
        ],
    )
    .expect("finished");

    assert_eq!(
        run.verdict,
        Verdict::Fail {
            step: Step::Register,
            cause: Cause::Rejected { status: 407 }
        },
        "a re-challenge after answering is a platform behaviour"
    );
    assert!(run.verdict.is_platform_failure());
}

#[test]
fn ep_i_1_and_i_2_differ_only_by_whether_a_credential_existed() {
    // The same responses in the same order, opposite verdicts — which is exactly what V5 says the
    // rule is, and the pair is here so a future change cannot quietly collapse them.
    let mut with = engine();
    let answered = play(
        &mut with,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(407, None)),
            (30, Act::Reply(407, None)),
            (40, Act::Reply(200, None)),
        ],
    )
    .expect("finished");

    let mut without = ProbeEngine::new(config().without_credentials(), marker());
    let unanswerable = play(
        &mut without,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(407, None)),
        ],
    )
    .expect("finished");

    assert!(answered.verdict.is_platform_failure());
    assert!(!unanswerable.verdict.is_platform_failure());
}

#[test]
fn ep_i_3_a_misconfigured_probe_emits_no_sip_at_all() {
    // A probe that dialled *something* with a broken configuration would report a platform failure it
    // manufactured.
    let mut broken = config();
    broken.target.address = String::new();
    let mut engine = ProbeEngine::new(broken, marker());

    let (run, sent) = play_capturing(&mut engine, vec![(0, Act::Start)]);
    let run = run.expect("finished immediately");
    assert_eq!(
        run.verdict,
        Verdict::Inconclusive {
            fault: ProbeFault::Misconfigured
        }
    );
    assert!(sent.is_empty());
    assert!(run.steps.is_empty());
}

#[test]
fn ep_i_4_no_local_socket_is_the_probes_own_fault() {
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![(0, Act::Start), (10, Act::LocalResourceUnavailable)],
    )
    .expect("finished");
    assert_eq!(
        run.verdict,
        Verdict::Inconclusive {
            fault: ProbeFault::NoLocalResource
        }
    );
}

// -------------------------------------------------------------------------- EP-C --------------

#[test]
fn ep_c_1_a_failed_bye_still_removes_the_registration() {
    let mut engine = engine();
    let (run, sent) = play_capturing(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(200, Some(marker()))),
            (5_030, Act::Timer(Step::Bye)),
            (5_040, Act::Reply(200, None)),
        ],
    );
    let run = run.expect("finished");
    assert!(matches!(
        run.verdict,
        Verdict::Fail {
            step: Step::Bye,
            ..
        }
    ));
    assert!(deregistered(&sent), "S4 holds regardless of the verdict");
}

#[test]
fn ep_c_2_no_binding_remains_after_any_verdict() {
    for script in [
        happy_path(),
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (30, Act::Reply(486, None)),
            (40, Act::Reply(200, None)),
        ],
    ] {
        let mut engine = engine();
        let (_, sent) = play_capturing(&mut engine, script);
        assert!(deregistered(&sent));
    }
}

#[test]
fn ep_c_3_a_cleanup_failure_does_not_change_the_verdict() {
    // V6 — a leaked binding is a probe defect and a failed call is a platform one. A cleanup failure
    // that rewrote the verdict would report the probe's own defect as the platform's.
    let mut engine = engine();
    let run = play(
        &mut engine,
        vec![
            (0, Act::Start),
            (10, Act::Resolved(true)),
            (20, Act::Reply(200, None)),
            (40, Act::Reply(200, Some(marker()))),
            (60, Act::Reply(200, None)),
            // The de-registration is refused.
            (70, Act::Reply(500, None)),
        ],
    )
    .expect("finished");

    assert_eq!(run.verdict, Verdict::Pass, "the call itself succeeded");
    assert!(
        run.cleanup_failed,
        "and the cleanup failure is recorded separately"
    );
}

// ---------------------------------------------------------------- properties -------------------

#[test]
fn a_finished_run_ignores_late_input() {
    let mut engine = engine();
    play(&mut engine, happy_path());
    assert!(engine.is_finished());
    assert!(
        engine
            .on_input(Duration::from_secs(99), Input::TimerFired(Step::Bye))
            .is_empty()
    );
}

#[test]
fn a_stale_timer_for_a_concluded_step_is_ignored() {
    let mut engine = engine();
    let _ = engine.on_input(Duration::ZERO, Input::Start);
    let _ = engine.on_input(Duration::from_millis(10), Input::Resolved(Ok(())));
    // The Resolve timer fires after Resolve already concluded.
    assert!(
        engine
            .on_input(Duration::from_millis(11), Input::TimerFired(Step::Resolve))
            .is_empty()
    );
}
