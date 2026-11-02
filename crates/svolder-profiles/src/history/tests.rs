// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use crate::Placed;
use crate::Registry;

use super::*;

const SECOND: Duration = Duration::from_secs(1);

/// An instant well into the run, so earlier ones exist.
fn origin() -> Instant {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *BASE.get_or_init(|| Instant::now() + Duration::from_secs(7_200))
}

fn series_of(aggregate: Aggregate, samples: &[(u64, f64)]) -> (Series, Instant) {
    let t0 = origin();
    let mut series = Series::new(aggregate, Window::TUI);
    for &(ms, value) in samples {
        series.push(t0 + Duration::from_millis(ms), value);
    }
    (series, t0)
}

fn some(values: &[f64]) -> Vec<Option<f64>> {
    values.iter().map(|&v| Some(v)).collect()
}

/// The daemon's window bins by the second and keeps half an hour: two readings in one second
/// are one sample, and a sample 30 minutes and a second old is gone.
#[test]
fn the_daemons_window_bins_by_the_second_and_keeps_half_an_hour() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Max, Window::DAEMON);
    series.push(t0, 1.0);
    series.push(t0 + Duration::from_millis(900), 5.0);
    series.push(t0 + SECOND, 2.0);
    assert_eq!(series.len(), 2);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [5.0, 2.0]
    );
    series.push(t0 + Duration::from_secs(30 * 60 + 1), 3.0);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [2.0, 3.0]
    );
}

/// A value held for longer than the keep still stands behind the next change: the newest sample
/// at or before the keep stays, and the one before it goes.
#[test]
fn the_newest_sample_beyond_the_keep_stays_to_say_what_held() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Last, Window::DAEMON);
    series.push(t0, 1.0);
    series.push(t0 + Duration::from_secs(31 * 60), 3.0);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [1.0, 3.0]
    );
    series.push(t0 + Duration::from_secs(32 * 60), 4.0);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [1.0, 3.0, 4.0]
    );
    series.push(t0 + Duration::from_secs(62 * 60), 5.0);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [4.0, 5.0]
    );
}

/// Seeded samples go before the ones the series holds; any at or after its oldest are dropped,
/// so a seed never doubles what was seen.
#[test]
fn a_seed_goes_before_what_the_series_holds() {
    let (mut series, t0) = series_of(Aggregate::Last, &[(10_000, 7.0)]);
    series.seed([
        (t0, 1.0),
        (t0 + 5 * SECOND, 2.0),
        (t0 + 10 * SECOND, 9.0),
        (t0 + 11 * SECOND, 9.0),
    ]);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [1.0, 2.0, 7.0]
    );
    let mut empty = Series::new(Aggregate::Last, Window::TUI);
    empty.seed([(t0, 4.0), (t0 + SECOND, f64::NAN)]);
    assert_eq!(empty.len(), 1);
}

/// The readings map keeps one series per function: the maximum for a meter, a level and a
/// fault, the last value for the rest, a function the profile does not name included.
#[test]
fn readings_keep_the_maximum_for_meters_levels_and_faults_and_the_last_for_the_rest() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in");
    let volume = profile.control_by_key("volume").expect("a volume").id;
    let input = profile.control_by_key("input").expect("an input").id;
    let unknown: svolder_proto::FunctionId = "99".parse().expect("an id");
    let t0 = origin();
    let mut readings = Readings::new(Window::DAEMON);
    for (id, first, second) in [(volume, 9.0, 3.0), (input, 9.0, 3.0), (unknown, 9.0, 3.0)] {
        readings.record(profile, id, t0, first);
        readings.record(profile, id, t0 + Duration::from_millis(500), second);
    }
    let last = |id| readings.series(id).expect("a series").samples().last();
    assert_eq!(last(volume).map(|(_, v)| v), Some(9.0));
    assert_eq!(last(input).map(|(_, v)| v), Some(3.0));
    assert_eq!(last(unknown).map(|(_, v)| v), Some(3.0));
    assert_eq!(
        readings.series(volume).map(Series::aggregate),
        Some(Aggregate::Max)
    );
    assert_eq!(readings.iter().count(), 3);
}

