//! The REGISTER command, the tenant policy it is judged against, and the outcome.
//!
//! Specification: [location-service §2, §5](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).

use std::time::Duration;

use bytes::Bytes;
use sipx_sip::Uri;

use crate::aor::CanonicalAor;
use crate::binding::{Binding, BindingSet, Push, SourceAddr, Timestamp};

/// One `Contact` value a REGISTER asks about.
#[derive(Debug, Clone)]
pub struct ContactOp {
    /// The contact URI as parsed, for §19.1.4 matching.
    pub uri: Uri,
    /// The contact URI's verbatim bytes, which is what gets stored and later forwarded.
    pub verbatim: Bytes,
    /// The `;expires` parameter, if the contact carried one (E1).
    pub expires: Option<u32>,
    /// The `;q` parameter in thousandths, if present (§7 L2).
    pub q: Option<u16>,
    /// `+sip.instance` (RFC 5626), stored in M1.
    pub instance_id: Option<Bytes>,
    /// `reg-id` (RFC 5626), stored in M1.
    pub reg_id: Option<u32>,
    /// RFC 8599 push parameters, stored in M1.
    pub push: Option<Push>,
}

/// What a REGISTER asks to be done to the binding set.
#[derive(Debug, Clone)]
pub enum ContactOps {
    /// Explicit `Contact` values. An empty list is a *query* — RFC 3261 §10.2.3 — and mutates
    /// nothing while still answering with the complete set.
    Explicit(Vec<ContactOp>),
    /// `Contact: *`, which is only valid with `Expires: 0` (§5.4 W2).
    Wildcard,
}

/// A validated REGISTER, ready to be decided on.
#[derive(Debug, Clone)]
pub struct RegisterCommand {
    /// From the edge's trust context, **never** from the URI.
    pub tenant: String,
    /// The canonical key (§3).
    pub aor: CanonicalAor,
    /// Byte-exact and case-sensitive: it is half of the ordering token.
    pub call_id: Bytes,
    /// The other half.
    pub cseq: u32,
    /// What to do.
    pub contacts: ContactOps,
    /// The `Expires` header, if present (E2).
    pub expires_header: Option<u32>,
    /// Verbatim `Path` URIs, topmost first (RFC 3327 §5.2).
    pub path: Vec<Bytes>,
    /// Whether the UAC advertised `Supported: path` (§5.6).
    pub supports_path: bool,
    /// `Require` option tags the registrar must understand (S2).
    pub require: Vec<String>,
    /// The observed source.
    pub received: Option<SourceAddr>,
    /// Opaque flow reference from the accepting edge (AF-2 owns the format).
    pub flow_ref: Option<Bytes>,
    /// The authenticated identity (RG-2), already authorized.
    pub principal: Option<Bytes>,
    /// Time, as an input.
    pub now: Timestamp,
}

/// Per-tenant policy: the numbers §5 leaves configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantPolicy {
    /// Granted when no expiry is stated anywhere (E3).
    pub default_expires: u32,
    /// The shortest registration this tenant accepts (E6). Below it, the whole request is `423`.
    pub min_expires: u32,
    /// The longest. A request above it is silently lowered, not refused (E5).
    pub max_expires: u32,
    /// How many active bindings one AoR may hold (§5.5).
    pub max_bindings_per_aor: usize,
    /// How many contact operations one REGISTER may carry (§5.5.1 Q1).
    ///
    /// A different quantity from `max_bindings_per_aor`, and neither substitutes for the other: the
    /// quota bounds the committed *outcome*, this bounds the *request*. A REGISTER made entirely of
    /// refreshes and removals never grows the set, so the quota cannot refuse it however long it is —
    /// and reconciliation costs `operations × bindings`, so its length is what has to be capped.
    ///
    /// §5.5.1's policy-consistency rule: keep this at least `max_bindings_per_aor`, or a tenant
    /// cannot refresh its whole binding set in one request.
    pub max_contact_ops: usize,
}

impl Default for TenantPolicy {
    fn default() -> Self {
        Self {
            default_expires: 3_600,
            min_expires: 60,
            max_expires: 86_400,
            max_bindings_per_aor: 10,
            max_contact_ops: 64,
        }
    }
}

/// One `Contact` value of a `200 OK`, describing a binding that currently exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactValue {
    /// The stored contact, verbatim.
    pub contact: Bytes,
    /// Remaining granted lifetime, in seconds, computed against the command's `now`.
    pub expires: u32,
    /// The stored preference in thousandths.
    pub q: u16,
}

/// The `200 OK` body of facts: the **complete** active set, and the stored Path.
///
/// The complete-set rule is unconditional (§5.6): a REGISTER that removed one of three bindings
/// answers with the two survivors, and a valid wildcard removal answers with none.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Accepted {
    /// Every active binding, in stored order.
    pub contacts: Vec<ContactValue>,
    /// The stored Path vector, echoed unmodified and in order.
    pub path: Vec<Bytes>,
}

