//! The simulated network: nodes, links, and what a link does to a message.
//!
//! Two link kinds mirror the transport reliability classes the transaction machines care about. A
//! **datagram** link samples per message — drop, duplicate, deliver at `now + latency` — so
//! reordering emerges from latency jitter the way it does on a wire rather than being a separate
//! knob pretending to be physics. A **stream** link preserves FIFO order and does not lose
//! messages; it fails by breaking, which surfaces as a transport error (RFC 3261 §16.9).
//!
//! Faults are not a third mechanism: a partition is a scheduled override of `partitioned` on the
//! crossing links, and a node kill applies the same [`Link::cut_outcome`] to every link of a node
//! without overwriting policy that must reappear after a restart. `CF-4` adds the scheduling; this
//! module still owns every way a link decides that bytes do not arrive.

use std::fmt;
use std::time::Duration;

use crate::rng::SimRng;

/// Which node, by position in the simulation.
///
/// An index rather than a name so that every iteration over nodes is in a fixed order — a
/// `HashMap<String, _>` walked in hash order would make the trace depend on the allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// The node at a known position.
    ///
    /// For scenarios that name their nodes as constants: the ids a topology hands back are
    /// positions, and writing `NodeId::from_index(1)` beside the `add_node` calls reads better
    /// than threading three bindings through every test in a file.
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }

    /// Its position, for indexing into the simulation's node list.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

/// How long a message spends on a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latency {
    /// The same every time.
    Constant(Duration),
    /// Uniform between two bounds, inclusive. Jitter here is what produces reordering on a
    /// datagram link, which is why there is no separate reordering knob.
    Uniform {
        /// Shortest possible delivery.
        min: Duration,
        /// Longest possible delivery.
        max: Duration,
    },
}

impl Latency {
    /// No delay at all — the default for a link a scenario does not care about.
    pub const INSTANT: Self = Self::Constant(Duration::ZERO);

    /// Uniform jitter between two millisecond bounds.
    #[must_use]
    pub const fn uniform_ms(min: u64, max: u64) -> Self {
        Self::Uniform {
            min: Duration::from_millis(min),
            max: Duration::from_millis(max),
        }
    }

    /// Draw one delivery delay.
    pub fn sample(self, rng: &mut SimRng) -> Duration {
        match self {
            Self::Constant(delay) => delay,
            Self::Uniform { min, max } => {
                let low = u64::try_from(min.as_nanos()).unwrap_or(u64::MAX);
                let high = u64::try_from(max.as_nanos()).unwrap_or(u64::MAX);
                Duration::from_nanos(rng.range_inclusive(low, high))
            }
        }
    }

    /// Whether sampling this distribution consumes randomness.
    ///
    /// A constant does not, so a scenario that adds a fixed-latency link does not shift the draws
    /// of every other link in the topology.
    #[must_use]
    pub fn is_deterministic(self) -> bool {
        match self {
            Self::Constant(_) => true,
            Self::Uniform { min, max } => min >= max,
        }
    }
}

/// What a link does to the messages crossing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkPolicy {
    /// Probability a datagram is dropped.
    pub loss: f64,
    /// Probability a datagram is delivered twice.
    pub duplicate: f64,
    /// How long delivery takes.
    pub latency: Latency,
    /// Whether the link is currently cut. A cut datagram link drops; a cut stream link breaks.
    pub partitioned: bool,
}

impl LinkPolicy {
    /// A perfect link: instant, lossless, connected. What a vector-replay scenario wants, so that
    /// what it asserts is the protocol and not the weather.
    pub const CLEAN: Self = Self {
        loss: 0.0,
        duplicate: 0.0,
        latency: Latency::INSTANT,
        partitioned: false,
    };

    /// A link with realistic jitter and no loss.
    #[must_use]
    pub const fn jittery(min_ms: u64, max_ms: u64) -> Self {
        Self {
            latency: Latency::uniform_ms(min_ms, max_ms),
            ..Self::CLEAN
        }
    }

    /// This link, but lossy.
    #[must_use]
    pub const fn with_loss(mut self, loss: f64) -> Self {
        self.loss = loss;
        self
    }

    /// This link, but duplicating.
    #[must_use]
    pub const fn with_duplication(mut self, duplicate: f64) -> Self {
        self.duplicate = duplicate;
        self
    }
}

impl Default for LinkPolicy {
    fn default() -> Self {
        Self::CLEAN
    }
}

/// Whether a link behaves like a datagram transport or a stream one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// UDP-shaped: per-message loss, duplication, and reordering from jitter.
    Datagram,
    /// TCP/TLS/WebSocket-shaped: FIFO, no loss while connected, breaks rather than drops.
    Stream,
}

/// What a link decided to do with one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// Deliver once, after this delay.
    Once(Duration),
    /// Deliver twice — the second copy after the first, so a duplicate never arrives before the
    /// original it duplicates.
    Twice(Duration, Duration),
    /// Lost.
    Dropped,
    /// The link is cut and this kind of link fails loudly rather than silently.
    Broken,
}

/// One direction of one link.
#[derive(Debug)]
pub(crate) struct Link {
    pub(crate) kind: LinkKind,
    pub(crate) policy: LinkPolicy,
    rng: SimRng,
    /// When the last message on a stream link was scheduled to arrive, so FIFO can be enforced.
    last_arrival: Option<crate::time::SimTime>,
}

