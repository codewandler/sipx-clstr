//! The discrete-event queue.
//!
//! Advancing time *is* popping this queue: `now` jumps to the next event's deadline and nothing
//! else moves it, so timers fire only when the simulation advances and an idle cluster
//! fast-forwards through silence for free.
//!
//! Equal deadlines are broken by insertion sequence, which is what makes the total event order a
//! pure function of (scenario, seed) rather than of `HashMap` iteration or thread scheduling.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Duration;

use crate::time::SimTime;

/// A queue of things that have not happened yet.
#[derive(Debug)]
pub struct EventQueue<T> {
    heap: BinaryHeap<Scheduled<T>>,
    next_seq: u64,
}

/// One scheduled item: when, in what order among equals, and what.
#[derive(Debug)]
struct Scheduled<T> {
    at: SimTime,
    seq: u64,
    payload: T,
}

// Ordering is inverted so that `BinaryHeap` — a max-heap — pops the *earliest* deadline. Written
// out rather than derived because the field order that makes `derive(Ord)` correct here is a
// coincidence waiting to be broken by someone adding a field.
impl<T> Ord for Scheduled<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .at
            .cmp(&self.at)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl<T> PartialOrd for Scheduled<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> PartialEq for Scheduled<T> {
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at && self.seq == other.seq
    }
}

impl<T> Eq for Scheduled<T> {}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EventQueue<T> {
    /// An empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            next_seq: 0,
        }
    }

    /// Schedule something at an absolute time. Returns its insertion sequence.
    pub fn schedule_at(&mut self, at: SimTime, payload: T) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.heap.push(Scheduled { at, seq, payload });
        seq
    }

    /// Schedule something `after` a delay from `now`.
    pub fn schedule_after(&mut self, now: SimTime, after: Duration, payload: T) -> u64 {
        self.schedule_at(now.saturating_add(after), payload)
    }

    /// When the next event is due, if there is one.
    #[must_use]
    pub fn next_deadline(&self) -> Option<SimTime> {
        self.heap.peek().map(|scheduled| scheduled.at)
    }

    /// The earliest event without removing it.
    ///
    /// Needed since `CF-7` split timers into the kernel's queue: at an instant where both queues
    /// have something due, the scheduler has to look at *what* this one holds before deciding which
    /// to drain, because a fault and a delivery due at the same moment do not have the same
    /// precedence against a timer.
    #[must_use]
    pub fn peek(&self) -> Option<&T> {
        self.heap.peek().map(|scheduled| &scheduled.payload)
    }

    /// Take the earliest event, with the time it is due at.
    pub fn pop(&mut self) -> Option<(SimTime, T)> {
        self.heap
            .pop()
            .map(|scheduled| (scheduled.at, scheduled.payload))
    }

    /// How many events are pending.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Whether nothing is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn events_come_out_earliest_first() {
        let mut queue = EventQueue::new();
        queue.schedule_at(SimTime::from_millis(30), "third");
        queue.schedule_at(SimTime::from_millis(10), "first");
        queue.schedule_at(SimTime::from_millis(20), "second");

        let order: Vec<&str> = std::iter::from_fn(|| queue.pop().map(|(_, p)| p)).collect();
        assert_eq!(order, ["first", "second", "third"]);
    }

    #[test]
    fn equal_deadlines_keep_insertion_order() {
        // The property the whole determinism claim rests on: two things due at the same instant
        // happen in the order they were scheduled, every run, on every machine.
        let mut queue = EventQueue::new();
        let at = SimTime::from_secs(1);
        for label in ["a", "b", "c", "d"] {
            queue.schedule_at(at, label);
        }
        let order: Vec<&str> = std::iter::from_fn(|| queue.pop().map(|(_, p)| p)).collect();
        assert_eq!(order, ["a", "b", "c", "d"]);
    }

    #[test]
    fn scheduling_after_a_delay_lands_at_the_absolute_time() {
        let mut queue = EventQueue::new();
        let now = SimTime::from_secs(5);
        queue.schedule_after(now, Duration::from_millis(500), ());
        assert_eq!(queue.next_deadline(), Some(SimTime::from_millis(5_500)));
    }

    #[test]
    fn an_empty_queue_has_no_deadline() {
        let queue: EventQueue<()> = EventQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.next_deadline(), None);
    }
}
