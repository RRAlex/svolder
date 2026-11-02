// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How loud what is playing is, for the animations that follow a level rather than an event.
//!
//! The breathing borders and the meters' peak markers both need one number — how far up its
//! scale the loudest thing the device reports is — and nothing else about where it came from.
//! [`LevelSource`] is that number, and [`MeterLevel`] is the device's own meters behind it. The
//! capture of the host's default sink is the other place a level comes from, and [`LevelFrom`]
//! names which of the two the glow, the borders and the title meter read: one at a time, never
//! a mix of both.
//!
//! A meter with no scale to be read against reports no level at all, exactly as its row draws
//! no bar: nothing here invents a full scale the device has not been measured for. A meter
//! that is dead (switched off, or its amp asleep or warming up) reports none either.

use svolder_profiles::ControlKind;

use crate::tui::app::App;

/// Where the level the glow, the breathing borders and the title meter read comes from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFrom {
    /// The device's own meters, while they stream.
    Meters,
    /// The capture of the host's default sink, while no meter streams and the capture runs.
    Host,
    /// Nowhere: no meter streams and no capture runs.
    Nowhere,
}

/// Something that reports how loud what it watches is now.
pub trait LevelSource {
    /// How far up its scale the loudest thing this source watches is, `0.0..=1.0`, or `None`
    /// while nothing it watches reports a level.
    fn level(&self) -> Option<f64>;
}

/// The device's own meters as a level source.
pub struct MeterLevel<'a> {
    /// The app whose readings the meters are read from.
    pub app: &'a App,
}

