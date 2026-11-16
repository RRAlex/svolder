// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU meter's state: a needle and an over lamp for each of the capture's two channels.
//!
//! The engine ([`svolder_host::vu`]) has no clock of its own; this keeps one for the loop: the
//! milliseconds since the meter was made. A block or a frame lands its levels ([`Vu::land`]) and
//! the needles are read at any moment after it ([`Vu::at`]), which integrates a copy of each up to
//! that moment, so a draw never changes what the next block finds. The path of a needle depends
//! only on the levels that landed and when, never on how often it is read.

use std::time::Instant;

use svolder_host::Levels;
use svolder_host::vu::{Lamp, Needle, Reference, Scale};

/// How long a needle goes on reading the last block's level when no new block lands, in
/// milliseconds: past it the input is zero, as the Spectrum's bands fall back when nothing lands.
pub const STALE_MS: u64 = 500;

/// What one channel of the meter shows at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel {
    /// The needle's amplitude ratio to the reference's amplitude, `1.0` at 0 VU.
    pub ratio: f64,
    /// The needle's deflection on the scale: `0.0` at the left stop, `1.0` at +3 VU.
    pub deflection: f64,
    /// Whether the over lamp is lit.
    pub lit: bool,
}

impl Channel {
    /// The needle's reading in VU: minus infinity at rest.
    pub fn reading(&self) -> f64 {
        Scale::reading_db(self.ratio)
    }
}

/// The two channels' needles and lamps.
#[derive(Debug, Clone, Copy)]
pub struct Vu {
    origin: Instant,
    /// The millisecond the last block landed on, `None` before the first.
    last_block: Option<u64>,
    needles: [Needle; 2],
    lamps: [Lamp; 2],
}

impl Vu {
    /// A meter at rest, whose clock starts at `origin`.
    pub fn new(origin: Instant) -> Vu {
        Vu {
            origin,
            last_block: None,
            needles: [Needle::at(0.0); 2],
            lamps: [Lamp::default(); 2],
        }
    }

    /// Both needles back at rest and the lamps dark: nothing is being read.
    pub fn rest(&mut self) {
        *self = Vu::new(self.origin);
    }

    /// `now` on the meter's clock, in milliseconds; a moment before its origin is 0.
    fn ms(&self, now: Instant) -> u64 {
        u64::try_from(now.saturating_duration_since(self.origin).as_millis()).unwrap_or(u64::MAX)
    }

    /// A block's `levels` landed at `now`: each needle runs on to `now` under the level it held,
    /// then holds the new RMS against `reference` until the next block, and a peak at full scale
    /// lights its lamp.
    pub fn land(&mut self, levels: &Levels, reference: Reference, now: Instant) {
        let ms = self.ms(now);
        for channel in 0..2 {
            let mut needle = self.run(channel, ms);
            needle.set(reference.ratio(f64::from(levels.rms[channel])));
            self.needles[channel] = needle;
            self.lamps[channel].push(levels.peak[channel], ms);
        }
        self.last_block = Some(ms);
    }

    /// Needle `channel` run on to `ms` under the level it holds, which is read as zero from
    /// [`STALE_MS`] after the last block.
    fn run(&self, channel: usize, ms: u64) -> Needle {
        let mut needle = self.needles[channel];
        match self.last_block {
            Some(last) if ms > last + STALE_MS => {
                needle.advance_to(last + STALE_MS);
                needle.set(0.0);
                needle.advance_to(ms);
            }
            _ => needle.advance_to(ms),
        }
        needle
    }

    /// Whether anything on the meter is off rest at `now`.
    pub fn moving(&self, now: Instant) -> bool {
        self.at(now)
            .iter()
            .any(|channel| channel.deflection > 0.0 || channel.lit)
    }

