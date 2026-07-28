//! The visualization wire vocabulary: one frame per trace entry, and nothing else.
//!
//! The constellation page ([cluster-viz design](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/cluster-viz.md))
//! animates a *serialization of the trace*, so the mapping from [`Event`] to [`Frame`] lives here,
//! next to the trace, where the compiler enforces it: the match in [`Frame::from_entry`] is
//! exhaustive, so a new event variant breaks this build rather than silently rendering as nothing
//! on stage. The dev adapter itself is a cargo example (`examples/viz/`); what lives in the
//! library is the vocabulary both sides agree on, because that is the part a test can pin.
//!
//! Frames are plain data — no serde, no JSON. Serialization is the adapter's job, which keeps the
//! harness free of an encoding dependency and the mapping testable as pure data.

use crate::net::{LinkKind, NodeId};
use crate::node::TimerId;
use crate::time::SimTime;
use crate::trace::{Entry, Event, Trace};

/// Which stage element a frame drives. The string form is the SSE `event:` name and the JSON
/// `kind` field — one token, so the page never parses payload shape to find the handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FrameKind {
    /// The scenario began; the node lights up.
    Started,
    /// A message was handed to a link; a particle sets out.
    Sent,
    /// A message arrived; the receiving node flashes.
    Received,
    /// The link discarded it; the particle fizzles mid-link.
    Dropped,
    /// The link delivers it twice; the particle splits.
    Duplicated,
    /// A stream link snapped; the link flashes.
    Broken,
    /// Bytes arrived that would not parse; a scrambled particle dissolves at the receiver.
    Malformed,
    /// A timer was armed; a ring starts sweeping the node.
    TimerSet,
    /// A timer fired; the ring flashes.
    TimerFired,
    /// A timer was disarmed; the ring fades.
    TimerCleared,
    /// A node's own record — a store lookup, a token verdict, a shed. A HUD log line.
    Note,
}

impl FrameKind {
    /// The wire token: SSE event name and JSON `kind` field in one.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Sent => "sent",
            Self::Received => "received",
            Self::Dropped => "dropped",
            Self::Duplicated => "duplicated",
            Self::Broken => "broken",
            Self::Malformed => "malformed",
            Self::TimerSet => "timer_set",
            Self::TimerFired => "timer_fired",
            Self::TimerCleared => "timer_cleared",
            Self::Note => "note",
        }
    }
}

/// One frame on the wire: one trace entry, flattened so the adapter's JSON encoder needs no enum
/// support. Every payload field is optional for the same reason — `kind` says which are present,
/// and the table in the design doc is the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Virtual time of the happening.
    pub at: SimTime,
    /// Global order; also the SSE `id`, so a reconnecting client can see a gap rather than
    /// silently miss entries.
    pub seq: u64,
    /// Where it happened.
    pub node: NodeId,
    /// The node's name, so a client never needs the topology beside the stream.
    pub node_name: String,
    /// Which stage element this drives.
    pub kind: FrameKind,
    /// The other end of the happening: `to` for sends, `from` for arrivals.
    pub peer: Option<NodeId>,
    /// Message summary, malformed reason, or note text — always the trace's own words, so the
    /// screen and the text render can never disagree about what a message was.
    pub summary: Option<String>,
    /// The timer a timer frame is about.
    pub timer: Option<TimerId>,
    /// When an armed timer will fire.
    pub timer_at: Option<SimTime>,
}

impl Frame {
    /// The frame for one trace entry. Total by construction: an [`Event`] variant without a
    /// mapping is a compile error here, which is the design's answer to schema drift.
    #[must_use]
    pub fn from_entry(entry: &Entry) -> Self {
        let (kind, peer, summary, timer, timer_at) = match &entry.event {
            Event::Started => (FrameKind::Started, None, None, None, None),
            Event::Sent { to, summary } => (
                FrameKind::Sent,
                Some(*to),
                Some(summary.clone()),
                None,
                None,
            ),
            Event::Received { from, summary } => (
                FrameKind::Received,
                Some(*from),
                Some(summary.clone()),
                None,
                None,
            ),
            Event::Dropped { to, summary } => (
                FrameKind::Dropped,
                Some(*to),
                Some(summary.clone()),
                None,
                None,
            ),
            Event::Duplicated { to, summary } => (
                FrameKind::Duplicated,
                Some(*to),
                Some(summary.clone()),
                None,
                None,
            ),
            Event::Broken { to } => (FrameKind::Broken, Some(*to), None, None, None),
            Event::Malformed { from, reason } => (
                FrameKind::Malformed,
                Some(*from),
                Some(reason.clone()),
                None,
                None,
            ),
            Event::TimerSet { timer, at } => {
                (FrameKind::TimerSet, None, None, Some(*timer), Some(*at))
            }
            Event::TimerFired(timer) => (FrameKind::TimerFired, None, None, Some(*timer), None),
            Event::TimerCleared(timer) => (FrameKind::TimerCleared, None, None, Some(*timer), None),
            Event::Note(note) => (FrameKind::Note, None, Some(note.clone()), None, None),
        };
        Self {
            at: entry.at,
            seq: entry.seq,
            node: entry.node,
            node_name: entry.node_name.clone(),
            kind,
            peer,
            summary,
            timer,
            timer_at,
        }
    }
}

