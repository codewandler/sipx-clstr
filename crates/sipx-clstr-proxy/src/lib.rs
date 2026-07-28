//! Sans-IO proxy forwarding.
//!
//! RFC 3261 §16 — amended by RFC 5393 for loop detection and RFC 6026 for the late 2xx — as a
//! state machine: requests, responses, fired timers and verified-token facts go in; effects come
//! out. No sockets, no clock, no location service. Those live in the driver
//! (`sipx-clstr-node`), which is what makes every rule in
//! [`docs/specs/proxy-behavior.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! reachable from a test that neither sleeps nor opens a port.
//!
//! The kernel underneath supplies lossless messages and all four transaction machines and
//! contains **no forwarding path** — no `Via` push/pop, no `Max-Forwards` decrement, no
//! Record-Route insertion, no forking, no response aggregation, no CANCEL propagation. That is
//! this crate.
//!
//! # Scope in M1
//!
//! Transaction-stateful forwarding only (§16.2). Stateless mode (§16.11) is specified but
//! deliberately unimplemented until mid-dialog requests carry affinity tokens and give it a
//! consumer — see the roadmap's M1 scope table.
//!
//! # Status
//!
//! Skeleton. `PX-2` designs the driver seam and `PX-5`/`PX-6` fill this in.

#![doc(html_no_source)]

pub mod config;
pub mod context;
pub mod cookie;
pub mod forward;
#[cfg(feature = "registrar-targets")]
pub mod from_registrar;
pub mod types;
pub mod validate;

pub use config::{CookieKey, EdgeIdentity, ProxyConfig};
pub use context::ResponseContext;
pub use cookie::{CookieInput, branch_for, cookie_of};
pub use forward::{ForwardError, ForwardPlan, TOKEN_PARAM, TOKEN_PARAM_BUDGET, forward};
#[cfg(feature = "registrar-targets")]
pub use from_registrar::targets_from_lookup;
pub use types::{BranchId, Effect, Input, Kind, ProxyTimer, Target, TargetQuery, TokenVerdict};
pub use validate::Refusal as ValidationRefusal;
pub use validate::{Refusal, Validated, validate};
