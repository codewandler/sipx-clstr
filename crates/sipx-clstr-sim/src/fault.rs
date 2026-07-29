//! Failure as a scripted input.
//!
//! Faults are **not a third mechanism**. The topology sets per-link defaults; a schedule overrides
//! them in time windows; that is the whole model
//! ([conformance-harness](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/conformance-harness.md)).
//! A partition is `partitioned` turned on for the links that cross a cut. Killing a node is that
//! on every link it has, plus dropping the timers it was waiting for. Nothing here reaches past
//! [`crate::net`] to invent a second way for a message to go missing, because two mechanisms that
//! can both drop a packet eventually disagree about whether one was dropped.
//!
//! **Actions are queue entries, not a side channel.** A scheduled fault is an event with a
//! deadline like any other, so it takes its place in the one totally ordered event queue and a
//! fault landing at the same instant as a delivery resolves by insertion sequence — the same rule
//! that makes everything else here a pure function of (scenario, seed). A fault applied from
//! outside that queue would be the one thing in the simulation whose ordering depended on when the
//! scenario author happened to call it.
//!
//! **Schedules are data.** [`Fault`] and [`Schedule`] are plain values, so a scenario can generate
//! them, mutate them, or fuzz them without touching scenario logic, and composing two schedules is
//! concatenating two lists.

use std::time::Duration;

use crate::net::{LinkPolicy, NodeId};
use crate::time::SimTime;

/// Something that goes wrong, at a scheduled instant.
///
/// Deliberately closed rather than a trait: an open set would let a scenario inject a fault the
/// trace vocabulary cannot describe, and a failure mode that cannot be rendered is one nobody can
/// diagnose from a failing seed.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    /// The node stops: every link it has is cut in both directions, and the timers it was waiting
    /// for are cancelled.
    ///
    /// Cancelling the timers is what makes this a *kill* rather than an isolation. A node whose
    /// timers keep firing is a node that is still running and merely unreachable — a real and
    /// different failure, which is [`Fault::Partition`] over all of its links.
    KillNode(NodeId),

    /// Cut every link crossing between the two groups, in both directions.
    ///
    /// Links *within* a group are untouched, which is what makes this a partition rather than a
    /// blackout: each side stays internally healthy and reaches its own conclusions, and the
    /// interesting behaviour is what the two sides decided while they could not talk.
    Partition {
        /// One side of the cut.
        a: Vec<NodeId>,
        /// The other.
        b: Vec<NodeId>,
    },

    /// Reconnect every link crossing between the two groups.
    ///
    /// The inverse of [`Fault::Partition`], and the half that finds the bugs: a cluster that
    /// survives a partition and then misbehaves when it ends has a reconciliation defect, and
    /// there is no way to reach that state without scheduling the heal.
    Heal {
        /// One side of the cut.
        a: Vec<NodeId>,
        /// The other.
        b: Vec<NodeId>,
    },

    /// Replace the policy on one direction of one link.
    ///
    /// The general case the other variants are shorthands for: loss, duplication and latency all
    /// change this way, so a "the far side got slow" scenario is one entry rather than a knob.
    SetLinkPolicy {
        /// The sending end.
        from: NodeId,
        /// The receiving end.
        to: NodeId,
        /// What the link becomes.
        policy: LinkPolicy,
    },

    /// Run a node's timers fast or slow, in per-mille of nominal.
    ///
    /// 1000 is nominal, 1100 is a clock 10% fast, 900 is 10% slow. Applied when a `SetTimer`
    /// effect is translated into a queue entry, so it skews what the node *waits for* without
    /// touching what it *believes the time is* — which is the real failure: RFC 3261's timers are
    /// durations, and two nodes disagreeing about how long a second is will disagree about whether
    /// a transaction timed out.
    TimerSkew {
        /// Whose clock.
        node: NodeId,
        /// Per-mille of nominal rate; 1000 is no skew.
        per_mille: u32,
    },
}

impl Fault {
    /// A one-line description, for the trace.
    ///
    /// Rendered here rather than at the trace site so that a new variant cannot be added without
    /// deciding how a reader will see it.
    #[must_use]
    pub fn summary(&self) -> String {
        fn group(nodes: &[NodeId]) -> String {
            nodes
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        }
        match self {
            Self::KillNode(node) => format!("kill {node}"),
            Self::Partition { a, b } => format!("partition {{{}}} | {{{}}}", group(a), group(b)),
            Self::Heal { a, b } => format!("heal {{{}}} | {{{}}}", group(a), group(b)),
            Self::SetLinkPolicy { from, to, policy } => format!(
                "policy {from}->{to} loss={} dup={} cut={}",
                policy.loss, policy.duplicate, policy.partitioned
            ),
            Self::TimerSkew { node, per_mille } => format!("skew {node} {per_mille}permille"),
        }
    }
}

