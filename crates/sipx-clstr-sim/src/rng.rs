//! Seeded randomness, in named streams.
//!
//! One `u64` master seed per scenario. Every consumer of randomness draws from a stream named by
//! a stable label — `"link:alice->vip"`, `"node:edge-1:branch"` — derived by hashing the label
//! together with the master seed. Adding a stream therefore never reshuffles the others, so a
//! scenario edit does not silently invalidate the seeds of the scenarios it did not touch.
//!
//! # Why the algorithm is written out here
//!
//! The harness's central promise is *same scenario, same seed, byte-identical trace*. A promise
//! like that cannot rest on a dependency's internals: `rand`'s own documentation is explicit that
//! its general-purpose generators may change output between versions, and a cargo update that
//! silently reshuffles every recorded seed would break the one property the whole design is for.
//!
//! So the generator is here, in full: `SplitMix64` to expand a seed into state, xoshiro256\*\* to
//! produce it. Both are small, public-domain, and pinned by the test vectors below. This is
//! harness machinery, not protocol logic — nothing here belongs in the kernel, which already
//! takes randomness as an injected source.

use std::num::Wrapping;

/// One step of `SplitMix64`, advancing `state` and returning the mixed word.
fn split_mix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut x = *state;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A named, reproducible stream of random numbers.
#[derive(Debug, Clone)]
pub struct SimRng {
    state: [u64; 4],
}

impl SimRng {
    /// The stream a given label draws from, under a given master seed.
    ///
    /// The label is hashed with FNV-1a — spelled out rather than taken from the standard library,
    /// because `DefaultHasher`'s output is explicitly not guaranteed across Rust releases and a
    /// toolchain upgrade must not change what a seed means.
    #[must_use]
    pub fn stream(master: u64, label: &str) -> Self {
        let mut hash = Wrapping(0xcbf2_9ce4_8422_2325_u64);
        for byte in label.as_bytes() {
            hash ^= Wrapping(u64::from(*byte));
            hash *= Wrapping(0x0000_0100_0000_01b3_u64);
        }
        Self::from_seed(master ^ hash.0)
    }

    /// A stream from a raw seed, with no label.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        // SplitMix64 expands one word into four that are not obviously related. Seeding
        // xoshiro's 256-bit state directly from a small integer leaves it correlated for the
        // first few outputs, which for us would mean scenarios with seeds 1 and 2 behaving
        // suspiciously alike.
        let mut z = seed;
        Self {
            state: [
                split_mix64(&mut z),
                split_mix64(&mut z),
                split_mix64(&mut z),
                split_mix64(&mut z),
            ],
        }
    }

    /// The next value: xoshiro256\*\*.
    ///
    /// The order of the state update is the algorithm, not an implementation detail — each step
    /// reads what the step before it wrote. Written as sequential assignments rather than one
    /// array literal for exactly that reason: the literal form computes every element from the
    /// *old* state and quietly produces a different, weaker generator.
    pub fn next_u64(&mut self) -> u64 {
        let [mut s0, mut s1, mut s2, mut s3] = self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s1 << 17;
        s2 ^= s0;
        s3 ^= s1;
        s1 ^= s2;
        s0 ^= s3;
        s2 ^= t;
        s3 = s3.rotate_left(45);
        self.state = [s0, s1, s2, s3];
        result
    }

    /// A value in `[0, 1)`.
    ///
    /// The top 53 bits scaled by 2⁻⁵³ — the exact-in-binary construction, so the same draw is the
    /// same float on every platform. Anything involving a division by `u64::MAX` would not be.
    pub fn unit(&mut self) -> f64 {
        // Precision is deliberate, not accidental: 53 bits is exactly an f64 mantissa.
        #[allow(clippy::cast_precision_loss)]
        let scaled = (self.next_u64() >> 11) as f64;
        scaled * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Whether an event of this probability happens.
    ///
    /// `probability <= 0.0` never happens and `>= 1.0` always does, without drawing — so a link
    /// configured clean consumes no randomness at all, and adding a lossless link to a topology
    /// cannot shift the draws of the links around it.
    pub fn chance(&mut self, probability: f64) -> bool {
        if probability <= 0.0 {
            return false;
        }
        if probability >= 1.0 {
            return true;
        }
        self.unit() < probability
    }

    /// A value in `[low, high]`, inclusive at both ends.
    ///
    /// Rejection-sampled rather than reduced modulo the range: the modulo shortcut biases the low
    /// end of the range, which in a latency distribution is the difference between "jitter" and
    /// "jitter that quietly prefers to be small".
    pub fn range_inclusive(&mut self, low: u64, high: u64) -> u64 {
        if high <= low {
            return low;
        }
        let span = high - low;
        if span == u64::MAX {
            return self.next_u64();
        }
        let bound = span + 1;
        let limit = u64::MAX - (u64::MAX % bound);
        loop {
            let draw = self.next_u64();
            if draw < limit {
                return low + (draw % bound);
            }
        }
    }
}

