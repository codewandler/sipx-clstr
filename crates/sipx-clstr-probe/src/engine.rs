//! The probe engine — [e2e-probe §2, §3](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md).
//!
//! A state machine and nothing else: inputs in, ordered effects out. Time enters as fired timers and
//! jitter as an injected random source, so the whole plan runs unmodified inside the deterministic
//! harness. A probe whose failure scenarios cannot be replayed from a seed is a probe nobody will
//! trust at 03:00 — which is the only hour it matters.

use std::time::Duration;

use bytes::Bytes;
use sipx_sip::{HeaderName, Message, Method, Request, RequestBuilder, Uri};

use crate::marker::{MARKER_HEADER, Marker};
use crate::verdict::{Cause, ProbeFault, Step, StepOutcome, Verdict};

/// Where a probe dials, and how (§5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The address dialled: a DNS name, the VIP, or one edge address. **Never** an internal address —
    /// a probe that skips the front door cannot detect a broken front door.
    pub address: String,
    /// The transport token (`UDP`, `TCP`, `TLS`, `WS`, `WSS`).
    pub transport: String,
    /// Which zone this target belongs to, for attributing a failure.
    pub zone: String,
}

/// What a probe needs to know before it can run.
#[derive(Debug, Clone)]
pub struct ProbeConfig {
    /// The probe's own address-of-record, in the test tenant.
    pub probe_aor: String,
    /// The echo's address-of-record, in the same tenant.
    pub echo_aor: String,
    /// The contact the probe registers.
    pub contact: String,
    /// Where to dial.
    pub target: Target,
    /// Per-step timeouts. Defaults are §3's.
    pub timeouts: [Duration; 4],
    /// The host this probe puts in its `Via`, so responses can find their way back.
    pub sent_by: String,
    /// Whether the probe has a credential it can offer to a challenge.
    ///
    /// The credential itself belongs to the driver — this engine is sans-IO and holds no secrets —
    /// but *whether one exists* is what §6 V5 turns on: a challenge the probe cannot answer is its
    /// own account (`Inconclusive`), and a challenge it answered and got again is the platform's
    /// (`Fail`). Without this flag the engine could only ever report the first, which would make a
    /// re-challenging registrar invisible.
    pub has_credentials: bool,
}

impl ProbeConfig {
    /// A configuration with the specification's default timeouts.
    #[must_use]
    pub fn new(probe_aor: &str, echo_aor: &str, contact: &str, target: Target) -> Self {
        Self {
            probe_aor: probe_aor.to_owned(),
            echo_aor: echo_aor.to_owned(),
            contact: contact.to_owned(),
            target,
            timeouts: [
                Step::Resolve.default_timeout(),
                Step::Register.default_timeout(),
                Step::Invite.default_timeout(),
                Step::Bye.default_timeout(),
            ],
            sent_by: "probe.invalid".to_owned(),
            has_credentials: true,
        }
    }

    /// The same configuration, with the host this probe is reachable at.
    #[must_use]
    pub fn sent_by(mut self, host: &str) -> Self {
        host.clone_into(&mut self.sent_by);
        self
    }

    /// The same configuration, but with no credential to offer.
    #[must_use]
    pub fn without_credentials(mut self) -> Self {
        self.has_credentials = false;
        self
    }

    fn timeout(&self, step: Step) -> Duration {
        let index = match step {
            Step::Resolve => 0,
            Step::Register => 1,
            Step::Invite => 2,
            Step::Bye => 3,
        };
        self.timeouts
            .get(index)
            .copied()
            .unwrap_or(Duration::from_secs(5))
    }

    /// Whether this configuration can produce a valid run at all (§6 `Misconfigured`).
    fn is_usable(&self) -> bool {
        !self.probe_aor.is_empty()
            && !self.echo_aor.is_empty()
            && !self.contact.is_empty()
            && !self.target.address.is_empty()
    }
}