/// Faults with the instants they happen at.
///
/// Insertion order is preserved and is the tie-break for two faults at the same instant, so a
/// schedule reads as what it does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Schedule {
    entries: Vec<(SimTime, Fault)>,
}

impl Schedule {
    /// An empty schedule — a scenario with no weather.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fault at an absolute instant.
    ///
    /// A fault at [`SimTime::START`] lands *after* the nodes have been started, because starting
    /// them is what creates time zero — so a `TimerSkew` scheduled there does not affect the timer
    /// a node arms in response to `Started`, only the ones it arms afterwards. Weather arrives at
    /// a running scenario; it is not a property a node is born with.
    #[must_use]
    pub fn at(mut self, at: SimTime, fault: Fault) -> Self {
        self.entries.push((at, fault));
        self
    }

    /// Add a fault this long after the scenario starts.
    #[must_use]
    pub fn after(self, after: Duration, fault: Fault) -> Self {
        self.at(SimTime::START.saturating_add(after), fault)
    }

    /// Cut between two groups for a window, and heal at the end of it.
    ///
    /// The paired heal is the point: a partition scheduled without one is indistinguishable from a
    /// permanent one for the rest of the scenario, and the bugs live in the reconnection.
    #[must_use]
    pub fn partition_during(
        self,
        from: SimTime,
        until: SimTime,
        a: Vec<NodeId>,
        b: Vec<NodeId>,
    ) -> Self {
        self.at(
            from,
            Fault::Partition {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .at(until, Fault::Heal { a, b })
    }

    /// Everything in this schedule, then everything in the other.
    ///
    /// Composition is concatenation, which is why schedules are values: two scenarios' weather can
    /// be merged without either knowing about the other.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        self.entries.extend(other.entries);
        self
    }

    /// The scheduled entries, in insertion order.
    #[must_use]
    pub fn entries(&self) -> &[(SimTime, Fault)] {
        &self.entries
    }

    /// Whether anything is scheduled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn node(index: usize) -> NodeId {
        NodeId::from_index(index)
    }

    #[test]
    fn merging_two_schedules_concatenates_them() {
        let first = Schedule::new().at(SimTime::from_secs(1), Fault::KillNode(node(0)));
        let second = Schedule::new().at(SimTime::from_secs(2), Fault::KillNode(node(1)));
        let merged = first.merge(second);
        let instants: Vec<SimTime> = merged.entries().iter().map(|(at, _)| *at).collect();
        assert_eq!(
            instants,
            vec![SimTime::from_secs(1), SimTime::from_secs(2)],
            "the first schedule's entries, then the second's"
        );
    }

    #[test]
    fn a_partition_window_schedules_its_own_heal() {
        let schedule = Schedule::new().partition_during(
            SimTime::from_secs(3),
            SimTime::from_secs(9),
            vec![node(0)],
            vec![node(1)],
        );
        let shape: Vec<(SimTime, &'static str)> = schedule
            .entries()
            .iter()
            .map(|(at, fault)| {
                let which = match fault {
                    Fault::Partition { .. } => "partition",
                    Fault::Heal { .. } => "heal",
                    _ => "unexpected",
                };
                (*at, which)
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (SimTime::from_secs(3), "partition"),
                (SimTime::from_secs(9), "heal"),
            ],
            "a window is a cut and the heal that ends it"
        );
    }

    #[test]
    fn every_fault_renders_a_summary_that_names_what_it_touched() {
        // A failing seed is diagnosed from the trace, so a fault that renders as a blank or an
        // opaque token would make the interesting scenarios the unreadable ones.
        let faults = [
            Fault::KillNode(node(0)),
            Fault::Partition {
                a: vec![node(0)],
                b: vec![node(1)],
            },
            Fault::Heal {
                a: vec![node(0)],
                b: vec![node(1)],
            },
            Fault::SetLinkPolicy {
                from: node(0),
                to: node(1),
                policy: LinkPolicy::CLEAN.with_loss(0.5),
            },
            Fault::TimerSkew {
                node: node(0),
                per_mille: 1100,
            },
        ];
        for fault in faults {
            let summary = fault.summary();
            assert!(!summary.is_empty(), "{fault:?}");
            assert!(
                summary.contains("n0"),
                "the summary must name a node it touched: {summary}"
            );
        }
    }
}
