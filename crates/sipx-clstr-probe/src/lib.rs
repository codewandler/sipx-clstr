//! The outside view: can a real call be placed through this deployment, right now?
//!
//! Every other crate proves the platform from the inside. This one dials the border the way a
//! customer does — through DNS and the VIP, per edge and per transport — calls an echo endpoint
//! in a dedicated test tenant, and turns the result into a verdict: `pass`, `fail(step, cause)`,
//! or `inconclusive`. See
//! [`docs/designs/e2e-tester.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/e2e-tester.md).
//!
//! The engine is sans-IO for the same reason everything else here is: a probe's interesting
//! behaviour is what it does when the call *fails*, and those scenarios have to be seeded harness
//! tests before they are a cron job that pages someone at 03:00.
//!
//! Signalling echo only. A media assertion, if it ever comes, goes through the relay — never
//! through RTP in the process that parses SIP.
//!
//! # Status
//!
//! Skeleton. `ET-1` specifies the role and the verdict taxonomy; `ET-2` implements the engine and
//! `ET-3` the echo endpoint.

#![doc(html_no_source)]

pub mod echo;
pub mod engine;
pub mod marker;
pub mod schedule;
pub mod verdict;

pub use echo::{EchoConfig, EchoEndpoint, MediaPolicy};
pub use engine::{Effect, Input, ProbeConfig, ProbeEngine, ProbeRun, Target};
pub use marker::{MARKER_HEADER, MARKER_PREFIX, Marker};
pub use schedule::{Cadence, RateBound, Scheduler};
pub use verdict::{Cause, ProbeFault, Step, StepOutcome, Verdict};