/// Something that happened to a run.
#[derive(Debug)]
pub enum Input {
    /// Begin.
    Start,
    /// P1 answered.
    Resolved(Result<(), ()>),
    /// A final response arrived for whatever is outstanding.
    Response(Box<sipx_sip::Response>),
    /// The transport failed.
    TransportError,
    /// A step's timeout fired.
    TimerFired(Step),
    /// The run was cancelled — shutdown, or superseded.
    Cancelled,
    /// The probe could not obtain a local resource.
    LocalResourceUnavailable,
}

/// Something the driver must do.
#[derive(Debug)]
pub enum Effect {
    /// Resolve the target.
    Resolve(Target),
    /// Put a request on the wire.
    Send(Box<Request>),
    /// Arm a step's timeout.
    SetTimer {
        /// Which step it guards.
        step: Step,
        /// How long from now.
        after: Duration,
    },
    /// Disarm a step's timeout.
    ClearTimer(Step),
    /// The run is over.
    Finish(Box<ProbeRun>),
}

/// A completed run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRun {
    /// The marker this run planted.
    pub marker: Marker,
    /// Where it dialled.
    pub target: Target,
    /// Every step attempted, in order — including the one that failed (§2).
    pub steps: Vec<StepOutcome>,
    /// What it concluded.
    pub verdict: Verdict,
    /// Whether cleanup itself failed. Recorded separately: a leaked binding is a probe defect and a
    /// failed call is a platform one, and §6 V6 keeps the verdict from confusing them.
    pub cleanup_failed: bool,
}

/// What a run still owes, and what it has learned.
///
/// The two cleanup obligations are a *set* rather than two flags, because S3 and S4 are the same
/// rule — "undo what you did, in reverse order" — and modelling them separately is how one of them
/// gets forgotten in a new terminal path.
#[derive(Debug, Default, Clone)]
struct RunState {
    /// Outstanding cleanup, innermost first: a dialog is torn down before a registration is removed.
    owes: Vec<Obligation>,
    /// Cleanup itself failed — recorded, never allowed to change the verdict (§6 V6).
    cleanup_failed: bool,
    /// The run is over.
    finished: bool,
    /// A challenge has been answered once, which is what §6 V5 turns on.
    answered_challenge: bool,
}

/// Something the run must undo before it may finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Obligation {
    /// A dialog exists (S3).
    EndDialog,
    /// A registration exists (S4).
    Deregister,
}

impl RunState {
    fn owe(&mut self, obligation: Obligation) {
        if !self.owes.contains(&obligation) {
            self.owes.push(obligation);
        }
    }

    fn forget(&mut self, obligation: Obligation) {
        self.owes.retain(|owed| *owed != obligation);
    }

    fn owes(&self, obligation: Obligation) -> bool {
        self.owes.contains(&obligation)
    }
}

/// What the engine is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    Nothing,
    Step(Step),
    /// Cleanup after the verdict is already decided (§3 S3, S4).
    Cleanup(Step),
}

/// One probe run, as a state machine.
#[derive(Debug)]
pub struct ProbeEngine {
    config: ProbeConfig,
    marker: Marker,
    waiting: Waiting,
    steps: Vec<StepOutcome>,
    step_started: Duration,
    now: Duration,
    /// Set once the plan has produced a verdict; cleanup may still be outstanding.
    decided: Option<Verdict>,
    /// What the run still owes, and what it has already learned.
    state: RunState,
    cseq: u32,
    /// The `CSeq` number of the request the engine is waiting on.
    ///
    /// **A response must be matched to the request that provoked it.** Without this the engine
    /// consumed whatever arrived while a step was outstanding, so a duplicated `200` to REGISTER —
    /// which UDP produces routinely — was read as the INVITE's answer and reported
    /// `MarkerMismatch`: a platform failure the probe manufactured. Found by the harness at seed 5,
    /// which is what an adversarial network is for.
    outstanding_cseq: Option<u32>,
}

impl ProbeEngine {
    /// A run of `config`, with `marker` planted in every request.
    #[must_use]
    pub fn new(config: ProbeConfig, marker: Marker) -> Self {
        Self {
            config,
            marker,
            waiting: Waiting::Nothing,
            steps: Vec::new(),
            step_started: Duration::ZERO,
            now: Duration::ZERO,
            decided: None,
            state: RunState::default(),
            cseq: 0,
            outstanding_cseq: None,
        }
    }

