//! When to probe what — [e2e-probe §5](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/e2e-probe.md) T5, T6.
//!
//! Pure, like everything else here: time is a parameter and jitter comes from an injected source, so
//! a schedule replays exactly under the harness. A scheduler that read a clock would make every
//! probe scenario a timing test.

use std::time::Duration;

use crate::engine::Target;

/// How often a target is probed, and how much the interval is smeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cadence {
    /// The base interval between runs of one target.
    pub interval: Duration,
    /// How much later than the interval a run may be placed.
    ///
    /// **Jitter is not decoration.** A fleet of testers started by the same rollout would otherwise
    /// synchronize into a spike every interval, and a spike of synthetic traffic is a poor way to
    /// discover that overload control works.
    pub jitter: Duration,
}

impl Cadence {
    /// Every `interval`, smeared by up to a tenth of it.
    ///
    /// The division is on `Duration` rather than through an integer cast: a truncating cast where a
    /// division exists is a paper cut waiting to be copied somewhere it matters.
    #[must_use]
    pub fn every(interval: Duration) -> Self {
        Self {
            interval,
            jitter: interval / 10,
        }
    }
}

/// The bound on how often probes may start (§5 T6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateBound {
    /// At most this many runs may start within `per`.
    pub runs: u32,
    /// The window the bound applies over.
    pub per: Duration,
}

impl RateBound {
    /// A bound that never trips — for a scenario whose subject is not the bound.
    pub const UNBOUNDED: Self = Self {
        runs: u32::MAX,
        per: Duration::from_secs(1),
    };
}

/// A target and when it is next due.
#[derive(Debug, Clone)]
struct Slot {
    target: Target,
    due: Duration,
}

/// Walks the target matrix, one run at a time, within the rate bound.
#[derive(Debug)]
pub struct Scheduler {
    slots: Vec<Slot>,
    cadence: Cadence,
    bound: RateBound,
    /// When runs started, oldest first, trimmed to the bound's window.
    recent_starts: Vec<Duration>,
    /// How many runs the bound has refused. Counted, never silent: a probe that quietly stopped
    /// probing would look exactly like a platform that quietly stopped failing.
    deferred: u64,
}

impl Scheduler {
    /// A scheduler over a target matrix.
    ///
    /// The first run of each target is spread across one interval rather than all at zero, so a node
    /// that just started does not emit the whole matrix at once.
    #[must_use]
    pub fn new(targets: Vec<Target>, cadence: Cadence, bound: RateBound) -> Self {
        let count = u32::try_from(targets.len().max(1)).unwrap_or(1);
        let stride = cadence.interval / count;
        let slots = targets
            .into_iter()
            .enumerate()
            .map(|(index, target)| Slot {
                target,
                due: stride * u32::try_from(index).unwrap_or(0),
            })
            .collect();
        Self {
            slots,
            cadence,
            bound,
            recent_starts: Vec::new(),
            deferred: 0,
        }
    }

    /// How many runs the rate bound has refused so far.
    #[must_use]
    pub fn deferred(&self) -> u64 {
        self.deferred
    }

    /// When the next run is due, if there is a target at all.
    #[must_use]
    pub fn next_due(&self) -> Option<Duration> {
        self.slots.iter().map(|slot| slot.due).min()
    }

    /// Take the target due at or before `now`, if the rate bound allows one.
    ///
    /// `jitter` is drawn from the caller's random source and is what smears the next due time.
    pub fn take_due(
        &mut self,
        now: Duration,
        jitter: &mut impl FnMut(Duration) -> Duration,
    ) -> Option<Target> {
        self.trim(now);

        let index = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.due <= now)
            // Earliest due first, and ties by position, so the matrix is walked in a fixed order
            // rather than in whatever order a hash produced.
            .min_by_key(|(index, slot)| (slot.due, *index))
            .map(|(index, _)| index)?;

        if self.recent_starts.len() >= usize::try_from(self.bound.runs).unwrap_or(usize::MAX) {
            // T6 — over the bound. The slot stays due; it is not dropped, because a skipped target is
            // a blind spot and a delayed one is only late.
            self.deferred += 1;
            return None;
        }

        let slot = self.slots.get_mut(index)?;
        let target = slot.target.clone();
        slot.due = now + self.cadence.interval + jitter(self.cadence.jitter);
        self.recent_starts.push(now);
        Some(target)
    }

    fn trim(&mut self, now: Duration) {
        // Expressed as `started + per > now` rather than `started > now - per`. The subtraction
        // saturates at zero, so early in a run's life the horizon collapsed to zero and dropped the
        // very entry recorded at time zero — the bound then let a second run through immediately,
        // which is the opposite of bounding.
        let window = self.bound.per;
        self.recent_starts.retain(|started| *started + window > now);
    }
}

#[cfg(test)]
// `duration_suboptimal_units` is off for the module rather than per row: every cadence here is
// `from_secs(60)` read against other second-valued quantities in the same assertion — the 59/60
// due boundary, dues spread at 20 and 40 across the interval, 60 + a 6 s jitter budget = 66, and
// loops that step one second at a time. `from_mins(1)` would break each of those comparisons.
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::duration_suboptimal_units
)]
mod tests {
    use super::*;

    fn target(address: &str, transport: &str, zone: &str) -> Target {
        Target {
            address: address.to_owned(),
            transport: transport.to_owned(),
            zone: zone.to_owned(),
        }
    }

