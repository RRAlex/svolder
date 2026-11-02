// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A stand-in for music: a level per channel, for the fake device's meters to play.

use std::time::Duration;

/// How often a signal is read onto a device's meters: ten times a second, fast enough for a
/// front end to draw a bar that moves and slow enough to leave the link to everything else.
pub const SIGNAL_PERIOD: Duration = Duration::from_millis(100);

/// A piece of imaginary music, as a level between silence and full scale over time.
///
/// It is a function of the clock and nothing else, so two readers with the same seed hear the
/// same thing and a test can assert on any moment of it.
pub struct Signal {
    seed: u64,
}

impl Signal {
    /// A signal of its own, told apart from every other by `seed`.
    pub fn new(seed: u64) -> Signal {
        Signal { seed }
    }

    /// The level of `channel` at `t`: a slow contour a few seconds long (the phrase), a
    /// faster tremble on it, quiet passages every so often, and the two channels a little
    /// apart, all of it between 0.0 and 1.0.
    pub fn at(&self, t: Duration, channel: usize) -> f64 {
        let s = t.as_secs_f64() + channel as f64 * 0.23;
        let phrase = 0.5 + 0.5 * (s * 0.9 + self.noise(s.floor() as u64)).sin();
        let tremble = 0.5 + 0.5 * (s * 7.3).sin();
        let rest = if ((s / 6.0).floor() as u64 + self.seed).is_multiple_of(4) {
            0.08
        } else {
            1.0
        };
        (rest * (0.15 + 0.85 * phrase) * (0.7 + 0.3 * tremble)).clamp(0.0, 1.0)
    }

    /// A number between 0.0 and 1.0 for the whole second `k`, from a seeded xorshift: enough
    /// to keep one phrase from sounding like the next, and no new dependency to carry.
    fn noise(&self, k: u64) -> f64 {
        let mut x = k ^ self.seed ^ 0x9E37_79B9_7F4A_7C15;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % 1000) as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The signal stays on the scale a meter can read, says the same thing twice for the
    /// same seed and moment, keeps the two channels apart, and goes quiet often enough for a
    /// front end following it to be seen settling.
    #[test]
    fn the_signal_is_bounded_deterministic_and_the_channels_differ() {
        let s = Signal::new(7);
        for ms in (0..20_000).step_by(100) {
            let t = Duration::from_millis(ms);
            for ch in 0..2 {
                let v = s.at(t, ch);
                assert!((0.0..=1.0).contains(&v), "{v} at {ms} ms");
            }
        }
        assert_eq!(
            s.at(Duration::from_millis(3_300), 0),
            Signal::new(7).at(Duration::from_millis(3_300), 0)
        );
        assert_ne!(
            s.at(Duration::from_millis(3_300), 0),
            s.at(Duration::from_millis(3_300), 1)
        );
        let quiet = (0..200)
            .filter(|i| s.at(Duration::from_millis(i * 100), 0) < 0.1)
            .count();
        assert!(quiet > 10, "there are quiet passages: {quiet}");
    }
}