    /// Whether the run is over.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.finished
    }

    /// Feed one input at virtual time `now`; get the effects to perform, in order.
    pub fn on_input(&mut self, now: Duration, input: Input) -> Vec<Effect> {
        if self.state.finished {
            return Vec::new();
        }
        self.now = now;
        match input {
            Input::Start => self.start(),
            Input::Resolved(outcome) => self.on_resolved(outcome),
            Input::Response(response) => self.on_response(&response),
            Input::TransportError => self.on_transport_error(),
            Input::TimerFired(step) => self.on_timeout(step),
            Input::Cancelled => self.abandon(ProbeFault::Cancelled),
            Input::LocalResourceUnavailable => self.abandon(ProbeFault::NoLocalResource),
        }
    }

    // ------------------------------------------------------------------ the plan --------------

    fn start(&mut self) -> Vec<Effect> {
        // §6 `Misconfigured` — and no SIP traffic is emitted at all. A probe that dialled *something*
        // with a broken configuration would report a platform failure it manufactured.
        if !self.config.is_usable() {
            return self.abandon(ProbeFault::Misconfigured);
        }
        self.begin(Step::Resolve);
        vec![
            Effect::Resolve(self.config.target.clone()),
            Effect::SetTimer {
                step: Step::Resolve,
                after: self.config.timeout(Step::Resolve),
            },
        ]
    }

    fn on_resolved(&mut self, outcome: Result<(), ()>) -> Vec<Effect> {
        if self.waiting != Waiting::Step(Step::Resolve) {
            return Vec::new();
        }
        if outcome.is_err() {
            self.conclude_step(false);
            return self.fail(Step::Resolve, Cause::ResolutionFailed);
        }
        self.conclude_step(true);
        self.send_register()
    }

    fn send_register(&mut self) -> Vec<Effect> {
        self.begin(Step::Register);
        let request = self.register_request(3_600);
        vec![
            Effect::ClearTimer(Step::Resolve),
            Effect::Send(Box::new(request)),
            Effect::SetTimer {
                step: Step::Register,
                after: self.config.timeout(Step::Register),
            },
        ]
    }

    fn on_response(&mut self, response: &sipx_sip::Response) -> Vec<Effect> {
        let status = response.status.code();
        // Provisionals do not conclude a step; the plan waits for a final.
        if response.status.is_provisional() {
            return Vec::new();
        }

        // Correlate. A response whose `CSeq` is not the outstanding request's belongs to an earlier
        // step — a duplicate, or a retransmission that overtook us — and consuming it here would
        // attribute one step's answer to another.
        if let Some(expected) = self.outstanding_cseq
            && cseq_number(response) != Some(expected)
        {
            return Vec::new();
        }

        match self.waiting {
            Waiting::Nothing => Vec::new(),
            Waiting::Cleanup(step) => {
                // §6 V6 — cleanup never changes a verdict already decided.
                if !response.status.is_success() {
                    self.state.cleanup_failed = true;
                }
                self.after_cleanup(step)
            }
            Waiting::Step(step) => self.on_step_response(step, status, response),
        }
    }

    fn on_step_response(
        &mut self,
        step: Step,
        status: u16,
        response: &sipx_sip::Response,
    ) -> Vec<Effect> {
        // A challenge the probe can answer is not a failure — it answers and retries the step once.
        // A challenge it cannot answer is its own credentials (§6 V5), and the distinction is
        // whether it had one to offer at all.
        if matches!(status, 401 | 407) {
            // §6 V5 — the distinction is whether a credential was offered at all.
            if !self.config.has_credentials {
                return self.abandon(ProbeFault::BadCredentials);
            }
            if self.state.answered_challenge {
                // Answered once and challenged again: that is the platform's behaviour, not the
                // probe's account.
                self.conclude_step(false);
                return self.fail(step, Cause::Rejected { status });
            }
            self.state.answered_challenge = true;
            // Re-send the same step's request; the driver attaches the credential. Not a retry in
            // S7's sense — a challenge is a protocol round trip, not a lost message.
            return self.retry_challenged(step);
        }

        // §8 B4 — shedding is the platform working as designed. It must not read as a dead listener.
        if status == 503
            && let Some(retry_after) = retry_after_of(response)
        {
            self.conclude_step(false);
            return self.fail(
                step,
                Cause::Shed {
                    retry_after: Some(retry_after),
                },
            );
        }

        if !response.status.is_success() {
            self.conclude_step(false);
            return self.fail(step, Cause::Rejected { status });
        }

        self.conclude_step(true);
        match step {
            Step::Register => {
                self.state.owe(Obligation::Deregister);
                self.send_invite()
            }
            Step::Invite => {
                self.state.owe(Obligation::EndDialog);
                // §4 M4 — answered, but by what? A `200` without our marker means something that is
                // not our echo picked up, which is a routing fault and reads differently from silence.
                if Marker::of(&Message::Response(response.clone())).as_ref() != Some(&self.marker) {
                    // The dialog exists, so S3 still owes it a BYE.
                    return self.fail(Step::Invite, Cause::MarkerMismatch);
                }
                self.send_bye()
            }
            Step::Bye => {
                self.state.forget(Obligation::EndDialog);
                self.decided = Some(Verdict::Pass);
                self.cleanup()
            }
            Step::Resolve => Vec::new(),
        }
    }

    /// Re-send the challenged request for a step, keeping its timer running.
    fn retry_challenged(&mut self, step: Step) -> Vec<Effect> {
        let request = match step {
            Step::Register => self.register_request(3_600),
            Step::Invite => self.invite_request(),
            Step::Bye => self.simple_request(&Method::Bye, &self.config.echo_aor.clone()),
            // Resolve sends nothing, so it cannot be challenged.
            Step::Resolve => return Vec::new(),
        };
        vec![Effect::Send(Box::new(request))]
    }

    fn send_invite(&mut self) -> Vec<Effect> {
        self.begin(Step::Invite);
        let request = self.invite_request();
        vec![
            Effect::ClearTimer(Step::Register),
            Effect::Send(Box::new(request)),
            Effect::SetTimer {
                step: Step::Invite,
                after: self.config.timeout(Step::Invite),
            },
        ]
    }

    fn send_bye(&mut self) -> Vec<Effect> {
        self.begin(Step::Bye);
        // S2 — the ACK precedes the BYE. Hanging up without acknowledging would leave the echo
        // retransmitting and would test the platform's cleanup rather than its call path.
        let ack = self.simple_request(&Method::Ack, &self.config.echo_aor.clone());
        let bye = self.simple_request(&Method::Bye, &self.config.echo_aor.clone());
        vec![
            Effect::ClearTimer(Step::Invite),
            Effect::Send(Box::new(ack)),
            Effect::Send(Box::new(bye)),
            Effect::SetTimer {
                step: Step::Bye,
                after: self.config.timeout(Step::Bye),
            },
        ]
    }

    fn on_timeout(&mut self, step: Step) -> Vec<Effect> {
        match self.waiting {
            Waiting::Step(waiting) if waiting == step => {
                self.conclude_step(false);
                self.fail(step, Cause::Timeout)
            }
            Waiting::Cleanup(waiting) if waiting == step => {
                self.state.cleanup_failed = true;
                self.after_cleanup(step)
            }
            // A late timer for a step that already concluded is stale, not an event.
            _ => Vec::new(),
        }
    }

    fn on_transport_error(&mut self) -> Vec<Effect> {
        match self.waiting {
            Waiting::Step(step) => {
                self.conclude_step(false);
                self.fail(step, Cause::Unreachable)
            }
            Waiting::Cleanup(step) => {
                self.state.cleanup_failed = true;
                self.after_cleanup(step)
            }
            Waiting::Nothing => Vec::new(),
        }
    }

    // ------------------------------------------------------------------ conclusion ------------

    /// Decide the verdict, then clean up (S3, S4).
    fn fail(&mut self, step: Step, cause: Cause) -> Vec<Effect> {
        if self.decided.is_none() {
            self.decided = Some(Verdict::Fail { step, cause });
        }
        // When the failing step *is* the BYE, the dialog teardown has already been attempted and
        // failed. Cleanup must not send a second one: S7 makes retries the transport's job, not the
        // probe's, and a probe that re-sent its own BYE would be measuring its own persistence.
        if step == Step::Bye {
            self.state.forget(Obligation::EndDialog);
        }
        let mut effects = vec![Effect::ClearTimer(step)];
        effects.extend(self.cleanup());
        effects
    }

    /// Abandon the run as the probe's own fault — no cleanup verdict change, and no SIP emitted if
    /// nothing was started.
    fn abandon(&mut self, fault: ProbeFault) -> Vec<Effect> {
        if self.decided.is_none() {
            self.decided = Some(Verdict::Inconclusive { fault });
        }
        if let Waiting::Step(step) = self.waiting {
            self.conclude_step(false);
            let mut effects = vec![Effect::ClearTimer(step)];
            effects.extend(self.cleanup());
            return effects;
        }
        self.cleanup()
    }

    /// S3/S4 — always attempt to clean up, whatever the verdict.
    ///
    /// A probe that abandons dialogs or accumulates bindings becomes the outage it was watching for.
    fn cleanup(&mut self) -> Vec<Effect> {
        if self.state.owes(Obligation::EndDialog) {
            self.state.forget(Obligation::EndDialog);
            self.waiting = Waiting::Cleanup(Step::Bye);
            let bye = self.simple_request(&Method::Bye, &self.config.echo_aor.clone());
            return vec![
                Effect::Send(Box::new(bye)),
                Effect::SetTimer {
                    step: Step::Bye,
                    after: self.config.timeout(Step::Bye),
                },
            ];
        }
        if self.state.owes(Obligation::Deregister) {
            self.state.forget(Obligation::Deregister);
            self.waiting = Waiting::Cleanup(Step::Register);
            let deregister = self.register_request(0);
            return vec![
                Effect::Send(Box::new(deregister)),
                Effect::SetTimer {
                    step: Step::Register,
                    after: self.config.timeout(Step::Register),
                },
            ];
        }
        self.finish()
    }

    fn after_cleanup(&mut self, step: Step) -> Vec<Effect> {
        self.waiting = Waiting::Nothing;
        let mut effects = vec![Effect::ClearTimer(step)];
        effects.extend(self.cleanup());
        effects
    }

    fn finish(&mut self) -> Vec<Effect> {
        self.state.finished = true;
        self.waiting = Waiting::Nothing;
        let verdict = self
            .decided
            .clone()
            // Reaching here undecided would be an engine bug, and inventing a `Pass` for it would be
            // the worst possible way to be wrong.
            .unwrap_or(Verdict::Inconclusive {
                fault: ProbeFault::Internal,
            });
        vec![Effect::Finish(Box::new(ProbeRun {
            marker: self.marker.clone(),
            target: self.config.target.clone(),
            steps: std::mem::take(&mut self.steps),
            verdict,
            cleanup_failed: self.state.cleanup_failed,
        }))]
    }

    // ------------------------------------------------------------------ bookkeeping -----------

    fn begin(&mut self, step: Step) {
        self.waiting = Waiting::Step(step);
        self.step_started = self.now;
    }

    fn conclude_step(&mut self, succeeded: bool) {
        if let Waiting::Step(step) = self.waiting {
            self.steps.push(StepOutcome {
                step,
                succeeded,
                elapsed: self.now.saturating_sub(self.step_started),
            });
        }
        self.waiting = Waiting::Nothing;
    }

    fn register_request(&mut self, expires: u32) -> Request {
        self.cseq += 1;
        self.outstanding_cseq = Some(self.cseq);
        let via = self.via();
        let target = format!("sip:{}", self.config.target.address);
        RequestBuilder::new(Method::Register, uri_or_placeholder(&target))
            .header(HeaderName::CallId, format!("probe-{}", self.marker))
            .and_then(|b| b.cseq(self.cseq, &Method::Register))
            .and_then(|b| {
                b.header(
                    HeaderName::From,
                    format!("<{}>;tag=probe", self.config.probe_aor),
                )
            })
            .and_then(|b| b.header(HeaderName::To, format!("<{}>", self.config.probe_aor)))
            .and_then(|b| {
                b.header(
                    HeaderName::Contact,
                    format!("<{}>;expires={expires}", self.config.contact),
                )
            })
            .and_then(|b| b.header(MARKER_HEADER, self.marker.header_bytes()))
            .and_then(|b| b.header(HeaderName::Via, via))
            .map(|b| b.max_forwards(70))
            .map_or_else(
                |_| RequestBuilder::new(Method::Register, uri_or_placeholder(&target)).build(),
                sipx_sip::RequestBuilder::build,
            )
    }

    fn invite_request(&mut self) -> Request {
        self.simple_request(&Method::Invite, &self.config.echo_aor.clone())
    }

    fn simple_request(&mut self, method: &Method, to: &str) -> Request {
        self.cseq += 1;
        // The ACK is not a request anything answers, so it must not become what the engine waits on.
        if *method != Method::Ack {
            self.outstanding_cseq = Some(self.cseq);
        }
        let via = self.via();
        RequestBuilder::new(method.clone(), uri_or_placeholder(to))
            .header(HeaderName::CallId, format!("probe-{}", self.marker))
            .and_then(|b| b.cseq(self.cseq, method))
            .and_then(|b| {
                b.header(
                    HeaderName::From,
                    format!("<{}>;tag=probe", self.config.probe_aor),
                )
            })
            .and_then(|b| b.header(HeaderName::To, format!("<{to}>")))
            .and_then(|b| b.header(MARKER_HEADER, self.marker.header_bytes()))
            .and_then(|b| b.header(HeaderName::Via, via))
            .map(|b| b.max_forwards(70))
            .map_or_else(
                |_| RequestBuilder::new(method.clone(), uri_or_placeholder(to)).build(),
                sipx_sip::RequestBuilder::build,
            )
    }
}

