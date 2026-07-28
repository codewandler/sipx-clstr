//! What a simulated node is, and how the scheduler talks to it.
//!
//! A node is sans-IO by construction here: it is handed inputs and returns effects, and has no
//! way to read a clock or touch a socket even if it wanted to. That is the same seam the real
//! driver sits on ([proxy-transaction-driver](https://github.com/codewandler/sipx-clstr/blob/main/docs/designs/proxy-transaction-driver.md)),
//! which is why a component proved here is proved for the binary too.

use std::any::Any;
use std::fmt;
use std::time::Duration;

use sipx_sip::Message;

use crate::net::NodeId;
use crate::time::SimTime;

/// A node's own name for one of its timers.
///
/// Opaque to the scheduler: what `TimerId(3)` means is the node's business, and the scheduler's
/// only job is to hand it back at the right virtual instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(pub u64);

impl fmt::Display for TimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

/// Something that happened to a node.
#[derive(Debug)]
pub enum Input<'a> {
    /// The scenario began. A node that sends the first message does it here.
    Started,
    /// A message arrived.
    Message {
        /// Who sent it.
        from: NodeId,
        /// What arrived, already parsed from the bytes that crossed the link.
        message: &'a Message,
    },
    /// A timer this node set has fired.
    Timer(TimerId),
    /// A stream link toward a peer is cut. RFC 3261 §16.9's transport error.
    TransportError {
        /// The unreachable peer.
        peer: NodeId,
    },
}

/// Something a node wants done.
///
/// Performed strictly in order. A node that emits `Send` before `SetTimer` gets exactly that, so
/// a retransmission timer can never start before the thing it retransmits has gone out — the
/// same ordering guarantee the kernel's transaction outputs carry, for the same reason.
#[derive(Debug)]
pub enum Effect {
    /// Put this message on the link toward `to`.
    Send {
        /// The peer.
        to: NodeId,
        /// The message, which is serialized and re-parsed on the way, exactly as on a wire.
        ///
        /// Boxed because a SIP message dwarfs every other effect, and an enum sized for its
        /// largest variant means every `SetTimer` in a busy scenario carries that weight too.
        message: Box<Message>,
    },
    /// Arm a timer.
    SetTimer {
        /// Which timer.
        timer: TimerId,
        /// How long from now.
        after: Duration,
    },
    /// Disarm a timer that has not fired.
    ClearTimer(TimerId),
    /// Record something in the trace. The seam a scenario asserts on when what matters is a
    /// decision rather than a message — a store lookup, a token verdict, a shed request.
    Note(String),
}

/// A participant in a simulation: a platform node, a simulated endpoint, a load balancer.
///
/// `Any` is a supertrait so a scenario can look inside a node it put in — asserting on a
/// registrar's bindings is often clearer than reconstructing them from the trace. `Debug` is one
/// so that a simulation can be printed whole when a scenario fails and the trace is not enough.
pub trait SimNode: Any + fmt::Debug {
    /// The name this node appears under in traces. Must be stable: it is part of the rendered
    /// output that two runs are compared on.
    fn name(&self) -> &str;

    /// React to one input.
    fn on_input(&mut self, now: SimTime, input: Input<'_>) -> Vec<Effect>;
}

/// Build an [`Effect::Send`] without spelling out the box at every call site.
#[must_use]
pub fn send(to: NodeId, message: Message) -> Effect {
    Effect::Send {
        to,
        message: Box::new(message),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_timer_id_renders_compactly() {
        assert_eq!(TimerId(7).to_string(), "t7");
    }
}