    /// What the meter shows at `now`, for a moment after the last block landed.
    pub fn at(&self, now: Instant) -> [Channel; 2] {
        let ms = self.ms(now);
        std::array::from_fn(|channel| {
            let needle = self.run(channel, ms);
            Channel {
                ratio: needle.x(),
                deflection: needle.deflection(),
                lit: self.lamps[channel].lit(ms),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// A block of `rms` and `peak` on both channels.
    fn levels(rms: f32, peak: f32) -> Levels {
        Levels {
            rms: [rms, rms],
            peak: [peak, peak],
        }
    }

    /// The amplitude of a level `db` dBFS.
    fn amp(db: f32) -> f32 {
        10f32.powf(db / 20.0)
    }

    fn after(origin: Instant, ms: u64) -> Instant {
        origin + Duration::from_millis(ms)
    }

    #[test]
    fn a_meter_at_rest_reads_nothing_and_shows_no_lamp() {
        let origin = Instant::now();
        let vu = Vu::new(origin);
        for channel in vu.at(after(origin, 5000)) {
            assert_eq!(channel.deflection, 0.0);
            assert_eq!(channel.reading(), f64::NEG_INFINITY);
            assert!(!channel.lit);
        }
    }

    #[test]
    fn a_level_at_the_reference_settles_on_zero_vu() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        let reference = Reference::new(-18);
        // 30 Hz blocks of a sine whose RMS is -18 dBFS, for one second.
        for block in 0..=30u64 {
            vu.land(
                &levels(amp(-18.0), amp(-15.0)),
                reference,
                after(origin, block * 33),
            );
        }
        for channel in vu.at(after(origin, 1000)) {
            assert!(channel.reading().abs() < 0.05, "{}", channel.reading());
            assert!(!channel.lit);
        }
    }

    #[test]
    fn the_reference_moves_the_reading_by_the_same_decibels() {
        let origin = Instant::now();
        for (reference, want) in [(-30, 12.0), (-6, -12.0)] {
            let mut vu = Vu::new(origin);
            for block in 0..=60u64 {
                vu.land(
                    &levels(amp(-18.0), amp(-15.0)),
                    Reference::new(reference),
                    after(origin, block * 33),
                );
            }
            let at = vu.at(after(origin, 2000))[0];
            match want > 3.2 {
                // +12 VU is far past the stop: the needle rests on it.
                true => assert!((at.deflection - Scale::STOP).abs() < 1e-9, "{at:?}"),
                false => assert!((at.reading() - want).abs() < 0.05, "{at:?}"),
            }
        }
    }

    #[test]
    fn the_lamp_lights_on_a_full_scale_peak_and_holds_for_1200_ms() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        let reference = Reference::default();
        vu.land(&levels(0.1, 0.5), reference, after(origin, 100));
        assert!(!vu.at(after(origin, 100))[0].lit);
        vu.land(&levels(0.1, 1.0), reference, after(origin, 200));
        assert!(vu.at(after(origin, 200))[0].lit);
        assert!(vu.at(after(origin, 1400))[0].lit, "still lit at 1200 ms");
        assert!(!vu.at(after(origin, 1401))[0].lit, "dark at 1201 ms");
        vu.land(&levels(0.1, 1.0), reference, after(origin, 2000));
        assert!(
            vu.at(after(origin, 2000))[1].lit,
            "an over again re-arms it"
        );
    }

    #[test]
    fn the_channels_run_apart() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        let loud = Levels {
            rms: [amp(-18.0), 0.0],
            peak: [1.0, 0.0],
        };
        for block in 0..=30u64 {
            vu.land(&loud, Reference::default(), after(origin, block * 33));
        }
        let [left, right] = vu.at(after(origin, 1000));
        assert!(left.deflection > 0.6 && left.lit);
        assert_eq!(right.deflection, 0.0);
        assert!(!right.lit);
    }

    #[test]
    fn reading_the_meter_never_changes_what_it_shows_later() {
        let origin = Instant::now();
        let mut read = Vu::new(origin);
        let mut left_alone = Vu::new(origin);
        for vu in [&mut read, &mut left_alone] {
            vu.land(
                &levels(amp(-18.0), 0.5),
                Reference::default(),
                after(origin, 10),
            );
        }
        for ms in (10..400).step_by(7) {
            let _ = read.at(after(origin, ms));
        }
        assert_eq!(
            read.at(after(origin, 400)),
            left_alone.at(after(origin, 400))
        );
    }

    #[test]
    fn the_path_does_not_depend_on_how_often_blocks_land() {
        let origin = Instant::now();
        let mut slow = Vu::new(origin);
        let mut fast = Vu::new(origin);
        let held = levels(amp(-18.0), 0.5);
        for at in [0u64, 100, 200, 300] {
            slow.land(&held, Reference::default(), after(origin, at));
        }
        for at in (0u64..=300).step_by(21) {
            fast.land(&held, Reference::default(), after(origin, at));
        }
        let (slow, fast) = (slow.at(after(origin, 300)), fast.at(after(origin, 300)));
        assert!(
            (slow[0].ratio - fast[0].ratio).abs() < 1e-9,
            "{slow:?} {fast:?}"
        );
    }

    #[test]
    fn rest_puts_the_needles_back_and_darkens_the_lamps() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        vu.land(&levels(0.5, 1.0), Reference::default(), after(origin, 50));
        vu.rest();
        for channel in vu.at(after(origin, 60)) {
            assert_eq!(channel.deflection, 0.0);
            assert!(!channel.lit);
        }
    }