/// Where a node sits on the fixed stage. The scenario builder says; the renderer never infers
/// from names — the design's open question on role hints, answered for `VZ-1`: explicit in the
/// `meta` frame, because name-sniffing is the brittle option and the builder already knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// A platform node — proxy, registrar — drawn in the centre of the stage.
    Edge,
    /// A simulated UA, carrier or probe — drawn at the wings.
    Endpoint,
}

impl Role {
    /// The wire token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edge => "edge",
            Self::Endpoint => "endpoint",
        }
    }
}

/// A node announcement in the `meta` frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMeta {
    /// Which node.
    pub id: NodeId,
    /// Its trace name.
    pub name: String,
    /// Where it sits on stage.
    pub role: Role,
}

/// A link announcement in the `meta` frame, one per pair (the stage draws links undirected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkMeta {
    /// One end.
    pub from: NodeId,
    /// The other end.
    pub to: NodeId,
    /// Datagram- or stream-shaped, so the stage can draw the link the way it can fail.
    pub kind: LinkKind,
}

/// One counter on the invariant HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invariant {
    /// The DP-3 counter name — the same vocabulary the metrics pipeline will use, so the
    /// animation and the alerts can never tell two stories.
    pub name: &'static str,
    /// The reading, or `None` for *uninstrumented*. The design's honesty rule: a counter with no
    /// instrumented source renders as uninstrumented, never as zero — silence and a clean reading
    /// must not look alike.
    pub value: Option<u64>,
}

