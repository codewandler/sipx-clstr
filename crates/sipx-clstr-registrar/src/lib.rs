//! REGISTER processing and the location service.
//!
//! The one place this platform is allowed durable state, which is why its update contract is the
//! strict part: every mutation of an address-of-record's binding set is a compare-and-swap on a
//! revision, so concurrent REGISTERs serialize on any backend rather than on one backend's
//! happens-to-work locking. See
//! [`docs/specs/location-service.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).
//!
//! Sans-IO like the proxy core: REGISTER processing is a function of (request, current binding
//! set, now) to (response, intended mutation). The store is a trait; the driver owns the socket
//! and the database handle.
//!
//! # Backends
//!
//! In-memory (`RG-3`) and `PostgreSQL` (`RG-4`, behind the `postgres` feature). The spec's
//! vector tables run against **both, unchanged** — a backend that needs its own version of a
//! vector has broken the contract rather than implemented it.
//!
//! # Status
//!
//! Skeleton. `RG-3` fills in canonicalization, the binding schema and REGISTER processing.

#![doc(html_no_source)]

pub mod aor;
pub mod auth;
pub mod binding;
pub mod command;
#[cfg(feature = "test-suite")]
pub mod conformance;
pub mod lookup;
pub mod parse;
pub mod process;
pub mod store;

pub use aor::{AorError, CanonicalAor, shard_key};
pub use auth::{
    Algorithm, AuthOutcome, ChallengeResponse, CredentialStore, Decision, InMemoryCredentials,
    TenantAuth,
};
pub use binding::{Binding, BindingSet, Revision, SourceAddr, Timestamp};
pub use command::{
    Accepted, ContactOp, ContactOps, ContactValue, Outcome, RegisterCommand, Rejection,
    TenantPolicy,
};
pub use lookup::{Target, order_targets};
pub use parse::{Admission, EdgeContext, admit, admit_audited, register_command};
pub use process::process;
pub use store::{Applied, CasConflict, Change, InMemoryStore, LocationStore, ReadFailure, apply};