impl rand::RngCore for SimRng {
    fn next_u32(&mut self) -> u32 {
        // The high bits of xoshiro256** are the strong ones.
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.next_u64() >> 32) as u32
        }
    }

    fn next_u64(&mut self) -> u64 {
        Self::next_u64(self)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        let mut chunks = dst.chunks_exact_mut(8);
        for chunk in &mut chunks {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes());
        }
        let tail = chunks.into_remainder();
        if !tail.is_empty() {
            let bytes = self.next_u64().to_le_bytes();
            for (out, byte) in tail.iter_mut().zip(bytes.iter()) {
                *out = *byte;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// `SplitMix64`'s published first output for seed 0. Pins the seeding half against the
    /// reference rather than against whatever this file happens to compute.
    #[test]
    fn split_mix_matches_the_reference() {
        let mut state = 0_u64;
        assert_eq!(split_mix64(&mut state), 0xe220_a839_7b1d_cdaf);
    }

    /// xoshiro256\*\* from state `[1, 2, 3, 4]`, worked out by hand from the reference:
    ///
    /// - output = rotl(2·5, 7)·9 = 1280·9 = **11520**
    /// - then `t = 2<<17`; `s2 ^= s0` → 2; `s3 ^= s1` → 6; `s1 ^= s2` → **0**; `s0 ^= s3` → 7;
    ///   `s2 ^= t` → 262146; `s3 = rotl(6, 45)` → 6·2⁴⁵
    /// - so the next output is rotl(0·5, 7)·9 = **0**
    ///
    /// The second output is the part that matters: it is zero only if each step read what the
    /// step before it wrote. An implementation that computes the new state from the old state
    /// throughout — the natural way to write it, and the way this file first did — produces a
    /// different generator that passes every statistical smell test and no longer *is* xoshiro.
    #[test]
    fn xoshiro_matches_the_reference_including_the_update_order() {
        let mut rng = SimRng {
            state: [1, 2, 3, 4],
        };
        assert_eq!(rng.next_u64(), 11_520);
        assert_eq!(rng.state, [7, 0, 262_146, 6 << 45]);
        assert_eq!(rng.next_u64(), 0);
    }

    /// With both halves pinned above, this locks the wiring between them. If it changes, every
    /// recorded seed in the suite now means something different, and that must be a deliberate
    /// act with a changelog entry rather than a surprise from a dependency bump.
    #[test]
    fn the_generator_is_pinned() {
        let mut rng = SimRng::from_seed(0x5eed);
        let drawn: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
        assert_eq!(
            drawn,
            vec![
                17_236_385_663_644_093_300,
                16_282_079_530_828_760_347,
                15_612_578_460_299_724_346,
                17_980_025_521_064_999_683,
            ]
        );
    }

    #[test]
    fn a_stream_is_a_function_of_its_label() {
        let a = SimRng::stream(7, "link:alice->vip").next_u64();
        let b = SimRng::stream(7, "link:alice->vip").next_u64();
        let c = SimRng::stream(7, "link:bob->vip").next_u64();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn the_master_seed_moves_every_stream() {
        let a = SimRng::stream(1, "node:edge-0:branch").next_u64();
        let b = SimRng::stream(2, "node:edge-0:branch").next_u64();
        assert_ne!(a, b);
    }

    #[test]
    fn certain_outcomes_consume_no_randomness() {
        // Adding a clean link to a topology must not shift the draws of every other link.
        let mut rng = SimRng::from_seed(3);
        assert!(!rng.chance(0.0));
        assert!(rng.chance(1.0));
        let after_certainties = rng.clone().next_u64();
        assert_eq!(after_certainties, SimRng::from_seed(3).next_u64());
    }

    #[test]
    fn a_degenerate_range_is_its_own_bound() {
        let mut rng = SimRng::from_seed(11);
        assert_eq!(rng.range_inclusive(20, 20), 20);
        assert_eq!(rng.range_inclusive(30, 10), 30);
    }

    #[test]
    fn a_range_covers_both_ends_and_stays_inside_them() {
        let mut rng = SimRng::from_seed(99);
        let mut low = false;
        let mut high = false;
        for _ in 0..2_000 {
            let value = rng.range_inclusive(5, 8);
            assert!((5..=8).contains(&value), "{value} outside 5..=8");
            low |= value == 5;
            high |= value == 8;
        }
        assert!(low && high, "range never reached one of its bounds");
    }

    #[test]
    fn unit_stays_inside_the_half_open_interval() {
        let mut rng = SimRng::from_seed(1234);
        for _ in 0..10_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value), "{value} outside [0, 1)");
        }
    }

    #[test]
    fn fill_bytes_fills_every_byte_including_a_partial_tail() {
        let mut rng = SimRng::from_seed(5);
        let mut buffer = [0_u8; 13];
        rand::RngCore::fill_bytes(&mut rng, &mut buffer);
        assert!(buffer.iter().any(|b| *b != 0));
        // The tail is the interesting part: a chunks_exact loop that forgets it leaves the last
        // five bytes zero, which looks like randomness until something depends on them.
        assert!(buffer[8..].iter().any(|b| *b != 0));
    }
}
