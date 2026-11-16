// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU meter's rules, pure and with no clock of their own: the [`Scale`] a needle reads, the
//! [`Reference`] that sets which level is 0 VU, the [`Needle`]'s ballistics and the over [`Lamp`].
//! The web page runs the same rules in its own model; [`golden`] gives the needle's path for a
//! fixed script (the file `scripts/web-checks/fixtures/vu-ballistics.json`, made by
//! `crates/svolder-host/examples/vu_golden.rs`) and both engines have to follow it.
//!
//! Amplitudes are 1.0 at full scale. A level is first divided by the reference's amplitude
//! ([`Reference::ratio`]), so a level equal to the reference is a ratio of 1.0, 0 VU.
//!
//! The needle is a second-order system, `x'' + 2ζω x' + ω² x = ω² u`, `u` being the latest ratio,
//! held until the next one comes. It is integrated in fixed [`STEP_MS`] steps on a whole-millisecond
//! clock the caller passes in, so its path depends only on `u(t)`, never on how often a frame
//! arrives or the screen is drawn. A step from 0 to 1 first reaches 0.99 at about 300 ms and
//! overshoots by about 0.85 %; a step from 1 to 0 is under 0.01 at about 300 ms. It rests on a stop
//! at each end of the scale and leaves it at once; a gap over [`MAX_CATCH_UP_MS`] snaps it to steady
//! state.
//!
//! The lamp reads the sample peak, so a hard-limited master whose source never touched full
//! scale can still light it: the tap's resampling (44.1 to 48 kHz) overshoots square content by
//! up to 1.9 dB.

use crate::analysis::OVER;

/// The needle's integration step, in milliseconds.
pub const STEP_MS: u64 = 1;

/// The needle's natural frequency, in radians per second, fitted with [`ZETA`] to the 300 ms rise.
const OMEGA: f64 = 14.2;

/// The needle's damping ratio.
const ZETA: f64 = 0.83;

/// The longest gap, in milliseconds, [`Needle::advance_to`] integrates through. A longer one (a
/// clock that starts far from the needle's, a resume after sleep) puts the needle at its steady
/// state at once: after this long the system's residual is about 6e-11.
pub const MAX_CATCH_UP_MS: u64 = 2000;

/// How long, in milliseconds, the lamp stays lit after the last over.
pub const HOLD_MS: u64 = 1200;

/// The scale the needle reads: VU -20 to +3 on its amplitude ratio.
#[derive(Debug, Clone, Copy)]
pub struct Scale;

impl Scale {
    /// The deflection past which the needle rests on its stop.
    pub const STOP: f64 = 1.03;
    /// The ratio at the left end of the scale, -20 VU.
    const LOW: f64 = 0.1;
    /// The amplitude ratio at the right stop: the needle's state never goes past it.
    pub const STOP_RATIO: f64 = 0.1 + 1.03 * (1.4125375446227544 - 0.1);

    /// The needle's deflection for an amplitude ratio: 0 at -20 VU, 1 at +3 VU, linear in
    /// amplitude between, clamped to 0 and [`Scale::STOP`]. Not a number is 0.
    pub fn deflection(ratio: f64) -> f64 {
        let top = 10f64.powf(3.0 / 20.0);
        let f = (ratio - Self::LOW) / (top - Self::LOW);
        if f.is_nan() {
            0.0
        } else {
            f.clamp(0.0, Self::STOP)
        }
    }

    /// The reading in VU of an amplitude ratio: `20·log10`; 0, a negative ratio (a needle that
    /// dips below rest) or not a number is minus infinity.
    pub fn reading_db(ratio: f64) -> f64 {
        if ratio.is_nan() || ratio <= 0.0 {
            return f64::NEG_INFINITY;
        }
        20.0 * ratio.log10()
    }
}

/// The level, in dBFS, that reads 0 VU, a whole number from [`Reference::MIN`] to
/// [`Reference::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reference(i8);

