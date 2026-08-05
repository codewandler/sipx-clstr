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

    /// A connection to a peer has been established, replacing whatever this node had toward it.
    ///
    /// The signal a partition cannot give and [`crate::Fault::Reconnect`] can: *this is a
    /// different connection*. A node that keeps a connection table takes a fresh slot and bumps
    /// its generation here ([affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
    /// §12.2 CT3), which is what makes every reference to the connection it replaced dead rather
    /// than stale-but-plausible.
    Connected {
        /// The peer on the other end of the new connection.
        peer: NodeId,
    },

    /// This node has restarted: it is a new process, and this is its incarnation.
    ///
    /// Whatever the node held is gone — clearing it is the node's own business, because only the
    /// node knows what surviving a restart would mean. The incarnation is **an input**, never a
    /// clock the node reads (AGENTS.md rule 2, and [affinity-token](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
    /// §12.2 CT2 says the same of the real one); the harness guarantees only that it is strictly
    /// greater than the one the previous run carried, which is the whole of what CT2 needs.
    Restarted {
        /// Strictly greater than the incarnation of the run this one replaced. Nodes are born at 1.
        incarnation: u32,
    },

    /// A write toward a peer has not left this node: the peer is accepted but is not reading.
    ///
    /// One per write held. Backpressure rather than an error — the connection is up — which is
    /// what a bound on pending writes and a write deadline are both about
    /// ([owner-rpc](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/owner-rpc.md)
    /// §8 BQ2 and `T_write`). What a held write *costs* is the node's to decide: the harness knows
    /// about connections and the node knows about flows.
    WriteStalled {
        /// Whose socket is not being drained.
        peer: NodeId,
    },

    /// A write toward a peer that had stalled has now left.
    ///
    /// One per write released, in the order they were written, so a node can undo its own
    /// accounting entry by entry rather than guessing how much drained.
    WriteFlushed {
        /// The peer that started reading again.
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