#[test]
fn a_value_that_is_not_finite_is_not_stored() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Max, Window::TUI);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        series.push(t0, value);
    }
    assert!(series.is_empty());
    series.push(t0, 1.0);
    series.push(t0 + SECOND, f64::NAN);
    assert_eq!(series.len(), 1);
}

#[test]
fn bins_of_250_ms_keep_the_last_or_the_maximum() {
    let (last, _) = series_of(
        Aggregate::Last,
        &[(0, 5.0), (100, 9.0), (200, 2.0), (250, 1.0)],
    );
    let (max, _) = series_of(
        Aggregate::Max,
        &[(0, 5.0), (100, 9.0), (200, 2.0), (250, 1.0)],
    );
    assert_eq!(last.len(), 2);
    assert_eq!(max.len(), 2);
    let now = origin() + SECOND;
    let all = |s: &Series| columns(s, now, SECOND, 4);
    assert_eq!(all(&last), vec![Some(2.0), Some(1.0), Some(1.0), Some(1.0)]);
    assert_eq!(all(&max), vec![Some(9.0), Some(1.0), Some(1.0), Some(1.0)]);
}

#[test]
fn a_bin_is_measured_from_its_first_sample() {
    let (series, _) = series_of(
        Aggregate::Last,
        &[(0, 1.0), (249, 2.0), (500, 3.0), (749, 4.0), (750, 5.0)],
    );
    assert_eq!(series.len(), 3);
}

#[test]
fn columns_take_the_maximum_for_meters_and_the_last_for_the_rest() {
    // Four samples 250 ms apart fall in one column of a 2-column, 2-second span.
    let (series, t0) = series_of(
        Aggregate::Max,
        &[(0, 1.0), (250, 9.0), (500, 3.0), (750, 2.0), (1_000, 4.0)],
    );
    let now = t0 + 2 * SECOND;
    assert_eq!(
        columns(&series, now, 2 * SECOND, 2),
        vec![Some(9.0), Some(4.0)]
    );
    let mut last = Series::new(Aggregate::Last, Window::TUI);
    for (ms, v) in [(0, 1.0), (250, 9.0), (500, 3.0), (750, 2.0), (1_000, 4.0)] {
        last.push(t0 + Duration::from_millis(ms), v);
    }
    assert_eq!(
        columns(&last, now, 2 * SECOND, 2),
        vec![Some(2.0), Some(4.0)]
    );
}

#[test]
fn a_peak_survives_at_every_span() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Max, Window::TUI);
    for i in 0..4_000u64 {
        let value = if i == 1_234 { 100.0 } else { 1.0 };
        series.push(t0 + Duration::from_millis(250 * i), value);
    }
    let now = t0 + Duration::from_millis(250 * 4_000);
    for minutes in [1u64, 2, 3, 5, 10, 15, 30, 60] {
        let span = Duration::from_secs(minutes * 60);
        let cols = columns(&series, now, span, 80);
        let inside = now.duration_since(t0 + Duration::from_millis(250 * 1_234)) <= span;
        let seen = cols.iter().flatten().any(|&v| v == 100.0);
        assert_eq!(seen, inside, "span {minutes} min");
    }
}

#[test]
fn empty_columns_carry_the_previous_value_forward() {
    let (series, t0) = series_of(Aggregate::Last, &[(500, 3.0), (3_000, 7.0)]);
    let cols = columns(&series, t0 + 4 * SECOND, 4 * SECOND, 8);
    assert_eq!(
        cols,
        vec![
            None,
            Some(3.0),
            Some(3.0),
            Some(3.0),
            Some(3.0),
            Some(3.0),
            Some(7.0),
            Some(7.0)
        ]
    );
}

#[test]
fn a_sample_before_the_window_carries_into_it() {
    let (series, t0) = series_of(Aggregate::Last, &[(0, 6.0)]);
    let cols = columns(&series, t0 + 10 * SECOND, 2 * SECOND, 4);
    assert_eq!(cols, some(&[6.0, 6.0, 6.0, 6.0]));
}