impl Reference {
    /// The lowest reference, in dBFS.
    pub const MIN: i8 = -30;
    /// The highest reference, in dBFS.
    pub const MAX: i8 = -6;

    /// The reference of `db` dBFS, clamped to the range (a wide number is clamped before it is
    /// narrowed, so it cannot wrap).
    pub fn new(db: i32) -> Self {
        Self(db.clamp(i32::from(Self::MIN), i32::from(Self::MAX)) as i8)
    }

    /// The reference in dBFS.
    pub fn db(self) -> i8 {
        self.0
    }

    /// The ratio of an RMS amplitude (1.0 = full scale) to the reference's amplitude.
    pub fn ratio(self, rms: f64) -> f64 {
        rms / 10f64.powf(f64::from(self.0) / 20.0)
    }
}

impl Default for Reference {
    fn default() -> Self {
        Self(-18)
    }
}

/// A needle: the state of the second-order system and the millisecond it was last advanced to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Needle {
    x: f64,
    v: f64,
    u: f64,
    ms: u64,
}

impl Needle {
    /// A needle at rest at `x`, at 0 ms on whatever clock the caller keeps (the first
    /// [`advance_to`](Self::advance_to) further than [`MAX_CATCH_UP_MS`] away snaps), with `x` as
    /// its input.
    pub fn at(x: f64) -> Self {
        Self {
            x,
            v: 0.0,
            u: x,
            ms: 0,
        }
    }

    /// The latest input ratio, held until the next call. Not a number or negative is 0.
    pub fn set(&mut self, u: f64) {
        self.u = if u.is_finite() { u.max(0.0) } else { 0.0 };
    }

    /// Integrate up to `ms` on the caller's clock (a time not after the last one does nothing).
    /// The needle rests on a stop at either end of the scale (0 and [`Scale::STOP_RATIO`]) and
    /// leaves it as soon as the input pulls it away. A gap over [`MAX_CATCH_UP_MS`] puts it at
    /// the input, held inside the stops, at rest.
    pub fn advance_to(&mut self, ms: u64) {
        if ms.saturating_sub(self.ms) > MAX_CATCH_UP_MS {
            self.x = self.u.clamp(0.0, Scale::STOP_RATIO);
            self.v = 0.0;
            self.ms = ms;
            return;
        }
        let dt = STEP_MS as f64 / 1000.0;
        while self.ms + STEP_MS <= ms {
            self.v += dt * (OMEGA * OMEGA * (self.u - self.x) - 2.0 * ZETA * OMEGA * self.v);
            self.x += dt * self.v;
            if self.x < 0.0 || self.x > Scale::STOP_RATIO {
                self.x = self.x.clamp(0.0, Scale::STOP_RATIO);
                self.v = 0.0;
            }
            self.ms += STEP_MS;
        }
    }

    /// Where the needle is, as an amplitude ratio, between the stops.
    pub fn x(&self) -> f64 {
        self.x
    }

    /// The needle's deflection on the [`Scale`].
    pub fn deflection(&self) -> f64 {
        Scale::deflection(self.x)
    }
}

/// The over lamp: lit from a frame whose peak is [`OVER`] or more until [`HOLD_MS`] after the
/// last such frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lamp {
    last_over: Option<u64>,
}

impl Lamp {
    /// A frame's sample peak (amplitude, 1.0 = full scale) at `ms`.
    pub fn push(&mut self, peak: f32, ms: u64) {
        if peak >= OVER {
            self.last_over = Some(ms);
        }
    }

    /// Whether the lamp is lit at `ms`.
    pub fn lit(&self, ms: u64) -> bool {
        self.last_over
            .is_some_and(|last| ms >= last && ms - last <= HOLD_MS)
    }
}

/// One point of the golden path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The time, in ms.
    pub t: u64,
    /// The needle's ratio.
    pub x: f64,
    /// Its deflection on the [`Scale`].
    pub f: f64,
}