/// Why a REGISTER was refused. Each variant fixes the status the spec assigns it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// S1/S5 — the domain or the AoR is not served here.
    ///
    /// The registrar role never silently proxies an unserved domain.
    #[error("404 the domain or address-of-record is not served here")]
    NotFound,
    /// S5/S6 — malformed, including every §3 canonicalization rejection and both wildcard rules.
    #[error("400 bad request: {0}")]
    BadRequest(&'static str),
    /// S4/S8 — a policy refusal. A retry cannot succeed without changing something.
    #[error("403 forbidden: {0}")]
    Forbidden(&'static str),
    /// S2 — `Require` named something this registrar does not implement.
    #[error("420 bad extension: {0:?}")]
    BadExtension(Vec<String>),
    /// E6 — the whole request fails, and the response must state the minimum.
    #[error("423 interval too brief, minimum {min}")]
    IntervalTooBrief {
        /// The tenant minimum, for `Min-Expires`.
        min: u32,
    },
    /// §5.6 — `Path` arrived from a UAC that did not advertise support for it.
    #[error("421 extension required: {0}")]
    ExtensionRequired(&'static str),
    /// B5/W3 — the ordering token went backwards, so the update is aborted.
    ///
    /// The RFC names no code. `500` rather than `400`: the request is not malformed, and a retry
    /// with a fresh `CSeq` succeeds, so `400` would send diagnostics the wrong way.
    #[error("500 the CSeq is not newer than the stored binding's")]
    StaleSequence,
    /// S10 — the store is unreachable, or CAS retries are exhausted.
    #[error("503 the location store is unavailable")]
    Unavailable,
}

impl Rejection {
    /// The status code to answer with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::BadRequest(_) => 400,
            Self::Forbidden(_) => 403,
            Self::BadExtension(_) => 420,
            Self::IntervalTooBrief { .. } => 423,
            Self::ExtensionRequired(_) => 421,
            Self::StaleSequence => 500,
            Self::Unavailable => 503,
        }
    }
}

/// What processing decided.
///
/// The spec writes this as `Commit | Noop`, with refusals carried inside a `Noop`'s response.
/// Split here so that "did this write?" is a question the type answers: `Reject` is a `Noop` whose
/// response is not a `200`, and neither commits anything.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The set changed. The driver must commit it under the revision it read.
    Commit {
        /// The new set.
        set: BindingSet,
        /// The response to send once the commit succeeds.
        response: Accepted,
    },
    /// Nothing changed, and that is the correct answer: an idempotent retry, or a query.
    Noop {
        /// The response to send.
        response: Accepted,
    },
    /// Refused. Nothing committed.
    Reject(Rejection),
}

impl Outcome {
    /// The status this outcome answers with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Commit { .. } | Self::Noop { .. } => 200,
            Self::Reject(rejection) => rejection.status(),
        }
    }

    /// Whether the store must be written.
    #[must_use]
    pub fn commits(&self) -> bool {
        matches!(self, Self::Commit { .. })
    }

    /// The accepted facts, if this outcome succeeded.
    #[must_use]
    pub fn accepted(&self) -> Option<&Accepted> {
        match self {
            Self::Commit { response, .. } | Self::Noop { response } => Some(response),
            Self::Reject(_) => None,
        }
    }
}

/// Build the `200 OK` facts from a set: every active binding, with what it was granted.
pub(crate) fn describe(set: &BindingSet, now: Timestamp) -> Accepted {
    let contacts = set
        .active(now)
        .map(|binding| ContactValue {
            contact: binding.contact.clone(),
            expires: seconds_of(binding.expires_in(now)),
            q: binding.q,
        })
        .collect();
    // Every binding of one AoR carries the same Path — it is a property of the registration route,
    // not of the individual contact — so the first active binding's is the set's.
    let path = set
        .active(now)
        .next()
        .map(|binding| binding.path.clone())
        .unwrap_or_default();
    Accepted { contacts, path }
}

/// Whole seconds, rounded up, so a binding never reports zero while it still exists.
pub(crate) fn seconds_of(duration: Duration) -> u32 {
    let secs = duration.as_secs();
    let rounded = if duration.subsec_nanos() > 0 {
        secs.saturating_add(1)
    } else {
        secs
    };
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

/// The binding a contact operation would produce.
pub(crate) fn binding_for(
    op: &ContactOp,
    cmd: &RegisterCommand,
    granted: u32,
    registered_at: Timestamp,
) -> Binding {
    Binding {
        contact: op.verbatim.clone(),
        q: op.q.unwrap_or(1_000),
        call_id: cmd.call_id.clone(),
        cseq: cmd.cseq,
        expires_at: cmd
            .now
            .saturating_add(Duration::from_secs(u64::from(granted))),
        registered_at,
        refreshed_at: cmd.now,
        path: cmd.path.clone(),
        received: cmd.received.clone(),
        instance_id: op.instance_id.clone(),
        reg_id: op.reg_id,
        flow_ref: cmd.flow_ref.clone(),
        push: op.push.clone(),
        principal: cmd.principal.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_rejection_carries_the_status_the_spec_assigns_it() {
        assert_eq!(Rejection::NotFound.status(), 404);
        assert_eq!(Rejection::BadRequest("x").status(), 400);
        assert_eq!(Rejection::Forbidden("x").status(), 403);
        assert_eq!(Rejection::BadExtension(vec![]).status(), 420);
        assert_eq!(Rejection::IntervalTooBrief { min: 300 }.status(), 423);
        assert_eq!(Rejection::ExtensionRequired("path").status(), 421);
        assert_eq!(Rejection::StaleSequence.status(), 500);
        assert_eq!(Rejection::Unavailable.status(), 503);
    }

    #[test]
    fn a_partial_second_still_reports_a_lifetime() {
        // Rounding down would let a binding that exists report `expires=0`, which tells a UA to
        // stop refreshing something that is still there.
        assert_eq!(seconds_of(Duration::from_millis(1)), 1);
        assert_eq!(seconds_of(Duration::from_millis(1_001)), 2);
        assert_eq!(seconds_of(Duration::ZERO), 0);
    }
}