#[test]
fn nothing_is_known_before_the_first_sample() {
    let (series, t0) = series_of(Aggregate::Last, &[(3_000, 6.0)]);
    let cols = columns(&series, t0 + 4 * SECOND, 4 * SECOND, 4);
    assert_eq!(cols, vec![None, None, None, Some(6.0)]);
    assert_eq!(
        columns(&Series::new(Aggregate::Last, Window::TUI), t0, SECOND, 3),
        vec![None; 3]
    );
}

#[test]
fn degenerate_windows_are_empty_not_panics() {
    let (series, t0) = series_of(Aggregate::Last, &[(0, 1.0)]);
    assert!(columns(&series, t0 + SECOND, SECOND, 0).is_empty());
    assert_eq!(
        columns(&series, t0 + SECOND, Duration::ZERO, 2),
        vec![None; 2]
    );
}

#[test]
fn a_clock_that_went_back_reads_the_newer_samples_as_not_yet_happened() {
    let (series, t0) = series_of(Aggregate::Last, &[(0, 1.0), (10_000, 2.0)]);
    // `now` is before the newest sample, and before the oldest.
    let cols = columns(&series, t0 + 5 * SECOND, 4 * SECOND, 4);
    assert_eq!(cols, some(&[1.0, 1.0, 1.0, 1.0]));
    let cols = columns(&series, t0 - SECOND, 4 * SECOND, 4);
    assert_eq!(cols, vec![None; 4]);
    // Pushing with a clock that went back neither panics nor drops the history.
    let mut series = series;
    series.push(t0 + SECOND, 3.0);
    assert!(series.len() >= 2);
}

#[test]
fn a_series_holds_at_most_14_400_samples() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Max, Window::TUI);
    // A device reporting every 50 ms for an hour.
    for i in 0..72_000u64 {
        series.push(t0 + Duration::from_millis(50 * i), i as f64);
    }
    assert!(series.len() <= Window::TUI.capacity);
    assert!(series.len() >= Window::TUI.capacity - 1);
    // Far more than an hour of 250 ms bins stays bounded by the capacity or the age.
    for i in 0..30_000u64 {
        series.push(t0 + 4 * SECOND * 1000 + Duration::from_millis(250 * i), 0.0);
    }
    assert!(series.len() <= Window::TUI.capacity);
}

#[test]
fn only_the_newest_sample_beyond_60_minutes_is_kept() {
    let t0 = origin();
    let mut series = Series::new(Aggregate::Last, Window::TUI);
    series.push(t0, 1.0);
    series.push(t0 + Duration::from_secs(3_600), 2.0);
    assert_eq!(series.len(), 2);
    series.push(t0 + Duration::from_secs(3_601), 3.0);
    assert_eq!(series.len(), 3);
    series.push(t0 + Duration::from_secs(7_300), 4.0);
    assert_eq!(
        series.samples().map(|(_, v)| v).collect::<Vec<_>>(),
        [3.0, 4.0]
    );
    series.clear();
    assert!(series.is_empty());
    series.push(t0, 1.0);
    assert_eq!(series.len(), 1);
}

/// A linear scale from `from` (empty) to `to` (full), with no class-A tick.
fn linear(from: i64, to: i64) -> Placed {
    Placed::Linear {
        from,
        to,
        tick: None,
    }
}

#[test]
fn a_scaled_control_normalises_through_its_scale() {
    assert_eq!(
        normalise(
            Some(linear(0, 200)),
            &[None, Some(0.0), Some(50.0), Some(400.0)]
        ),
        vec![None, Some(0.0), Some(0.25), Some(1.0)]
    );
}

#[test]
fn a_log_scale_puts_a_quiet_window_at_the_bottom() {
    let placed = Placed::Log {
        zero: 160,
        full: 350,
        class_a: 0.25,
    };
    let quiet = [Some(161.0), Some(162.0), Some(161.0)];
    let drawn = normalise(Some(placed), &quiet);
    assert!(
        drawn.iter().flatten().all(|level| *level < 0.25),
        "{drawn:?}"
    );
    // With no scale the same window fills the graph's height: the self-scaling a scale does not do.
    let unscaled = normalise(None, &quiet);
    assert!(unscaled.iter().flatten().any(|level| *level == 1.0));
}

