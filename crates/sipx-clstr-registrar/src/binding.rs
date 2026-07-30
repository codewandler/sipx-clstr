//! What is stored: bindings, the set they form, and the revision that fences it.
//!
//! Contact and Path values are kept as **verbatim bytes**. What a UA registered is what the proxy
//! later forwards ([proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! §7 F2), so re-serializing a parsed URI here would quietly change what goes on the wire.
//!
//! Specification: [location-service §4](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).

use std::fmt;
use std::time::Duration;

use bytes::Bytes;

/// A point in time, as nanoseconds on some monotonic origin.
///
/// Time is an *input* to every decision in this crate — never read from a clock — so it has to be
/// a value the deterministic harness can mint at will. `std::time::Instant` cannot be constructed
/// from a number, which makes it exactly the wrong type here.
///
/// It lives in this crate for now because this crate is the only one that needs it. When the proxy
/// engine needs one too, it moves somewhere shared rather than being defined twice.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The origin.
    pub const ZERO: Self = Self(0);

    /// A timestamp this many nanoseconds after the origin.
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// A timestamp this many seconds after the origin.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs.saturating_mul(1_000_000_000))
    }

    /// Nanoseconds since the origin.
    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Whole seconds since the origin, truncated.
    ///
    /// The digest primitives count in seconds ([`crate::auth`], and `S-16` upstream), the location
    /// service in nanoseconds, and [`crate::parse::admit`] is where the two meet. Truncating rather
    /// than rounding is the safe direction: a nonce judged against a `now` that is at most one
    /// second early expires a moment late, never a moment early, so no client is told `stale` for a
    /// nonce that was still good.
    #[must_use]
    pub const fn as_secs(self) -> u64 {
        self.0 / 1_000_000_000
    }

    /// This instant plus a duration, saturating.
    #[must_use]
    pub fn saturating_add(self, after: Duration) -> Self {
        Self(
            self.0
                .saturating_add(u64::try_from(after.as_nanos()).unwrap_or(u64::MAX)),
        )
    }

    /// How long from here to `later`; zero if `later` is earlier.
    #[must_use]
    pub fn until(self, later: Self) -> Duration {
        Duration::from_nanos(later.0.saturating_sub(self.0))
    }
}

/// The observed source of a registration (RFC 3261 §18.2.1, RFC 3581).
///
/// Diagnostics and NAT evidence. **Not** a routing input in M1 — the proxy routes to the
/// registered contact and its Path set, and treating an observed address as a route without the
/// RFC 5626 machinery to back it is how a registrar starts guessing.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAddr {
    /// How it arrived, as a transport token (`UDP`, `TCP`, `TLS`, `WS`, `WSS`).
    pub transport: String,
    /// The address it came from.
    pub ip: std::net::IpAddr,
    /// The port it came from.
    pub port: u16,
}

/// RFC 8599 push parameters, projected from the verbatim contact.
///
/// Stored from M1 and used in M3, so activating push is a semantics change rather than a migration.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    /// `pn-provider`.
    pub provider: Bytes,
    /// `pn-prid`.
    pub prid: Bytes,
    /// `pn-param`.
    pub param: Option<Bytes>,
}

/// One row of an address-of-record's set.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The registered Contact URI, verbatim.
    pub contact: Bytes,
    /// Preference in thousandths, 0–1000. Absent `;q` becomes 1000 (§7 L2).
    pub q: u16,
    /// The `Call-ID` that last wrote this binding, byte-exact and case-sensitive.
    pub call_id: Bytes,
    /// The `CSeq` that last wrote it.
    pub cseq: u32,
    /// When it stops existing for every purpose.
    pub expires_at: Timestamp,
    /// When it was first created — the deterministic tie-break for §5.3's first-match rule.
    pub registered_at: Timestamp,
    /// When it was last successfully updated — the §7 L3 ordering tie-break.
    pub refreshed_at: Timestamp,
    /// The stored Path vector, topmost first, exactly as received (RFC 3327 §5.2).
    pub path: Vec<Bytes>,
    /// The observed source.
    pub received: Option<SourceAddr>,
    /// `+sip.instance` (RFC 5626). Stored in M1, becomes binding identity in M3.
    pub instance_id: Option<Bytes>,
    /// `reg-id` (RFC 5626). M3.
    pub reg_id: Option<u32>,
    /// Opaque flow reference from the accepting edge; never a socket (AF-2 owns the format).
    pub flow_ref: Option<Bytes>,
    /// RFC 8599 push parameters. M3.
    pub push: Option<Push>,
    /// Who created or refreshed it (RG-2), for audit and authorization.
    pub principal: Option<Bytes>,
}

impl Binding {
    /// Whether this binding still exists at `now`.
    ///
    /// An expired binding is **absent for every purpose** — lookup, contact matching, and
    /// Call-ID/CSeq comparison (§5.3). Once a binding is gone the RFC's model has nothing left to
    /// compare against, which is why a late REGISTER with an old `CSeq` adds a fresh one.
    #[must_use]
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.expires_at > now
    }

    /// Remaining granted lifetime at `now`.
    #[must_use]
    pub fn expires_in(&self, now: Timestamp) -> Duration {
        now.until(self.expires_at)
    }
}

