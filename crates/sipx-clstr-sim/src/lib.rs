//! The deterministic cluster harness.
//!
//! A seeded, virtual-time, multi-node simulation. Every cluster behaviour this platform claims
//! must reproduce here before it touches a socket, and a failure is a **seed you replay**, not a
//! flake you re-run. See
//! [`docs/designs/conformance-harness.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/conformance-harness.md).
//!
//! The clock is the scheduler's loop variable, never a trait the layers below can see: sans-IO
//! code takes fired timers by contract, and a `Clock` it could read would be a way to write code
//! this harness cannot test.
//!
//! # Status
//!
//! Skeleton. `CF-5` implements the scheduler, the simulated network and the scenario runner;
//! `CF-4` adds fault schedules. The timer queue and the seeded loopback link are asked of the
//! kernel in sipx `X-14` — until that lands, this crate carries its own and swaps it out later.

#![doc(html_no_source)]

pub mod fault;
pub mod net;
pub mod node;
pub mod queue;
pub mod rng;
pub mod sim;
pub mod time;
pub mod trace;
pub mod viz;

pub use fault::{Fault, Schedule};
pub use net::{Latency, LinkKind, LinkPolicy, NodeId};
pub use node::{Effect, Input, SimNode, TimerId};
pub use sim::{Sim, SimError};
pub use time::SimTime;
pub use trace::Trace;