impl Link {
    pub(crate) fn new(kind: LinkKind, policy: LinkPolicy, master_seed: u64, label: &str) -> Self {
        Self {
            kind,
            policy,
            rng: SimRng::stream(master_seed, label),
            last_arrival: None,
        }
    }

    /// Decide what happens to one message, consuming randomness only where a decision is
    /// genuinely uncertain.
    pub(crate) fn decide(&mut self, now: crate::time::SimTime) -> Delivery {
        if self.policy.partitioned {
            return self.cut_outcome();
        }

        if self.kind == LinkKind::Datagram && self.rng.chance(self.policy.loss) {
            return Delivery::Dropped;
        }

        let first = self.policy.latency.sample(&mut self.rng);
        let duplicated = self.kind == LinkKind::Datagram && self.rng.chance(self.policy.duplicate);

        if self.kind == LinkKind::Stream {
            // FIFO: a stream link cannot deliver out of order however the latency lands, so the
            // arrival is pushed to at least one nanosecond after the previous one. Without this,
            // jitter on a TCP link would reorder messages that the real transport guarantees not
            // to — and a test written against that would be testing a transport nobody has.
            let arrival = now.saturating_add(first);
            let arrival = match self.last_arrival {
                Some(previous) if arrival <= previous => {
                    previous.saturating_add(Duration::from_nanos(1))
                }
                _ => arrival,
            };
            self.last_arrival = Some(arrival);
            return Delivery::Once(arrival.since(now));
        }

        if duplicated {
            let second = self.policy.latency.sample(&mut self.rng);
            let (early, late) = if first <= second {
                (first, second)
            } else {
                (second, first)
            };
            return Delivery::Twice(early, late);
        }

        Delivery::Once(first)
    }

    /// The deterministic result of cutting this transport, shared by partitions and node kills.
    pub(crate) const fn cut_outcome(&self) -> Delivery {
        match self.kind {
            LinkKind::Datagram => Delivery::Dropped,
            LinkKind::Stream => Delivery::Broken,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::time::SimTime;

    #[test]
    fn a_partitioned_datagram_link_drops_and_a_stream_link_breaks() {
        let policy = LinkPolicy {
            partitioned: true,
            ..LinkPolicy::CLEAN
        };
        let mut datagram = Link::new(LinkKind::Datagram, policy, 1, "l");
        let mut stream = Link::new(LinkKind::Stream, policy, 1, "l");
        assert_eq!(datagram.decide(SimTime::START), Delivery::Dropped);
        assert_eq!(stream.decide(SimTime::START), Delivery::Broken);
    }

    #[test]
    fn a_stream_link_never_loses_a_message_however_lossy_the_policy() {
        // Loss is a datagram property. A TCP connection that "loses" a message has broken, and
        // pretending otherwise would let a test assert behaviour no real transport produces.
        let mut link = Link::new(LinkKind::Stream, LinkPolicy::CLEAN.with_loss(0.99), 2, "l");
        for _ in 0..200 {
            assert!(matches!(link.decide(SimTime::START), Delivery::Once(_)));
        }
    }

    #[test]
    fn a_stream_link_preserves_order_under_jitter() {
        let mut link = Link::new(LinkKind::Stream, LinkPolicy::jittery(1, 50), 3, "l");
        let now = SimTime::START;
        let mut last = SimTime::START;
        for _ in 0..500 {
            let Delivery::Once(delay) = link.decide(now) else {
                panic!("a stream link should deliver");
            };
            let arrival = now.saturating_add(delay);
            assert!(
                arrival > last || last == SimTime::START,
                "arrivals went backwards"
            );
            last = arrival;
        }
    }

    #[test]
    fn a_datagram_link_reorders_when_the_jitter_is_wide() {
        let mut link = Link::new(LinkKind::Datagram, LinkPolicy::jittery(1, 50), 4, "l");
        let now = SimTime::START;
        let mut previous = Duration::ZERO;
        let mut reordered = false;
        for _ in 0..500 {
            let Delivery::Once(delay) = link.decide(now) else {
                continue;
            };
            reordered |= delay < previous;
            previous = delay;
        }
        assert!(reordered, "wide jitter on a datagram link should reorder");
    }

    #[test]
    fn a_duplicate_never_arrives_before_its_original() {
        let mut link = Link::new(
            LinkKind::Datagram,
            LinkPolicy::jittery(1, 50).with_duplication(1.0),
            5,
            "l",
        );
        for _ in 0..200 {
            match link.decide(SimTime::START) {
                Delivery::Twice(first, second) => assert!(first <= second),
                other => panic!("expected a duplicate, got {other:?}"),
            }
        }
    }

    #[test]
    fn certain_loss_loses_everything_and_no_loss_loses_nothing() {
        let mut always = Link::new(LinkKind::Datagram, LinkPolicy::CLEAN.with_loss(1.0), 6, "l");
        let mut never = Link::new(LinkKind::Datagram, LinkPolicy::CLEAN, 6, "l");
        for _ in 0..100 {
            assert_eq!(always.decide(SimTime::START), Delivery::Dropped);
            assert_eq!(never.decide(SimTime::START), Delivery::Once(Duration::ZERO));
        }
    }

    #[test]
    fn a_clean_link_consumes_no_randomness() {
        // So that adding one to a topology cannot shift what every other link draws.
        let mut link = Link::new(LinkKind::Datagram, LinkPolicy::CLEAN, 7, "l");
        let before = link.rng.clone().next_u64();
        for _ in 0..50 {
            let _ = link.decide(SimTime::START);
        }
        assert_eq!(link.rng.next_u64(), before);
    }
}