impl LevelSource for MeterLevel<'_> {
    /// The loudest meter the device is reporting, on its own scale: each reading's
    /// [`svolder_profiles::Placed::fraction`], `0.0..=1.0`.
    ///
    /// `None` while the meters are dead and when no meter that has a scale has a reading,
    /// which is what leaves the borders still on a device that reports no meters.
    fn level(&self) -> Option<f64> {
        if !self.app.meters_live() {
            return None;
        }
        self.app
            .profile
            .controls
            .iter()
            .filter(|control| control.kind == ControlKind::Meter)
            .filter_map(|control| {
                let raw = self.app.readings.values.get(&control.id)?.raw_i64()?;
                Some(self.app.placed(control)?.fraction(raw as f64))
            })
            .fold(None, |loudest, level| {
                Some(loudest.map_or(level, |best: f64| best.max(level)))
            })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use svolder_ipc::TuiConfig;
    use svolder_profiles::{Profile, Registry};

    use crate::backend::Update;
    use crate::tui::test_support::{
        AEGIR_SCALE, meter_reading, vo, with_log_scale, with_meter_range, without_scale,
    };

    use super::*;

    /// The Vidar 2's own meter functions.
    const METER_L: &str = "20.1";
    /// The second of them.
    const METER_R: &str = "20.2";
    /// The scale the meters are given in these cases.
    const SCALE: (i64, i64) = (0, 3_000);

    /// The built-in profile `name`.
    fn profile(name: &str) -> Profile {
        Registry::builtin()
            .expect("the built-in profiles load")
            .get(name)
            .unwrap_or_else(|| panic!("{name} is built in"))
            .clone()
    }

    /// An app on the Vidar 2, whose two meters are given the scale no amp has been measured
    /// for yet.
    fn amp_app() -> App {
        let profile = with_meter_range(profile("vidar2"), SCALE);
        App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new())
    }

    /// Report one meter reading.
    fn hear(app: &mut App, id: &str, raw: i64) {
        app.apply(Update::Status(meter_reading(id, raw)), Instant::now());
    }

    /// The level is the loudest channel over its own scale, and there is none at all until a
    /// meter has reported.
    #[test]
    fn the_meter_level_is_the_loudest_channel_over_its_scale_and_none_without_meters() {
        let mut app = amp_app();
        assert_eq!(MeterLevel { app: &app }.level(), None);

        hear(&mut app, METER_L, 1_500);
        hear(&mut app, METER_R, 3_000);
        let level = MeterLevel { app: &app }
            .level()
            .expect("two meters have reported");
        assert!(
            (level - 1.0).abs() < 1e-9,
            "the louder channel is at the top of its scale, not at {level}"
        );
    }

    /// With more than two scaled meters streaming, the level is still the loudest of them —
    /// the glow follows one channel among any number, not just a left/right pair.
    #[test]
    fn the_meter_level_is_the_loudest_of_more_than_two_channels() {
        let mut app = App::new(
            profile("fake-five"),
            "five".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        for (n, raw) in [(1, 100), (2, 400), (3, 200), (4, 60), (5, 150)] {
            hear(&mut app, &format!("20.{n}"), raw);
        }
        let level = MeterLevel { app: &app }
            .level()
            .expect("every meter has reported");
        // Meter 2's 400 is the loudest raw reading, on the Kraken's scale of zero 30 and full
        // scale 600: log10(400 - 30 + 1) / log10(600 - 30 + 1).
        let expected = 371f64.log10() / 571f64.log10();
        assert!(
            (level - expected).abs() < 1e-9,
            "the loudest channel came out as {level}, not {expected}"
        );
    }

    /// A meter fills from the idle reading its profile names rather than from the bottom of
    /// its range, and a reading outside the scale is read as one of its ends.
    #[test]
    fn the_level_runs_from_the_meters_floor_and_is_clamped_to_its_scale() {
        let mut app = amp_app();
        for control in &mut app.profile.controls {
            if control.kind == ControlKind::Meter {
                control.floor = Some(1_000);
            }
        }
        hear(&mut app, METER_L, 1_500);
        let level = MeterLevel { app: &app }
            .level()
            .expect("a meter has reported");
        assert!(
            (level - 0.25).abs() < 1e-9,
            "a quarter of the way above the floor reads as {level}"
        );

        hear(&mut app, METER_L, 9_000);
        assert_eq!(MeterLevel { app: &app }.level(), Some(1.0));
        hear(&mut app, METER_L, 0);
        assert_eq!(MeterLevel { app: &app }.level(), Some(0.0));
    }

    /// A meter with no scale, as a user's profile may leave one, reports no level: the borders
    /// stand still rather than breathing against a scale nothing named.
    #[test]
    fn a_meter_with_no_scale_of_its_own_reports_no_level() {
        let mut app = App::new(
            without_scale(profile("vidar2")),
            "vidar".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        hear(&mut app, METER_L, 1_500);
        assert_eq!(
            MeterLevel { app: &app }.level(),
            None,
            "a meter with no scale was read against a scale of its own"
        );
    }

    /// A log-scaled meter's level is its reading's fraction of the live part of its scale.
    #[test]
    fn a_log_meter_gives_its_fraction_of_the_live_part_as_the_level() {
        // Aegir 2's scale; 173 is half way up the live part.
        let mut app = App::new(
            with_log_scale(profile("vidar2"), AEGIR_SCALE),
            "vidar".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        hear(&mut app, METER_L, 173);
        let level = MeterLevel { app: &app }.level().expect("a level");
        assert!((level - 0.5).abs() < 0.005, "{level}");
    }

    /// A meter switched off reports no level, whatever it last read.
    #[test]
    fn a_dead_meter_gives_no_level() {
        let mut app = App::new(
            with_log_scale(profile("vidar2"), AEGIR_SCALE),
            "vidar".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        hear(&mut app, METER_L, 300);
        assert!(MeterLevel { app: &app }.level().is_some());
        let off = svolder_ipc::ValueObject {
            value_type: "bool".into(),
            ..vo(
                "21.2",
                Some("power_meters"),
                serde_json::json!(false),
                "off",
            )
        };
        app.apply(Update::Status(off), Instant::now());
        assert_eq!(MeterLevel { app: &app }.level(), None);
    }
}
