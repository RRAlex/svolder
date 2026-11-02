// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The intro's own randomness: a SplitMix64 seeded once per launch, so one seed replays a whole
//! intro and its exit, and a test can fix it.

use std::time::{SystemTime, UNIX_EPOCH};

/// A SplitMix64 generator.
#[derive(Debug, Clone)]
pub(in crate::tui) struct Rng(u64);

impl Rng {
    /// A generator that draws the numbers `seed` gives.
    pub(in crate::tui) fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// The next 64 bits.
    pub(in crate::tui) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0.0..1.0`.
    pub(in crate::tui) fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A number in `-1.0..1.0`.
    pub(in crate::tui) fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// A seed for this launch: the clock's nanoseconds mixed with the process id, so two launches
/// in the same instant still scatter differently.
pub fn launch_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    Rng::new(nanos ^ (u64::from(std::process::id()) << 32)).next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_seed_draws_the_same_numbers_and_another_seed_others() {
        let draw = |seed| {
            let mut rng = Rng::new(seed);
            (0..8).map(|_| rng.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(draw(7), draw(7));
        assert_ne!(draw(7), draw(8));
    }

    #[test]
    fn unit_and_signed_stay_in_their_ranges() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            assert!((0.0..1.0).contains(&rng.unit()));
            assert!((-1.0..1.0).contains(&rng.signed()));
        }
    }
}