/// The sim feed's invariant snapshot: a trace query per counter, emitted as the `invariant`
/// frame. Counters whose source does not exist yet are present and uninstrumented — the HUD
/// shows the shape of the whole DP-3 set from day one, honestly.
#[must_use]
pub fn invariants(trace: &Trace) -> Vec<Invariant> {
    // State rides the message (AGENTS.md non-negotiable 5): the proxy core has no dialog table,
    // so a cross-node dialog lookup could only ever appear here as a node's own note. The counter
    // is instrumented — the trace is its source — and it must read zero.
    let lookups = trace.count(|entry| {
        matches!(&entry.event, Event::Note(note) if note.contains("cross-node dialog lookup"))
    });
    vec![
        Invariant {
            name: "cross_node_dialog_lookups",
            value: Some(u64::try_from(lookups).unwrap_or(u64::MAX)),
        },
        Invariant {
            name: "token_verification_failures",
            value: None,
        },
        Invariant {
            name: "flow_rpc_failures",
            value: None,
        },
        Invariant {
            name: "trunk_breakers_open",
            value: None,
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn entry(event: Event) -> Entry {
        Entry {
            at: SimTime::from_millis(7),
            seq: 42,
            node: NodeId::from_index(1),
            node_name: "edge".to_owned(),
            event,
        }
    }

    /// The totality test the design promises: one entry per `Event` variant, each with its
    /// expected wire token. The mapping being an exhaustive match is what makes "a new variant
    /// fails the build" literal rather than aspirational; this test is what makes it visible.
    #[test]
    fn every_event_variant_has_a_frame_kind() {
        let cases: Vec<(Event, FrameKind)> = vec![
            (Event::Started, FrameKind::Started),
            (
                Event::Sent {
                    to: NodeId::from_index(2),
                    summary: "INVITE sip:bob [c #1 INVITE]".to_owned(),
                },
                FrameKind::Sent,
            ),
            (
                Event::Received {
                    from: NodeId::from_index(2),
                    summary: "200 OK [c #-]".to_owned(),
                },
                FrameKind::Received,
            ),
            (
                Event::Dropped {
                    to: NodeId::from_index(2),
                    summary: "INVITE sip:bob [c #1 INVITE]".to_owned(),
                },
                FrameKind::Dropped,
            ),
            (
                Event::Duplicated {
                    to: NodeId::from_index(2),
                    summary: "INVITE sip:bob [c #1 INVITE]".to_owned(),
                },
                FrameKind::Duplicated,
            ),
            (
                Event::Broken {
                    to: NodeId::from_index(2),
                },
                FrameKind::Broken,
            ),
            (
                Event::Malformed {
                    from: NodeId::from_index(2),
                    reason: "bad start line".to_owned(),
                },
                FrameKind::Malformed,
            ),
            (
                Event::TimerSet {
                    timer: TimerId(3),
                    at: SimTime::from_secs(1),
                },
                FrameKind::TimerSet,
            ),
            (Event::TimerFired(TimerId(3)), FrameKind::TimerFired),
            (Event::TimerCleared(TimerId(3)), FrameKind::TimerCleared),
            (
                Event::Note("looked up 2 target(s)".to_owned()),
                FrameKind::Note,
            ),
        ];
        assert_eq!(cases.len(), 11, "an Event variant is missing a case");
        for (event, expected) in cases {
            let frame = Frame::from_entry(&entry(event));
            assert_eq!(frame.kind, expected, "{}", expected.as_str());
        }
    }

    #[test]
    fn frame_kinds_have_unique_wire_tokens() {
        let kinds = [
            FrameKind::Started,
            FrameKind::Sent,
            FrameKind::Received,
            FrameKind::Dropped,
            FrameKind::Duplicated,
            FrameKind::Broken,
            FrameKind::Malformed,
            FrameKind::TimerSet,
            FrameKind::TimerFired,
            FrameKind::TimerCleared,
            FrameKind::Note,
        ];
        let tokens: BTreeSet<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(tokens.len(), kinds.len());
    }

    #[test]
    fn a_frame_carries_the_entrys_envelope_verbatim() {
        let frame = Frame::from_entry(&entry(Event::Sent {
            to: NodeId::from_index(9),
            summary: "BYE sip:alice [c #3 BYE]".to_owned(),
        }));
        assert_eq!(frame.at, SimTime::from_millis(7));
        assert_eq!(frame.seq, 42);
        assert_eq!(frame.node, NodeId::from_index(1));
        assert_eq!(frame.node_name, "edge");
        assert_eq!(frame.peer, Some(NodeId::from_index(9)));
        assert_eq!(frame.summary.as_deref(), Some("BYE sip:alice [c #3 BYE]"));
        assert_eq!(frame.timer, None);
    }

    #[test]
    fn timer_frames_carry_the_timer_and_its_deadline() {
        let set = Frame::from_entry(&entry(Event::TimerSet {
            timer: TimerId(5),
            at: SimTime::from_secs(3),
        }));
        assert_eq!(set.timer, Some(TimerId(5)));
        assert_eq!(set.timer_at, Some(SimTime::from_secs(3)));

        let fired = Frame::from_entry(&entry(Event::TimerFired(TimerId(5))));
        assert_eq!(fired.timer, Some(TimerId(5)));
        assert_eq!(fired.timer_at, None);
    }

    #[test]
    fn the_invariant_snapshot_marks_uninstrumented_counters_as_absent_not_zero() {
        let snapshot = invariants(&Trace::new());
        let by_name = |name: &str| snapshot.iter().find(|i| i.name == name).copied();
        assert_eq!(
            by_name("cross_node_dialog_lookups").and_then(|i| i.value),
            Some(0),
            "instrumented and clean: a real zero"
        );
        assert_eq!(
            by_name("token_verification_failures").and_then(|i| i.value),
            None,
            "no source yet: uninstrumented, never a pretend zero"
        );
        assert_eq!(by_name("flow_rpc_failures").and_then(|i| i.value), None);
        assert_eq!(by_name("trunk_breakers_open").and_then(|i| i.value), None);
    }

    #[test]
    fn a_cross_node_dialog_lookup_note_moves_the_counter() {
        let mut trace = Trace::new();
        trace.record(
            SimTime::START,
            NodeId::from_index(0),
            "edge",
            Event::Note("cross-node dialog lookup: abc".to_owned()),
        );
        let snapshot = invariants(&trace);
        let value = snapshot
            .iter()
            .find(|i| i.name == "cross_node_dialog_lookups")
            .and_then(|i| i.value);
        assert_eq!(value, Some(1));
    }
}