impl ProbeEngine {
    /// The `Via` for the request about to go out.
    ///
    /// **Every request needs one.** RFC 3261 §8.1.1.7 makes `Via` how a response finds its way back,
    /// and a UAC that omits it cannot receive one — the end-to-end scenario found exactly that: a
    /// compliant proxy refused the probe's INVITE as unanswerable and the probe reported a timeout it
    /// had caused itself.
    ///
    /// The branch is derived from the marker and the sequence number rather than drawn at random:
    /// unique per transaction, which is all RFC 3261 asks, and reproducible, which is what the
    /// harness asks.
    fn via(&self) -> String {
        format!(
            "SIP/2.0/{} {};branch=z9hG4bK-probe-{}-{}",
            self.config.target.transport, self.config.sent_by, self.marker, self.cseq
        )
    }
}

/// A URI, or a syntactically valid placeholder.
///
/// A configuration whose URIs do not parse is caught as `Misconfigured` before any request is built,
/// so this is the unreachable arm — and returning something parseable keeps it from becoming a panic
/// in a crate that forbids them.
fn uri_or_placeholder(text: &str) -> Uri {
    Uri::parse(Bytes::from(text.to_owned())).unwrap_or_else(|_| {
        Uri::parse(Bytes::from_static(b"sip:invalid.invalid"))
            .unwrap_or_else(|_| uri_or_placeholder("sip:invalid.invalid"))
    })
}

/// The `CSeq` number of a response.
fn cseq_number(response: &sipx_sip::Response) -> Option<u32> {
    let value = response.headers.value(&HeaderName::CSeq)?;
    String::from_utf8_lossy(&value)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// `Retry-After`, in seconds, if the response states one.
fn retry_after_of(response: &sipx_sip::Response) -> Option<Duration> {
    let value = response
        .headers
        .value(&HeaderName::Other(Bytes::from_static(b"Retry-After")))?;
    String::from_utf8_lossy(&value)
        .trim()
        .split(';')
        .next()?
        .trim()
        .parse()
        .ok()
        .map(Duration::from_secs)
}