#[test]
fn an_unscaled_control_autoscales_to_the_window() {
    assert_eq!(
        normalise(None, &[Some(10.0), None, Some(30.0), Some(20.0)]),
        vec![Some(0.0), None, Some(1.0), Some(0.5)]
    );
}

#[test]
fn a_flat_window_draws_at_the_bottom() {
    assert_eq!(normalise(None, &some(&[5.0, 5.0])), some(&[0.0, 0.0]));
    assert_eq!(normalise(None, &[None]), vec![None]);
}

#[test]
fn channels_share_one_autoscale() {
    let both = normalise_together(None, &[&some(&[0.0, 10.0]), &some(&[5.0, 20.0])]);
    assert_eq!(both, vec![some(&[0.0, 0.5]), some(&[0.25, 1.0])]);
}

const HOUR: Duration = Duration::from_secs(3600);

#[test]
fn the_link_is_a_step_function_over_the_span() {
    let mut link = Link::default();
    // Up before the window, down for the third hour of four, up again.
    link.seed([(0, true), (2 * 3_600_000 + 1, false), (3 * 3_600_000, true)]);
    let columns = link.columns(4 * 3_600_000, 4 * HOUR, 4);
    assert_eq!(columns, some(&[1.0, 1.0, 0.0, 1.0]));
}

#[test]
fn the_link_is_blank_before_its_first_change_and_carries_the_state_in() {
    let mut link = Link::default();
    link.seed([(3_000, true)]);
    let columns = link.columns(4_000, Duration::from_secs(4), 4);
    assert_eq!(columns, vec![None, None, None, Some(1.0)]);
    link.seed([(1_000, false)]);
    let columns = link.columns(10_000, Duration::from_secs(4), 4);
    assert_eq!(columns, some(&[0.0, 0.0, 0.0, 0.0]));
    assert_eq!(Link::default().columns(10_000, HOUR, 3), vec![None; 3]);
}

#[test]
fn a_change_after_now_is_not_counted_and_never_draws_outside() {
    let mut link = Link::default();
    link.seed([(1_000, true), (9_000_000, false), (9_000_001, true)]);
    let columns = link.columns(2_000, Duration::from_secs(4), 4);
    assert_eq!(columns.len(), 4);
    assert_eq!(columns[3], Some(1.0));
    // A span reaching back past the epoch does not underflow.
    assert_eq!(link.columns(10, HOUR, 5).len(), 5);
    assert_eq!(link.columns(0, HOUR, 5), vec![None; 5]);
}

#[test]
fn a_repeated_state_adds_nothing_and_the_history_holds_its_caps() {
    let mut link = Link::default();
    link.record(1_000, true);
    link.record(2_000, true);
    assert_eq!(link.changes.len(), 1);
    for i in 0..10_000u64 {
        link.record(3_000 + i * 1_000, i % 2 == 0);
    }
    assert!(link.changes.len() <= LINK_CAP);
    // Two days of hourly flips keep the day plus the change before it.
    let mut link = Link::default();
    for hour in 0..48u64 {
        link.record(hour * 3_600_000, hour % 2 == 0);
    }
    assert_eq!(link.changes.len(), 25);
}

const HOUR_MS: u64 = 3_600_000;

#[test]
fn a_drop_shorter_than_a_column_still_shows_down() {
    let now = 100 * HOUR_MS;
    let mut link = Link::default();
    link.seed([
        (now - 20 * HOUR_MS, true),
        (now - 6 * HOUR_MS, false),
        (now - 6 * HOUR_MS + 30_000, true),
    ]);
    let columns = link.columns(now, 12 * HOUR, 140);
    let down: Vec<usize> = (0..140).filter(|&i| columns[i] == Some(0.0)).collect();
    assert_eq!(down, [70]);
    assert!(
        columns
            .iter()
            .enumerate()
            .all(|(i, c)| i == 70 || *c == Some(1.0))
    );
}

