//! The run's product: an append-only log of everything that happened.
//!
//! Assertions are queries over this, not over a node's private state. That is the point — "zero
//! cross-node dialog lookups" is a trace query, and a claim you can only make by asking a
//! component whether it behaved is a claim on the honour system.
//!
//! [`Trace::render`] produces a canonical, fixed-width text form. Same scenario, same seed,
//! byte-identical render — which makes "did this change?" a `diff`, and makes the first line that
//! differs the place to look.

use std::fmt::Write as _;

use sipx_sip::{Message, Method};

use crate::net::NodeId;
use crate::node::TimerId;
use crate::time::SimTime;

/// One thing that happened, at one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The scenario began and the node was told so.
    Started,
    /// A message was handed to the link toward `to`.
    Sent {
        /// Where it was addressed.
        to: NodeId,
        /// What it was.
        summary: String,
    },
    /// A message arrived.
    Received {
        /// Who sent it.
        from: NodeId,
        /// What it was.
        summary: String,
    },
    /// The link discarded it.
    Dropped {
        /// Where it was headed.
        to: NodeId,
        /// What it was.
        summary: String,
    },
    /// The link will deliver it twice.
    Duplicated {
        /// Where it was headed.
        to: NodeId,
        /// What it was.
        summary: String,
    },
    /// A stream link toward `to` is cut; the sender is told, as a transport error.
    Broken {
        /// The unreachable peer.
        to: NodeId,
    },
    /// Something arrived that could not be parsed. Traced rather than silently discarded: a
    /// serializer bug looks exactly like a network with no traffic on it.
    Malformed {
        /// Who sent it.
        from: NodeId,
        /// Why it would not parse.
        reason: String,
    },
    /// A timer was armed.
    TimerSet {
        /// Which timer.
        timer: TimerId,
        /// When it will fire.
        at: SimTime,
    },
    /// A timer fired.
    TimerFired(TimerId),
    /// A timer was disarmed before firing.
    TimerCleared(TimerId),
    /// Something the node wanted recorded — the hook a scenario asserts on when what matters is a
    /// decision rather than a message.
    Note(String),
}

/// One line of the trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Virtual time.
    pub at: SimTime,
    /// Global order among everything at this instant.
    pub seq: u64,
    /// Where it happened.
    pub node: NodeId,
    /// The node's name, so a rendered trace is readable without the topology beside it.
    pub node_name: String,
    /// What happened.
    pub event: Event,
}

/// Everything that happened, in order.
#[derive(Debug, Default)]
pub struct Trace {
    entries: Vec<Entry>,
    next_seq: u64,
}

