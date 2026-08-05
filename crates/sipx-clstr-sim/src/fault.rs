//! Failure as a scripted input.
//!
//! Faults are **not a third mechanism**. The topology sets per-link defaults; a schedule overrides
//! them in time windows; that is the whole model
//! ([conformance-harness](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/conformance-harness.md)).
//! A partition is `partitioned` turned on for the links that cross a cut. Killing a node gives
//! every link it has the same cut outcome, as an overlay that preserves the link's own policy for
//! a later restart, plus dropping the timers it was waiting for. Nothing here reaches past
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

    /// Close the connection between two nodes and accept a new one in its place.
    ///
    /// The fault a *link* cannot express, and the reason `CF-26` exists. A partition takes the
    /// wire away and gives it back; both ends still hold the connection they had, so anything
    /// naming that connection — a flow reference ([affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
    /// §11.2) — keeps resolving. A reconnect is the other failure: the connection is *gone*, the
    /// client comes back on a new one, and every reference to the old one died at that instant
    /// (§13.2 RS3). Both ends learn — a transport error, then an established connection — because
    /// a reconnect neither end noticed is a reconnect that changed nothing.
    ///
    /// Applied in one instant. A scenario that wants the client to be away for a while
    /// [partitions](Fault::Partition) first and reconnects at the end of the window.
    ///
    /// Both ends are told unconditionally, because the schedule named this pair: a scenario that
    /// reconnects two nodes is asserting there was a connection between them. Contrast
    /// [`Fault::RestartNode`], which has to infer whose connections it broke.
    Reconnect {
        /// One end.
        a: NodeId,
        /// The other.
        b: NodeId,
    },

    /// Stop a node and start it again, under an incarnation strictly greater than the one it was
    /// running under.
    ///
    /// Distinct from `KillNode` + a heal, and the distinction is the whole point: a restarted node
    /// is a **new process**. Its table is empty, so it re-issues connection slots from the
    /// beginning, and only the incarnation keeps a reference minted before the restart from
    /// resolving to a connection accepted after it ([affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
    /// §12.2 CT2, and FR-9 for what happens without one). The incarnation is the harness's to
    /// issue and the node's to receive, never a clock the node reads (AGENTS.md rule 2).
    ///
    /// A restart of a node a [`Fault::KillNode`] stopped puts its links back exactly as the kill
    /// cut them; a restart of a running node leaves link policy alone.
    ///
    /// **Which peers are told.** Only those holding a stream link to it. The harness has no
    /// connection object to consult, so what it uses is the closest thing it has: a link entry
    /// exists once a message has crossed that pair, which is as close to "a connection was opened"
    /// as this model gets. A node that has never exchanged a message with the restarted one is
    /// therefore told nothing, which is right — it had nothing to lose — but it does mean the
    /// notification follows traffic rather than topology.
    RestartNode(NodeId),

    /// Accept, but stop draining: writes toward this node are held rather than delivered.
    ///
    /// The non-reading peer. It is not loss and it is not a cut — the connection is up, and that
    /// is what makes it interesting: the bytes sit in the sender's buffer, so a sender that keeps
    /// writing discovers backpressure rather than an error
    /// ([owner-rpc](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/owner-rpc.md)
    /// §8 BQ2's `max_pending_per_flow`, and the `T_write` that must be able to expire for §7's
    /// `FlowRejected` to be reachable at all).
    ///
    /// Checked **before** the link, because a write that never left never reached the wire. A
    /// schedule that stalls a node and cuts its links in the same window is asking for two
    /// contradictory things and gets the stall; [`Fault::Reconnect`], [`Fault::RestartNode`] and
    /// [`Fault::KillNode`] each discard what was held, because a connection that is gone took its
    /// buffer with it. A `StopReading` scheduled *after* `KillNode` cannot recreate that buffer:
    /// death dominates backpressure in either event order.
    StopReading(NodeId),

    /// Read again: everything held for this node goes out, in the order it was written.
    ///
    /// The inverse of [`Fault::StopReading`], and the half that finds the bugs — a sender that
    /// coped with the stall and then mishandles the flush has an accounting defect, and there is
    /// no way to reach that state without scheduling the resume.
    ResumeReading(NodeId),
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
            Self::Reconnect { a, b } => format!("reconnect {a}|{b}"),
            Self::RestartNode(node) => format!("restart {node}"),
            Self::StopReading(node) => format!("stop-reading {node}"),
            Self::ResumeReading(node) => format!("resume-reading {node}"),
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

    /// Stop a node reading for a window, and let it drain at the end of it.
    ///
    /// Paired for the same reason [`Schedule::partition_during`] is: a stall with no resume is
    /// indistinguishable from a client that never came back, and the accounting bugs live in the
    /// flush.
    #[must_use]
    pub fn stalled_during(self, from: SimTime, until: SimTime, node: NodeId) -> Self {
        self.at(from, Fault::StopReading(node))
            .at(until, Fault::ResumeReading(node))
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
    fn a_stall_window_schedules_its_own_resume() {
        let schedule =
            Schedule::new().stalled_during(SimTime::from_secs(1), SimTime::from_secs(4), node(1));
        let shape: Vec<(SimTime, &Fault)> = schedule
            .entries()
            .iter()
            .map(|(at, fault)| (*at, fault))
            .collect();
        assert_eq!(
            shape,
            vec![
                (SimTime::from_secs(1), &Fault::StopReading(node(1))),
                (SimTime::from_secs(4), &Fault::ResumeReading(node(1))),
            ],
            "a window is a stall and the resume that ends it"
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
            Fault::Reconnect {
                a: node(0),
                b: node(1),
            },
            Fault::RestartNode(node(0)),
            Fault::StopReading(node(0)),
            Fault::ResumeReading(node(0)),
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