#[test]
fn a_drop_on_a_column_boundary_fills_exactly_its_columns() {
    // Four columns of a minute each over four minutes.
    let now = 10 * 60_000;
    let mut link = Link::default();
    link.seed([(0, true), (now - 3 * 60_000, false), (now - 60_000, true)]);
    let columns = link.columns(now, Duration::from_secs(240), 4);
    assert_eq!(columns, some(&[1.0, 0.0, 0.0, 1.0]));
    // Straddling a boundary: both columns it touches are down.
    link.seed([(0, true), (now - 90_000, false), (now - 30_000, true)]);
    let columns = link.columns(now, Duration::from_secs(240), 4);
    assert_eq!(columns, some(&[1.0, 1.0, 0.0, 0.0]));
}

#[test]
fn an_up_down_up_column_is_down_and_a_long_drop_carries_through_empty_columns() {
    let now = 10_000;
    let mut link = Link::default();
    link.seed([(0, true), (5_100, false), (5_200, true)]);
    assert_eq!(
        link.columns(now, Duration::from_secs(10), 5),
        some(&[1.0, 1.0, 0.0, 1.0, 1.0])
    );
    link.seed([(0, true), (2_000, false)]);
    assert_eq!(
        link.columns(now, Duration::from_secs(10), 5),
        some(&[1.0, 0.0, 0.0, 0.0, 0.0])
    );
}

#[test]
fn a_zero_width_flap_marks_its_column_down_and_a_later_change_is_ignored() {
    let now = 10_000;
    let mut link = Link::default();
    link.seed([(0, true), (5_000, false), (5_000, true), (now + 1, false)]);
    assert_eq!(
        link.columns(now, Duration::from_secs(10), 5),
        some(&[1.0, 1.0, 0.0, 1.0, 1.0])
    );
}

#[test]
fn a_zero_width_change_before_the_window_never_marks_column_zero_down() {
    let now = 100_000;
    let span = Duration::from_secs(10);
    let mut link = Link::default();
    link.seed([(1_000, true), (5_000, false), (5_000, true)]);
    assert_eq!(link.columns(now, span, 5), some(&[1.0; 5]));
    let mut link = Link::default();
    link.record(1_000, true);
    link.record(5_000, false);
    link.record(3_000, true);
    assert_eq!(link.columns(now, span, 5), some(&[1.0; 5]));
    // At the window's start it is still its own moment.
    let mut link = Link::default();
    link.seed([(1_000, true), (90_000, false), (90_000, true)]);
    assert_eq!(link.columns(now, span, 5)[0], Some(0.0));
}

#[test]
fn a_merge_keeps_the_carried_changes_before_the_daemons_and_appends_its_own() {
    let mut link = Link::default();
    link.seed([(1_000, true), (10_000, false), (20_000, true)]);
    let span = Duration::from_secs(50);
    assert_eq!(
        link.columns(50_000, span, 5),
        some(&[1.0, 0.0, 1.0, 1.0, 1.0])
    );
    // The old daemon's feed closed: down at 30 s. The new one answers up at 40 s.
    link.record(30_000, false);
    link.merge([(40_000, true)]);
    assert_eq!(
        link.columns(50_000, span, 5),
        some(&[1.0, 0.0, 1.0, 0.0, 1.0])
    );
    // A daemon answer that is the state already held adds nothing; nothing held is a seed.
    link.merge([(45_000, true)]);
    assert_eq!(link.changes.len(), 5);
    let mut fresh = Link::default();
    fresh.merge([(1_000, true), (9_000, false)]);
    assert_eq!(fresh.changes, [(1_000, true), (9_000, false)]);
}

#[cfg(feature = "serde")]
#[test]
fn an_unknown_aggregate_kind_decodes_and_reads_as_last() {
    assert_eq!(
        serde_json::from_str::<Aggregate>("\"max\"").unwrap(),
        Aggregate::Max
    );
    assert_eq!(
        serde_json::from_str::<Aggregate>("\"last\"").unwrap(),
        Aggregate::Last
    );
    let later = serde_json::from_str::<Aggregate>("\"median\"").unwrap();
    let mut series = Series::new(later, Window::DAEMON);
    let start = Instant::now();
    series.push(start, 5.0);
    series.push(start, 3.0);
    assert_eq!(
        series.samples().map(|(_, value)| value).collect::<Vec<_>>(),
        [3.0]
    );
}
