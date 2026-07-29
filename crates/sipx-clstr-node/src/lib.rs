//! The node: drivers, roles, and the process that runs them.
//!
//! One binary, roles by config. Everything with a socket, a clock or a database handle in it
//! lives here; everything that decides anything lives in the sans-IO crates below. The boundary
//! is not stylistic — it is what lets `sipx-clstr-sim` run the same decision logic under virtual
//! time with no network at all.
//!
//! # Status
//!
//! Skeleton. The proxy driver — one server transaction fanning out to N client transactions over
//! the kernel's [`TransactionLayer`](sipx_sip::TransactionLayer) — is designed in `PX-2` and
//! implemented in `PX-5`.
//!
//! The configuration surface here is **provisional and deliberately minimal**: listeners, the
//! registrar realm, the location-store URL. `DP-1` owns the real schema and will replace this
//! rather than extend it, so nothing should grow to depend on its shape.

#![doc(html_no_source)]

pub mod driver;

/// The `PostgreSQL` location store (`RG-4`), behind the `postgres` feature.
#[cfg(feature = "postgres")]
pub mod postgres_store;

/// The sipx kernel release this node is built against.
///
/// Reported by `sipx-clstr --version` because "which kernel version is this behaviour true of?"
/// is a question an operator asks during an incident, and reading it off a running process beats
/// inferring it from a lockfile they do not have.
pub const KERNEL_VERSION: &str = "0.4.0";

/// This node's own version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