/// The golden script and the path the needle takes through it.
#[derive(Debug, Clone, PartialEq)]
pub struct Golden {
    /// The frames: when each arrives and its input ratio, in time order.
    pub events: Vec<(u64, f64)>,
    /// The needle every [`SAMPLE_MS`], after the events up to and including that time.
    pub samples: Vec<Sample>,
}

/// The golden path's sample spacing, in ms.
pub const SAMPLE_MS: u64 = 10;

/// The golden path's length, in ms.
pub const GOLDEN_MS: u64 = 8000;

/// The needle's path for a fixed script: silence, a level at the reference, a +6 dB burst, a
/// decay, a pinned level and silence again, with frames at 30 Hz, then 47 Hz, then at irregular
/// times. Each sample is taken after the events up to its time: advance to the event's time, set
/// its input, and at the end advance to the sample's time.
pub fn golden() -> Golden {
    let reference = Reference::default();
    let level = |dbfs: f64| reference.ratio(10f64.powf(dbfs / 20.0));
    // (from, to, level in dBFS RMS, frame gaps in ms)
    let script: [(u64, u64, Option<f64>, &[u64]); 7] = [
        (0, 1000, None, &[33, 33, 34]),
        (1000, 3000, Some(-18.0), &[33, 33, 34]),
        (3000, 3600, Some(-12.0), &[21, 21, 22]),
        (3600, 4600, Some(-24.0), &[21, 21, 22]),
        (4600, 5600, Some(-3.0), &[5, 40, 17, 63, 9, 28]),
        (5600, 6600, Some(-30.0), &[5, 40, 17, 63, 9, 28]),
        (6600, GOLDEN_MS, None, &[33, 33, 34]),
    ];
    let mut events = Vec::new();
    for (from, to, dbfs, gaps) in script {
        let (mut t, mut i) = (from, 0);
        while t < to {
            // A small, fixed wobble so the input is not a constant.
            let wobble = 1.0 + 0.1 * (((t / 7) % 5) as f64 - 2.0) / 2.0;
            events.push((t, dbfs.map_or(0.0, |d| level(d) * wobble)));
            t += gaps[i % gaps.len()];
            i += 1;
        }
    }
    let mut needle = Needle::at(0.0);
    let mut next = 0;
    let mut samples = Vec::new();
    for t in (0..=GOLDEN_MS).step_by(SAMPLE_MS as usize) {
        while next < events.len() && events[next].0 <= t {
            needle.advance_to(events[next].0);
            needle.set(events[next].1);
            next += 1;
        }
        needle.advance_to(t);
        samples.push(Sample {
            t,
            x: needle.x(),
            f: needle.deflection(),
        });
    }
    Golden { events, samples }
}

