//! Turning a location lookup into targets the forwarding core forks on — `RG-6`.
//!
//! The two `Target` types are deliberately separate. The location service's carries what a
//! *registration* knows (`expires_in`, the `flow_ref`); the proxy's carries what *forwarding* needs.
//! Keeping them apart is what lets the same forwarding core take targets from a trunk (`RT-*`) or
//! straight from a Request-URI, and this module is the one place the first becomes the second.
//!
//! Ordering is **not** redone here. [`sipx_clstr_registrar::order_targets`] already applied
//! [location-service §7](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/location-service.md)'s
//! L1–L3 — expired bindings excluded, descending `q`, ties by recency then contact bytes — and it is
//! a pure function of the set and `now`, so every node computes the identical order. Re-sorting here
//! would be a second opinion about that, and two opinions is exactly one too many.

use crate::types::Target;

/// Convert an ordered lookup result into forking targets, preserving order.
///
/// The `Path` vector becomes the branch's route set, in stored order (topmost first) — RFC 3327
/// §5.2 accumulates `Path` the way `Record-Route` accumulates, so the topmost value is the first hop
/// toward the UA.
#[must_use]
pub fn targets_from_lookup(found: &[sipx_clstr_registrar::Target]) -> Vec<Target> {
    found
        .iter()
        .map(|binding| Target {
            uri: binding.contact.clone(),
            route_set: binding.route_set.clone(),
            q: binding.q,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::time::Duration;

    fn found(
        contact: &'static str,
        q: u16,
        path: Vec<&'static str>,
    ) -> sipx_clstr_registrar::Target {
        sipx_clstr_registrar::Target {
            contact: Bytes::from_static(contact.as_bytes()),
            route_set: path
                .into_iter()
                .map(|p| Bytes::from_static(p.as_bytes()))
                .collect(),
            flow_ref: None,
            q,
            expires_in: Duration::from_hours(1),
        }
    }

    #[test]
    fn the_lookups_order_is_preserved_exactly() {
        // §7 L3 makes the lookup order a pure function of the set and `now`, so every node computes
        // the same one. Re-sorting here would be a second opinion about it.
        let targets = targets_from_lookup(&[
            found("sip:bob@10.0.0.2", 1_000, vec![]),
            found("sip:bob@10.0.0.1", 1_000, vec![]),
            found("sip:bob@10.0.0.3", 500, vec![]),
        ]);
        let uris: Vec<&[u8]> = targets.iter().map(|t| t.uri.as_ref()).collect();
        assert_eq!(
            uris,
            [
                &b"sip:bob@10.0.0.2"[..],
                &b"sip:bob@10.0.0.1"[..],
                &b"sip:bob@10.0.0.3"[..]
            ]
        );
    }

    #[test]
    fn a_contact_keeps_its_verbatim_bytes() {
        // F2: what the UA registered is what goes in the Request-URI, parameters and all.
        let targets =
            targets_from_lookup(&[found("sip:bob@10.0.0.1;transport=tcp", 1_000, vec![])]);
        assert_eq!(
            targets.first().map(|t| t.uri.as_ref()),
            Some(&b"sip:bob@10.0.0.1;transport=tcp"[..])
        );
    }

    #[test]
    fn a_path_becomes_the_branchs_route_set_in_stored_order() {
        let targets = targets_from_lookup(&[found(
            "sip:bob@10.0.0.1",
            1_000,
            vec!["<sip:p2.example;lr>", "<sip:p1.example;lr>"],
        )]);
        assert_eq!(
            targets.first().map(|t| t.route_set.clone()),
            Some(vec![
                Bytes::from_static(b"<sip:p2.example;lr>"),
                Bytes::from_static(b"<sip:p1.example;lr>"),
            ]),
            "RFC 3327 §5.2: topmost is the first hop toward the UA"
        );
    }

    #[test]
    fn an_empty_lookup_yields_no_targets() {
        // Which the engine turns into 480 (§7). The location service returns empty sets rather than
        // deciding responses, and this conversion keeps it that way.
        assert!(targets_from_lookup(&[]).is_empty());
    }

    #[test]
    fn the_q_value_survives_so_the_engine_can_group_branches() {
        let targets = targets_from_lookup(&[found("sip:bob@10.0.0.1", 700, vec![])]);
        assert_eq!(targets.first().map(|t| t.q), Some(700));
    }
}
