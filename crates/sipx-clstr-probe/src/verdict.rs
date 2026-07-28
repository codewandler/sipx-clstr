//! Verdicts — [e2e-probe §6](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md).
//!
//! Three values, and the third is load-bearing: a probe that could not conduct a valid test is not an
//! outage, and conflating the two trains operators to ignore the alert. An ignored alert is worse than
//! no alert.

use std::fmt;
use std::time::Duration;

/// One step of the probe plan (§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// P1 — resolve the target.
    Resolve,
    /// P2 — REGISTER.
    Register,
    /// P3 — INVITE, and the marker reflected.
    Invite,
    /// P4 — BYE.
    Bye,
}

impl Step {
    /// The plan, in order.
    pub const PLAN: [Self; 4] = [Self::Resolve, Self::Register, Self::Invite, Self::Bye];

    /// The default timeout for this step (§3).
    #[must_use]
    pub fn default_timeout(self) -> Duration {
        match self {
            Self::Resolve => Duration::from_secs(2),
            Self::Register | Self::Bye => Duration::from_secs(5),
            Self::Invite => Duration::from_secs(10),
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Resolve => "resolve",
            Self::Register => "register",
            Self::Invite => "invite",
            Self::Bye => "bye",
        };
        f.write_str(name)
    }
}

/// Why a step the **platform** is responsible for did not succeed (§6).
///
/// A closed list. A condition fitting none of these is [`ProbeFault::Internal`], because a probe that
/// invents a platform failure it cannot substantiate is worse than one that admits confusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    /// The step's timeout elapsed with no final response.
    Timeout,
    /// A non-2xx final response. The status is carried because `403` and `503` are different
    /// incidents.
    Rejected {
        /// The status received.
        status: u16,
    },
    /// Answered, but not by our echo — no marker, or another run's (§4 M4).
    ///
    /// Deliberately not a network failure: the call was answered by *something*, which is a routing
    /// fault and reads differently from silence.
    MarkerMismatch,
    /// Transport-level failure: refused, handshake failure, no route.
    Unreachable,
    /// The target would not resolve. For a DNS-name target this *is* a platform fault — that record
    /// is part of the service.
    ResolutionFailed,
    /// Overload control shed the probe (§8 B4).
    ///
    /// Its own cause because shedding is the platform working as designed under load, and it must not
    /// read as a broken listener.
    Shed {
        /// The `Retry-After` the platform asked for, if it stated one.
        retry_after: Option<Duration>,
    },
}

/// The probe's **own** fault (§6). Never reported as a platform failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeFault {
    /// The probe's credentials were rejected and it had none to offer.
    BadCredentials,
    /// The probe's configuration is unusable.
    Misconfigured,
    /// No local socket or port.
    NoLocalResource,
    /// Local time moved in a way that invalidates the measurement.
    ClockSkew,
    /// Cancelled — shutdown, or superseded.
    Cancelled,
    /// A probe-side error with no better name (§6 V4).
    Internal,
}

/// What a run concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every step succeeded and the marker came back.
    Pass,
    /// A step the platform owns failed.
    Fail {
        /// Which step.
        step: Step,
        /// Why.
        cause: Cause,
    },
    /// The probe could not conduct a valid test.
    Inconclusive {
        /// What went wrong on the probe's side.
        fault: ProbeFault,
    },
}

impl Verdict {
    /// Whether this run counts toward the platform's success ratio.
    ///
    /// `Inconclusive` does not (§6 V3): a broken prober is not an outage, and counting it as one is
    /// how a success-ratio alert becomes noise nobody reads.
    #[must_use]
    pub fn counts_as_platform_result(&self) -> bool {
        !matches!(self, Self::Inconclusive { .. })
    }

    /// Whether the platform is being reported as broken.
    #[must_use]
    pub fn is_platform_failure(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("pass"),
            Self::Fail { step, cause } => write!(f, "fail({step}, {cause:?})"),
            Self::Inconclusive { fault } => write!(f, "inconclusive({fault:?})"),
        }
    }
}

/// What one step did, and how long it took.
///
/// Recorded for **every** attempted step including the failed one: the latency of a failure is
/// diagnostic, and a verdict without the steps behind it tells an operator that something is wrong
/// and nothing about where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    /// Which step.
    pub step: Step,
    /// Whether it succeeded.
    pub succeeded: bool,
    /// How long it took.
    pub elapsed: Duration,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_is_in_order_with_the_specs_timeouts() {
        assert_eq!(
            Step::PLAN,
            [Step::Resolve, Step::Register, Step::Invite, Step::Bye]
        );
        assert_eq!(Step::Resolve.default_timeout(), Duration::from_secs(2));
        assert_eq!(Step::Register.default_timeout(), Duration::from_secs(5));
        assert_eq!(Step::Invite.default_timeout(), Duration::from_secs(10));
        assert_eq!(Step::Bye.default_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn an_inconclusive_run_is_not_a_platform_result() {
        // V3, the rule the whole three-valued taxonomy exists for.
        let broken = Verdict::Inconclusive {
            fault: ProbeFault::BadCredentials,
        };
        assert!(!broken.counts_as_platform_result());
        assert!(!broken.is_platform_failure());

        let outage = Verdict::Fail {
            step: Step::Register,
            cause: Cause::Timeout,
        };
        assert!(outage.counts_as_platform_result());
        assert!(outage.is_platform_failure());

        assert!(Verdict::Pass.counts_as_platform_result());
        assert!(!Verdict::Pass.is_platform_failure());
    }

    #[test]
    fn a_verdict_says_which_step_and_why() {
        let verdict = Verdict::Fail {
            step: Step::Invite,
            cause: Cause::Rejected { status: 480 },
        };
        let rendered = verdict.to_string();
        assert!(rendered.contains("invite"), "{rendered}");
        assert!(rendered.contains("480"), "{rendered}");
    }
}
