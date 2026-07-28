//! The lookup contract: an address-of-record to a forking-ordered target set.
//!
//! Shaped for the proxy's `TargetsResolved(Vec<Target>)` input
//! ([proxy-behavior](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/proxy-behavior.md)
//! §2), which `RG-6` wires up.
//!
//! The order is a **pure function of the set and `now`**. Every node computes the identical order,
//! which is what lets the deterministic harness assert it — and what stops two edges forking the
//! same call in two different orders.
//!
//! Specification: [location-service §7](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md).

use std::time::Duration;

use bytes::Bytes;

use crate::binding::{BindingSet, Timestamp};

/// One place to try, and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The registered Contact URI, verbatim — parameters and all, per §7 F2.
    pub contact: Bytes,
    /// The stored Path vector in stored order, topmost first: the route toward the UA.
    pub route_set: Vec<Bytes>,
    /// Opaque flow reference, present when the binding rides a client-initiated connection.
    pub flow_ref: Option<Bytes>,
    /// Preference in thousandths, 0–1000.
    pub q: u16,
    /// Remaining lifetime at the `now` the lookup was given.
    pub expires_in: Duration,
}

/// Order a binding set into the target list the proxy forks on.
///
/// L1: expired bindings are excluded, against the caller's `now` — never a read of a clock.
/// L2: descending `q`, and a contact without `q` sorts as 1000, because an unstated preference is
/// full preference. Sorting it last would quietly demote every plain registration.
/// L3: ties break by more recent `refreshed_at` first — recency being the best reachability signal
/// available — and then by ascending contact bytes, which is a total order and therefore leaves no
/// pair unordered.
#[must_use]
pub fn order_targets(set: &BindingSet, now: Timestamp) -> Vec<Target> {
    let mut active: Vec<&crate::binding::Binding> = set.active(now).collect();

    active.sort_by(|a, b| {
        b.q.cmp(&a.q)
            .then_with(|| b.refreshed_at.cmp(&a.refreshed_at))
            .then_with(|| a.contact.cmp(&b.contact))
    });

    active
        .into_iter()
        .map(|binding| Target {
            contact: binding.contact.clone(),
            route_set: binding.path.clone(),
            flow_ref: binding.flow_ref.clone(),
            q: binding.q,
            expires_in: binding.expires_in(now),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::binding::Binding;

    fn binding(contact: &'static str, q: u16, refreshed: u64, expires: u64) -> Binding {
        Binding {
            contact: Bytes::from_static(contact.as_bytes()),
            q,
            call_id: Bytes::from_static(b"i1"),
            cseq: 1,
            expires_at: Timestamp::from_secs(expires),
            registered_at: Timestamp::from_secs(0),
            refreshed_at: Timestamp::from_secs(refreshed),
            path: Vec::new(),
            received: None,
            instance_id: None,
            reg_id: None,
            flow_ref: None,
            push: None,
            principal: None,
        }
    }

    /// The LS-L fixture: A (q 1000, refreshed 100, Path [P2, P1]), B (q 1000, refreshed 200,
    /// `flow_ref`), C (q 500), D (expired), E (no q — so 1000, refreshed 50).
    fn fixture() -> BindingSet {
        let mut set = BindingSet::new();

        let mut a = binding("sip:a@h", 1_000, 100, 1_000);
        a.path = vec![
            Bytes::from_static(b"<sip:p2>"),
            Bytes::from_static(b"<sip:p1>"),
        ];
        set.insert(a);

        let mut b = binding("sip:b@h", 1_000, 200, 1_000);
        b.flow_ref = Some(Bytes::from_static(b"flow-7"));
        set.insert(b);

        set.insert(binding("sip:c@h", 500, 300, 1_000));
        set.insert(binding("sip:d@h", 1_000, 400, 250)); // expired at now = 300
        set.insert(binding("sip:e@h", 1_000, 50, 1_000));
        set
    }

    fn contacts(targets: &[Target]) -> Vec<&str> {
        targets
            .iter()
            .map(|t| std::str::from_utf8(&t.contact).unwrap_or("?"))
            .collect()
    }

    #[test]
    fn ls_l_1_and_2_order_is_q_then_recency_and_a_missing_q_is_full_preference() {
        let targets = order_targets(&fixture(), Timestamp::from_secs(300));
        assert_eq!(
            contacts(&targets),
            ["sip:b@h", "sip:a@h", "sip:e@h", "sip:c@h"]
        );
    }

    #[test]
    fn ls_l_1_an_expired_binding_is_absent() {
        let targets = order_targets(&fixture(), Timestamp::from_secs(300));
        assert!(!contacts(&targets).contains(&"sip:d@h"));
    }

    #[test]
    fn ls_l_3_the_order_is_a_pure_function_of_the_set_and_now() {
        let set = fixture();
        let now = Timestamp::from_secs(300);
        assert_eq!(order_targets(&set, now), order_targets(&set, now));
    }

    #[test]
    fn ls_l_3_equal_q_and_equal_recency_still_have_a_total_order() {
        // Without the contact-bytes tie-break, two identical-looking bindings would be ordered by
        // whatever the sort happened to do — and two nodes could disagree.
        let mut set = BindingSet::new();
        set.insert(binding("sip:z@h", 1_000, 100, 1_000));
        set.insert(binding("sip:y@h", 1_000, 100, 1_000));
        let targets = order_targets(&set, Timestamp::from_secs(0));
        assert_eq!(contacts(&targets), ["sip:y@h", "sip:z@h"]);
    }

    #[test]
    fn ls_l_4_every_binding_expired_is_the_empty_set() {
        let targets = order_targets(&fixture(), Timestamp::from_secs(5_000));
        assert!(targets.is_empty());
    }

    #[test]
    fn ls_l_5_a_target_carries_its_route_set_in_stored_order() {
        let targets = order_targets(&fixture(), Timestamp::from_secs(300));
        let a = targets
            .iter()
            .find(|t| t.contact.as_ref() == b"sip:a@h")
            .expect("A");
        assert_eq!(
            a.route_set,
            vec![
                Bytes::from_static(b"<sip:p2>"),
                Bytes::from_static(b"<sip:p1>")
            ]
        );
        assert_eq!(a.expires_in, Duration::from_secs(700));
        assert_eq!(a.q, 1_000);
    }

    #[test]
    fn ls_l_6_a_flow_reference_is_carried_opaquely() {
        let targets = order_targets(&fixture(), Timestamp::from_secs(300));
        let b = targets
            .iter()
            .find(|t| t.contact.as_ref() == b"sip:b@h")
            .expect("B");
        assert_eq!(b.flow_ref.as_deref(), Some(&b"flow-7"[..]));
    }

    #[test]
    fn ls_l_7_an_empty_set_yields_no_targets() {
        assert!(order_targets(&BindingSet::new(), Timestamp::ZERO).is_empty());
    }
}
