//! Virtual time.
//!
//! The clock exists only inside the scheduler. Nothing below it can read a clock — sans-IO
//! components take fired timers by contract — so "what time is it" has exactly one answer per
//! simulation, and that answer moves only when the scheduler pops an event.

use std::fmt;
use std::time::Duration;

/// Nanoseconds since the scenario started.
///
/// Not a wall clock and deliberately not convertible to one: a simulation that could ask the
/// operating system what time it is would be a simulation whose results depend on the machine
/// that ran it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SimTime(u64);

impl SimTime {
    /// The instant every scenario starts at.
    pub const START: Self = Self(0);

    /// A time this many nanoseconds after the start.
    #[must_use]
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// A time this many milliseconds after the start.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis.saturating_mul(1_000_000))
    }

    /// A time this many seconds after the start.
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs.saturating_mul(1_000_000_000))
    }

    /// Nanoseconds since the start.
    #[must_use]
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// This instant plus a duration.
    ///
    /// Saturating rather than wrapping or panicking: a scenario that schedules something 600
    /// years out has a bug, and the useful behaviour is for it to land at the end of time where
    /// an assertion can see it — not to wrap around to the past, and not to take the process
    /// down.
    #[must_use]
    pub fn saturating_add(self, after: Duration) -> Self {
        let nanos = u64::try_from(after.as_nanos()).unwrap_or(u64::MAX);
        Self(self.0.saturating_add(nanos))
    }

    /// How long from `earlier` to here; zero if `earlier` is later.
    #[must_use]
    pub fn since(self, earlier: Self) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }
}

impl fmt::Display for SimTime {
    /// Fixed width to the microsecond, so traces from two runs line up column for column when
    /// something diverges and a diff is the fastest way to see where.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let micros = self.0 / 1_000;
        write!(f, "{:>7}.{:06}", micros / 1_000_000, micros % 1_000_000)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn display_is_fixed_width_so_traces_diff_cleanly() {
        assert_eq!(SimTime::START.to_string(), "      0.000000");
        assert_eq!(SimTime::from_millis(1_500).to_string(), "      1.500000");
        assert_eq!(SimTime::from_secs(31).to_string(), "     31.000000");
    }

    #[test]
    fn an_absurd_duration_saturates_rather_than_wrapping() {
        let far = SimTime::from_nanos(u64::MAX - 5);
        assert_eq!(
            far.saturating_add(Duration::from_secs(1)).as_nanos(),
            u64::MAX
        );
    }

    #[test]
    fn since_is_zero_when_the_other_instant_is_later() {
        assert_eq!(SimTime::START.since(SimTime::from_secs(9)), Duration::ZERO);
    }
}