/// A monotonic per-AoR revision.
///
/// Never reset — including across periods where the set is empty (§6 K3). A consumer holding
/// revision *n* discards anything labelled below it, and RG-5's shard handoff fences on the same
/// counter, so resetting it on an empty set would silently un-fence both.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Revision(pub u64);

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{}", self.0)
    }
}

impl Revision {
    /// The revision an AoR that has never been written carries.
    pub const INITIAL: Self = Self(0);

    /// The next revision.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Every binding for one address-of-record, active or not.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingSet {
    /// In creation order: `registered_at`, then contact bytes. §5.3's first-match rule depends on
    /// that order being stable, so it is maintained on insert rather than sorted on read.
    bindings: Vec<Binding>,
}

impl BindingSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every binding, including expired ones.
    #[must_use]
    pub fn all(&self) -> &[Binding] {
        &self.bindings
    }

    /// The bindings that still exist at `now`.
    pub fn active(&self, now: Timestamp) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().filter(move |b| b.is_active(now))
    }

    /// How many bindings are active at `now` — what the quota (§5.5) counts.
    #[must_use]
    pub fn active_count(&self, now: Timestamp) -> usize {
        self.active(now).count()
    }

    /// Add a binding, keeping creation order.
    pub fn insert(&mut self, binding: Binding) {
        let _ = self.insert_at(binding);
    }

    /// Add a binding, keeping creation order, and report the index it landed at.
    ///
    /// Creation order means an insert is not an append, so a caller holding anything indexed
    /// alongside `all()` cannot know where the set moved without being told. REGISTER
    /// reconciliation holds exactly that — the parsed contact view it matches against
    /// ([location-service §5.3.2](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)
    /// B6) — and keeping the two in step is what makes a multi-contact request commit the set it
    /// describes.
    pub fn insert_at(&mut self, binding: Binding) -> usize {
        let position = self.bindings.partition_point(|existing| {
            (existing.registered_at, &existing.contact) <= (binding.registered_at, &binding.contact)
        });
        self.bindings.insert(position, binding);
        position
    }

    /// Replace the binding at `index`, if it exists.
    pub fn replace(&mut self, index: usize, binding: Binding) {
        if let Some(slot) = self.bindings.get_mut(index) {
            *slot = binding;
        }
    }

    /// Drop the binding at `index`, if it exists.
    pub fn remove(&mut self, index: usize) {
        if index < self.bindings.len() {
            self.bindings.remove(index);
        }
    }

    /// Forget every binding that expired at or before `now`.
    ///
    /// Housekeeping, not semantics: `is_active` already makes them invisible. This keeps a set
    /// that is refreshed forever from growing forever.
    pub fn drop_expired(&mut self, now: Timestamp) {
        self.bindings.retain(|binding| binding.is_active(now));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn binding(contact: &'static str, registered: u64, expires: u64) -> Binding {
        Binding {
            contact: Bytes::from_static(contact.as_bytes()),
            q: 1000,
            call_id: Bytes::from_static(b"i1"),
            cseq: 1,
            expires_at: Timestamp::from_secs(expires),
            registered_at: Timestamp::from_secs(registered),
            refreshed_at: Timestamp::from_secs(registered),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }

    #[test]
    fn a_binding_expires_at_its_deadline_not_after_it() {
        let b = binding("sip:a@h", 0, 10);
        assert!(b.is_active(Timestamp::from_secs(9)));
        // `expires_at <= now` is absent (§5.3 L1): at the deadline it is already gone, so a
        // binding and a lookup at the same instant cannot disagree about whether it exists.
        assert!(!b.is_active(Timestamp::from_secs(10)));
        assert!(!b.is_active(Timestamp::from_secs(11)));
    }

    #[test]
    fn creation_order_is_stable_and_breaks_ties_by_contact_bytes() {
        let mut set = BindingSet::new();
        set.insert(binding("sip:c@h", 5, 100));
        set.insert(binding("sip:a@h", 5, 100));
        set.insert(binding("sip:b@h", 1, 100));
        let order: Vec<&[u8]> = set.all().iter().map(|b| b.contact.as_ref()).collect();
        assert_eq!(order, [&b"sip:b@h"[..], &b"sip:a@h"[..], &b"sip:c@h"[..]]);
    }

    #[test]
    fn the_quota_counts_only_what_still_exists() {
        let mut set = BindingSet::new();
        set.insert(binding("sip:a@h", 0, 5));
        set.insert(binding("sip:b@h", 0, 100));
        assert_eq!(set.active_count(Timestamp::from_secs(10)), 1);
    }

    #[test]
    fn a_revision_never_goes_backwards_even_across_an_empty_set() {
        // K3: RG-5's handoff fences on this counter, so resetting it when the set empties would
        // un-fence the handoff as well as the caches.
        let revision = Revision::INITIAL.next().next();
        assert_eq!(revision, Revision(2));
        assert_eq!(revision.next(), Revision(3));
    }
}