    #[test]
    fn a_needle_with_no_block_for_500_ms_reads_its_input_as_zero_and_falls() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        for block in 0..=60u64 {
            vu.land(
                &levels(amp(-18.0), 0.5),
                Reference::default(),
                after(origin, block * 33),
            );
        }
        let last = 60 * 33;
        // Within the window the needle holds its level.
        assert!(vu.at(after(origin, last + 500))[0].deflection > 0.6);
        // After it the needle falls to rest, and a long stall leaves it there.
        assert_eq!(vu.at(after(origin, last + 5_000))[0].deflection, 0.0);
        assert_eq!(vu.at(after(origin, last + 600_000))[0].deflection, 0.0);
        // A block that lands after the stall starts from the fallen needle.
        vu.land(
            &levels(amp(-18.0), 0.5),
            Reference::default(),
            after(origin, last + 5_000),
        );
        assert!(vu.at(after(origin, last + 5_001))[0].deflection < 0.1);
    }

    #[test]
    fn the_meter_moves_while_a_needle_is_off_rest_or_a_lamp_is_lit_and_no_longer() {
        let origin = Instant::now();
        let mut vu = Vu::new(origin);
        assert!(!vu.moving(after(origin, 10)), "at rest");
        vu.land(&levels(0.0, 0.0), Reference::default(), after(origin, 10));
        assert!(!vu.moving(after(origin, 20)), "silence");
        for block in 0..=30u64 {
            vu.land(
                &levels(amp(-18.0), 0.5),
                Reference::default(),
                after(origin, 30 + block * 33),
            );
        }
        assert!(vu.moving(after(origin, 1100)), "a needle off rest");
        // Silence: the needle falls to exactly rest within a second, and the meter settles.
        let mut at = 1100;
        vu.land(&levels(0.0, 0.0), Reference::default(), after(origin, at));
        while vu.moving(after(origin, at)) {
            at += 33;
            assert!(at < 3000, "the needle never came to rest");
            vu.land(&levels(0.0, 0.0), Reference::default(), after(origin, at));
        }
        // A lit lamp moves it, for 1200 ms.
        vu.land(&levels(0.0, 1.0), Reference::default(), after(origin, 4000));
        assert!(vu.moving(after(origin, 4000 + 1200)));
        assert!(!vu.moving(after(origin, 4000 + 1201)));
    }
}