impl Trace {
    /// An empty trace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record(&mut self, at: SimTime, node: NodeId, node_name: &str, event: Event) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(Entry {
            at,
            seq,
            node,
            node_name: node_name.to_owned(),
            event,
        });
    }

    /// Every entry, in the order it happened.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How many entries match a predicate — the shape most assertions take.
    pub fn count(&self, matching: impl FnMut(&&Entry) -> bool) -> usize {
        self.entries.iter().filter(matching).count()
    }

    /// Every message summary received by a node, in order.
    #[must_use]
    pub fn received_by(&self, node: NodeId) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.node == node)
            .filter_map(|entry| match &entry.event {
                Event::Received { summary, .. } => Some(summary.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Every note a node recorded, in order.
    #[must_use]
    pub fn notes(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.event {
                Event::Note(note) => Some(note.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The canonical text form. Two runs of the same scenario at the same seed render byte for
    /// byte alike, so comparing them is a string comparison and locating a divergence is a diff.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            // Writing to a String is infallible; the result is discarded rather than unwrapped
            // because a panic here would be a panic in the thing that explains failures.
            let _ = writeln!(
                out,
                "{} {:>4} {:<10} {}",
                entry.at,
                entry.seq,
                entry.node_name,
                render_event(&entry.event)
            );
        }
        out
    }
}

fn render_event(event: &Event) -> String {
    match event {
        Event::Started => "started".to_owned(),
        Event::Sent { to, summary } => format!("-> {to} {summary}"),
        Event::Received { from, summary } => format!("<- {from} {summary}"),
        Event::Dropped { to, summary } => format!("xx {to} {summary} (lost)"),
        Event::Duplicated { to, summary } => format!("== {to} {summary} (duplicated)"),
        Event::Broken { to } => format!("!! {to} link broken"),
        Event::Malformed { from, reason } => format!("?? {from} unparseable: {reason}"),
        Event::TimerSet { timer, at } => format!("timer {timer} set for {at}"),
        Event::TimerFired(timer) => format!("timer {timer} fired"),
        Event::TimerCleared(timer) => format!("timer {timer} cleared"),
        Event::Note(note) => format!("note {note}"),
    }
}

/// A one-line description of a SIP message: enough to follow a trace, stable enough to diff.
///
/// Deliberately not the whole message. A trace that reproduced every header would be unreadable
/// at exactly the moment it is needed, and the details are in the message the scenario still
/// holds.
#[must_use]
pub fn summarize(message: &Message) -> String {
    let header = |name: &sipx_sip::HeaderName| {
        message.headers().value(name).map_or_else(
            || "-".to_owned(),
            |value| String::from_utf8_lossy(&value).trim().to_owned(),
        )
    };
    let call_id = header(&sipx_sip::HeaderName::CallId);
    let cseq = header(&sipx_sip::HeaderName::CSeq);

    match message {
        Message::Request(request) => {
            format!(
                "{} {} [{call_id} #{cseq}]",
                method_name(&request.method),
                request.uri
            )
        }
        Message::Response(response) => {
            let reason = String::from_utf8_lossy(&response.reason);
            format!(
                "{} {} [{call_id} #{cseq}]",
                response.status.code(),
                reason.trim()
            )
        }
    }
}

fn method_name(method: &Method) -> String {
    match method {
        Method::Other(name) => String::from_utf8_lossy(name).into_owned(),
        known => format!("{known:?}").to_uppercase(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use sipx_sip::{HeaderName, RequestBuilder, StatusCode, Uri};

    fn request() -> Message {
        let uri = Uri::parse(Bytes::from_static(b"sip:bob@example.test")).unwrap();
        let built = RequestBuilder::new(Method::Invite, uri)
            .header(HeaderName::CallId, "abc123")
            .unwrap()
            .cseq(1, &Method::Invite)
            .unwrap()
            .build();
        Message::Request(built)
    }

    #[test]
    fn a_request_summary_names_the_method_uri_call_and_sequence() {
        assert_eq!(
            summarize(&request()),
            "INVITE sip:bob@example.test [abc123 #1 INVITE]"
        );
    }

    #[test]
    fn a_response_summary_leads_with_the_status() {
        let response = sipx_sip::ResponseBuilder::new(StatusCode::new(200).unwrap(), "OK")
            .unwrap()
            .header(HeaderName::CallId, "abc123")
            .unwrap()
            .build();
        assert_eq!(
            summarize(&Message::Response(response)),
            "200 OK [abc123 #-]"
        );
    }

    #[test]
    fn a_message_missing_its_identifying_headers_still_summarizes() {
        // A trace that cannot describe a malformed message is a trace that goes quiet exactly
        // when something is wrong.
        let uri = Uri::parse(Bytes::from_static(b"sip:bob@example.test")).unwrap();
        let bare = RequestBuilder::new(Method::Options, uri).build();
        assert_eq!(
            summarize(&Message::Request(bare)),
            "OPTIONS sip:bob@example.test [- #-]"
        );
    }

    #[test]
    fn the_render_is_stable_and_ordered() {
        let mut trace = Trace::new();
        trace.record(SimTime::START, NodeId(0), "alice", Event::Started);
        trace.record(
            SimTime::from_millis(5),
            NodeId(0),
            "alice",
            Event::Sent {
                to: NodeId(1),
                summary: "INVITE sip:bob [c #1 INVITE]".to_owned(),
            },
        );
        let rendered = trace.render();
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 2);
        let (first, second) = (lines.first().copied(), lines.get(1).copied());
        assert!(first.is_some_and(|l| l.ends_with("started")), "{first:?}");
        assert!(
            second.is_some_and(|l| l.contains("-> n1 INVITE")),
            "{second:?}"
        );
        assert_eq!(rendered, trace.render());
    }
}