impl Golden {
    /// The golden file's text: licence keys first, then the events and the samples, numbers to
    /// seven places.
    pub fn json(&self) -> String {
        let events: Vec<String> = self
            .events
            .iter()
            .map(|(t, u)| format!("[{t},{u:.7}]"))
            .collect();
        let samples: Vec<String> = self
            .samples
            .iter()
            .map(|s| format!("[{},{:.7},{:.7}]", s.t, s.x, s.f))
            .collect();
        format!(
            "{{\n  \"licence_1\": \"SPDX-FileCopyrightText: 2026 the svolder authors\",\n  \
             \"licence_2\": \"SPDX-License-Identifier: GPL-3.0-or-later\",\n  \
             \"step_ms\": {STEP_MS},\n  \"sample_ms\": {SAMPLE_MS},\n  \
             \"omega\": {OMEGA},\n  \"zeta\": {ZETA},\n  \
             \"stop_ratio\": {},\n  \"max_catch_up_ms\": {MAX_CATCH_UP_MS},\n  \
             \"events\": [{}],\n  \"samples\": [{}]\n}}\n",
            Scale::STOP_RATIO,
            events.join(","),
            samples.join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first millisecond `needle`, fed `from` then `to` as of 0 ms, reads `pred`.
    fn first_ms(from: f64, to: f64, pred: impl Fn(f64) -> bool) -> u64 {
        let mut needle = Needle::at(from);
        needle.set(to);
        (1..2000)
            .find(|&ms| {
                needle.advance_to(ms);
                pred(needle.x())
            })
            .expect("never reached")
    }

    #[test]
    fn a_step_up_reaches_99_percent_at_300_ms_and_overshoots_a_little() {
        let ms = first_ms(0.0, 1.0, |x| x >= 0.99);
        assert!((285..=315).contains(&ms), "{ms}");
        let mut needle = Needle::at(0.0);
        needle.set(1.0);
        let mut top = 0.0f64;
        for ms in 1..2000 {
            needle.advance_to(ms);
            top = top.max(needle.x());
        }
        assert!((1.005..=1.015).contains(&top), "{top}");
    }

    #[test]
    fn a_step_down_reaches_1_percent_at_300_ms() {
        let ms = first_ms(1.0, 0.0, |x| x <= 0.01);
        assert!((285..=315).contains(&ms), "{ms}");
    }

    #[test]
    fn the_path_depends_on_the_input_not_on_how_often_it_is_read_or_fed() {
        let mut dense = Needle::at(0.0);
        dense.set(0.8);
        let mut sparse = Needle::at(0.0);
        sparse.set(0.8);
        let mut at = 0;
        for ms in 1..=1000u64 {
            dense.advance_to(ms);
            // Fed again at 30 Hz, 47 Hz and irregular times with the same value: no change.
            if ms % 33 == 0 || ms % 21 == 0 || ms % 7 == 3 {
                dense.set(0.8);
            }
            if ms % 250 == 0 {
                sparse.advance_to(ms);
                at += 1;
                assert_eq!(dense.x(), sparse.x(), "{ms}");
            }
        }
        assert_eq!(at, 4);
    }

    #[test]
    fn the_scale_maps_vu_to_deflection() {
        let ratio = |vu: f64| 10f64.powf(vu / 20.0);
        assert_eq!(Scale::deflection(ratio(-20.0)), 0.0);
        assert_eq!(Scale::deflection(0.0), 0.0);
        assert!((Scale::deflection(1.0) - 0.9 / (ratio(3.0) - 0.1)).abs() < 1e-12);
        assert!((Scale::deflection(ratio(3.0)) - 1.0).abs() < 1e-12);
        assert_eq!(Scale::deflection(10.0), Scale::STOP);
        assert!((Scale::reading_db(ratio(-4.3)) + 4.3).abs() < 1e-9);
        assert_eq!(Scale::reading_db(0.0), f64::NEG_INFINITY);
    }

    #[test]
    fn the_stop_ratio_is_the_ratio_at_the_stop_deflection() {
        let top = 10f64.powf(3.0 / 20.0);
        assert!((Scale::STOP_RATIO - (0.1 + Scale::STOP * (top - 0.1))).abs() < 1e-12);
    }

    #[test]
    fn a_reading_is_never_a_number_that_cannot_be_compared() {
        assert_eq!(Scale::reading_db(-0.001), f64::NEG_INFINITY);
        assert_eq!(Scale::reading_db(f64::NAN), f64::NEG_INFINITY);
        assert_eq!(Scale::reading_db(-0.0), f64::NEG_INFINITY);
    }

    #[test]
    fn the_needle_stops_at_both_ends_and_leaves_a_stop_at_once() {
        let mut needle = Needle::at(1.0);
        needle.set(0.0);
        for ms in 1..2000 {
            needle.advance_to(ms);
            assert!(needle.x() >= 0.0, "{ms}");
        }
        // +22 VU held for a second, then 0 VU: off the right stop as the ballistics say.
        let mut hit = Needle::at(0.0);
        hit.set(10f64.powf(22.0 / 20.0));
        hit.advance_to(1000);
        assert_eq!(hit.x(), Scale::STOP_RATIO);
        assert!((hit.deflection() - Scale::STOP).abs() < 1e-12);
        hit.set(1.0);
        let mut off = None;
        for ms in 1001..2000 {
            hit.advance_to(ms);
            if hit.deflection() < Scale::STOP {
                off = Some(ms - 1000);
                break;
            }
        }
        assert!(off.is_some_and(|ms| ms <= 5), "{off:?}");
    }

    #[test]
    fn a_long_gap_snaps_to_steady_state_without_stepping() {
        let mut needle = Needle::at(0.0);
        needle.set(0.5);
        needle.advance_to(1_700_000_000_000);
        assert_eq!(needle.x(), 0.5);
        needle.set(100.0);
        needle.advance_to(1_700_000_000_000 + MAX_CATCH_UP_MS + 1);
        assert_eq!(needle.x(), Scale::STOP_RATIO);
        // A gap inside the bound integrates as before.
        let mut a = Needle::at(0.0);
        a.set(1.0);
        a.advance_to(MAX_CATCH_UP_MS);
        assert!(a.x() > 0.999 && a.x() < 1.0001);
    }

    #[test]
    fn a_reference_given_as_a_wide_number_does_not_wrap() {
        assert_eq!(Reference::new(200).db(), Reference::MAX);
        assert_eq!(Reference::new(-200).db(), Reference::MIN);
        assert_eq!(Reference::new(i32::MAX).db(), Reference::MAX);
        assert_eq!(Reference::new(-18).db(), -18);
    }

    #[test]
    fn the_reference_is_clamped_and_sets_zero_vu() {
        assert_eq!(Reference::default().db(), -18);
        assert_eq!(Reference::new(-31i32).db(), -30);
        assert_eq!(Reference::new(-5).db(), -6);
        let rms = 10f64.powf(-18.0 / 20.0);
        let vu = |reference: i32| Scale::reading_db(Reference::new(reference).ratio(rms));
        assert!(vu(-18).abs() < 0.05);
        assert!((vu(-6) + 12.0).abs() < 0.05);
        let pinned = Scale::deflection(Reference::new(-30).ratio(rms));
        assert_eq!(pinned, Scale::STOP);
        assert!((vu(-30) - 12.0).abs() < 0.05);
    }

    #[test]
    fn a_minus_18_dbfs_rms_reads_zero_vu_after_a_second() {
        let rms = 10f64.powf(-18.0 / 20.0);
        let mut needle = Needle::at(0.0);
        needle.set(Reference::default().ratio(rms));
        needle.advance_to(1000);
        assert!(Scale::reading_db(needle.x()).abs() < 0.05);
    }

    #[test]
    fn the_lamp_holds_1200_ms_after_the_last_over() {
        let mut lamp = Lamp::default();
        assert!(!lamp.lit(0));
        lamp.push(0.5, 100);
        assert!(!lamp.lit(100));
        lamp.push(crate::analysis::OVER, 1000);
        assert!(lamp.lit(1000));
        assert!(lamp.lit(1000 + 1199));
        assert!(!lamp.lit(1000 + 1201));
        lamp.push(f32::NAN, 2300);
        assert!(!lamp.lit(2300));
        lamp.push(1.0, 5000);
        assert!(lamp.lit(5000 + 1199));
        assert!(!lamp.lit(5000 + 1201));
    }

    #[test]
    fn the_golden_path_is_deterministic_and_covers_the_script() {
        let a = golden();
        assert_eq!(a, golden());
        assert!(a.samples.len() > 300);
        let top = a.samples.iter().map(|s| s.f).fold(0.0, f64::max);
        assert_eq!(top, Scale::STOP);
        assert_eq!(a.samples[0].f, 0.0);
    }
}