    fn matrix() -> Vec<Target> {
        vec![
            target("vip.example", "UDP", "zone-a"),
            target("edge-1.example", "UDP", "zone-a"),
            target("edge-1.example", "TLS", "zone-a"),
        ]
    }

    /// No jitter, for the rows whose subject is not jitter.
    fn no_jitter() -> impl FnMut(Duration) -> Duration {
        |_| Duration::ZERO
    }

    #[test]
    fn the_whole_matrix_is_walked_and_each_target_keeps_its_identity() {
        let mut scheduler = Scheduler::new(
            matrix(),
            Cadence::every(Duration::from_secs(60)),
            RateBound::UNBOUNDED,
        );
        let mut jitter = no_jitter();
        let mut taken = Vec::new();
        for second in 0..60 {
            if let Some(target) = scheduler.take_due(Duration::from_secs(second), &mut jitter) {
                taken.push(target);
            }
        }
        assert_eq!(
            taken.len(),
            3,
            "each target runs once in the first interval"
        );
        // Every verdict must be attributable, which means the address, transport and zone travel
        // with the run rather than being reconstructed later.
        assert!(taken.iter().any(|t| t.transport == "TLS"));
        assert!(taken.iter().all(|t| !t.zone.is_empty()));
    }

    #[test]
    fn the_first_runs_are_spread_across_an_interval_rather_than_all_at_once() {
        // A node that just started must not emit its whole matrix in the same instant — which is
        // exactly what a rollout of many nodes would otherwise produce, all together.
        let scheduler = Scheduler::new(
            matrix(),
            Cadence::every(Duration::from_secs(60)),
            RateBound::UNBOUNDED,
        );
        let dues: Vec<Duration> = scheduler.slots.iter().map(|slot| slot.due).collect();
        assert_eq!(
            dues,
            [
                Duration::ZERO,
                Duration::from_secs(20),
                Duration::from_secs(40)
            ]
        );
    }

    #[test]
    fn a_target_comes_round_again_one_interval_later() {
        let mut scheduler = Scheduler::new(
            vec![target("vip.example", "UDP", "zone-a")],
            Cadence::every(Duration::from_secs(60)),
            RateBound::UNBOUNDED,
        );
        let mut jitter = no_jitter();
        assert!(scheduler.take_due(Duration::ZERO, &mut jitter).is_some());
        assert!(
            scheduler
                .take_due(Duration::from_secs(59), &mut jitter)
                .is_none()
        );
        assert!(
            scheduler
                .take_due(Duration::from_secs(60), &mut jitter)
                .is_some()
        );
    }

    #[test]
    fn jitter_moves_the_next_run_and_comes_from_the_caller() {
        let mut scheduler = Scheduler::new(
            vec![target("vip.example", "UDP", "zone-a")],
            Cadence::every(Duration::from_secs(60)),
            RateBound::UNBOUNDED,
        );
        // A source that always returns its whole budget, so the effect is visible rather than
        // statistical.
        let mut jitter = |budget: Duration| budget;
        scheduler.take_due(Duration::ZERO, &mut jitter);
        assert_eq!(
            scheduler.next_due(),
            Some(Duration::from_secs(66)),
            "60s interval plus the 6s jitter budget"
        );
    }

    #[test]
    fn the_rate_bound_defers_rather_than_dropping() {
        // T6 — a skipped target is a blind spot; a delayed one is only late. And the deferral is
        // counted, because a probe that quietly stopped probing looks exactly like a platform that
        // quietly stopped failing.
        let mut scheduler = Scheduler::new(
            matrix(),
            Cadence::every(Duration::from_secs(60)),
            RateBound {
                runs: 1,
                per: Duration::from_secs(60),
            },
        );
        let mut jitter = no_jitter();

        assert!(scheduler.take_due(Duration::ZERO, &mut jitter).is_some());
        assert!(
            scheduler
                .take_due(Duration::from_secs(20), &mut jitter)
                .is_none(),
            "the bound refuses the second run"
        );
        assert_eq!(scheduler.deferred(), 1);

        // The window slides, and the deferred target is still waiting rather than lost.
        assert!(
            scheduler
                .take_due(Duration::from_secs(61), &mut jitter)
                .is_some(),
            "the deferred target runs as soon as the bound allows"
        );
    }

    #[test]
    fn an_empty_matrix_schedules_nothing_and_does_not_divide_by_zero() {
        let mut scheduler = Scheduler::new(
            Vec::new(),
            Cadence::every(Duration::from_secs(60)),
            RateBound::UNBOUNDED,
        );
        assert_eq!(scheduler.next_due(), None);
        assert!(
            scheduler
                .take_due(Duration::ZERO, &mut no_jitter())
                .is_none()
        );
    }

    #[test]
    fn the_same_jitter_source_produces_the_same_schedule() {
        // The determinism the harness depends on.
        let run = || {
            let mut scheduler = Scheduler::new(
                matrix(),
                Cadence::every(Duration::from_secs(60)),
                RateBound::UNBOUNDED,
            );
            let mut step = 0_u64;
            let mut jitter = move |budget: Duration| {
                step += 1;
                budget / u32::try_from(step).unwrap_or(1)
            };
            let mut taken = Vec::new();
            for second in 0..300 {
                if let Some(target) = scheduler.take_due(Duration::from_secs(second), &mut jitter) {
                    taken.push((second, target.transport.clone()));
                }
            }
            taken
        };
        assert_eq!(run(), run());
    }
}
