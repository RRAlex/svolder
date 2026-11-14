// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the bar between the Controls pane and the Tone pane stands.
//!
//! The two panes share the width the band gives them, less the column the wide tier keeps between
//! them. A fraction (`device_split` under `[tui]`) says how much of that width the Controls pane
//! takes; each pane keeps at least [`MIN_SIDE`] columns, so the fraction is read through
//! [`controls_columns`], which clamps it again at every width. A band with no room for two panes
//! of [`MIN_SIDE`] has no bar to move: [`movable`] says so. No screen is drawn under 80 columns,
//! so the panes there are 40 and 40.

/// The fewest columns either pane keeps.
pub(crate) const MIN_SIDE: u16 = 40;

/// How far a nudge of the bar by key moves it, in columns.
pub(crate) const NUDGE: u16 = 4;

/// The columns the two panes share in a band `band` wide with `gap` columns between them.
pub(crate) fn shared(band: u16, gap: u16) -> u16 {
    band.saturating_sub(gap)
}

/// Whether a band `band` wide has room for two panes of [`MIN_SIDE`] columns, which is when the
/// bar can be moved.
pub(crate) fn movable(band: u16, gap: u16) -> bool {
    shared(band, gap) >= 2 * MIN_SIDE
}

/// The columns the Controls pane takes of `shared`: `fraction` of them, held to [`MIN_SIDE`] on
/// both sides. A `shared` too small for both gives the Controls pane [`MIN_SIDE`] all the same.
pub(crate) fn controls_columns(fraction: f64, shared: u16) -> u16 {
    let most = shared.saturating_sub(MIN_SIDE).max(MIN_SIDE);
    let wanted = (fraction.clamp(0.0, 1.0) * f64::from(shared)).round();
    // The product is in 0..=shared, which a u16 holds.
    (wanted as u16).clamp(MIN_SIDE, most)
}

/// The fraction that puts `columns` of `shared` in the Controls pane, to the three decimals the
/// file keeps: [`controls_columns`] reads it back as the same `columns` at every width a terminal
/// has.
pub(crate) fn fraction_of(columns: u16, shared: u16) -> f64 {
    if shared == 0 {
        return 0.5;
    }
    (f64::from(columns) / f64::from(shared) * 1000.0).round() / 1000.0
}

/// The fraction after the bar moves to leave `columns` to the Controls pane of `shared`, held to
/// [`MIN_SIDE`] on both sides.
pub(crate) fn moved_to(columns: i32, shared: u16) -> f64 {
    let most = shared.saturating_sub(MIN_SIDE).max(MIN_SIDE);
    let columns = columns.clamp(i32::from(MIN_SIDE), i32::from(most));
    fraction_of(u16::try_from(columns).unwrap_or(MIN_SIDE), shared)
}

/// The fraction after a nudge of [`NUDGE`] columns toward the Tone pane (`dir` above zero) or the
/// Controls pane (below zero), starting from the columns `fraction` stands for in `shared`.
pub(crate) fn nudged(fraction: f64, shared: u16, dir: i32) -> f64 {
    let now = i32::from(controls_columns(fraction, shared));
    moved_to(now + dir.signum() * i32::from(NUDGE), shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_even_split_gives_each_pane_half_at_the_three_widths() {
        // 80 columns: 40 and 40. 120: 119 shared, the Controls pane 60 of them. 200: 199, 100.
        assert_eq!(controls_columns(0.5, 80), 40);
        assert_eq!(controls_columns(0.5, 119), 60);
        assert_eq!(controls_columns(0.5, 199), 100);
    }

    #[test]
    fn a_fraction_is_held_to_forty_columns_on_both_sides() {
        for shared in [80, 119, 199, 255] {
            assert_eq!(controls_columns(0.0, shared), MIN_SIDE, "{shared}");
            assert_eq!(controls_columns(0.01, shared), MIN_SIDE, "{shared}");
            assert_eq!(controls_columns(1.0, shared), shared - MIN_SIDE, "{shared}");
            assert_eq!(
                controls_columns(0.99, shared),
                shared - MIN_SIDE,
                "{shared}"
            );
            // Out of range or not a number is still a split inside the bounds.
            assert_eq!(controls_columns(7.0, shared), shared - MIN_SIDE, "{shared}");
            assert_eq!(controls_columns(-3.0, shared), MIN_SIDE, "{shared}");
            let from_nan = controls_columns(f64::NAN, shared);
            assert!(
                (MIN_SIDE..=shared - MIN_SIDE).contains(&from_nan),
                "{shared}"
            );
        }
    }

    #[test]
    fn a_resize_reclamps_the_same_fraction() {
        // 0.8 of 199 columns is 159, and 159 of a 119-column share would leave the Tone pane 0:
        // the same fraction at the narrower width keeps its 40.
        assert_eq!(controls_columns(0.8, 199), 159);
        assert_eq!(controls_columns(0.8, 119), 79);
        assert_eq!(controls_columns(0.8, 100), 60);
        assert_eq!(controls_columns(0.8, 80), 40);
        assert_eq!(controls_columns(0.3, 199), 60);
        assert_eq!(controls_columns(0.3, 100), 40);
    }

    #[test]
    fn the_bar_moves_only_where_there_is_room_for_two_panes_of_forty() {
        // The tiers below 120 columns keep no gap, the wide tier keeps one.
        assert!(!movable(79, 0));
        assert!(movable(80, 0));
        assert!(movable(119, 0));
        assert!(movable(120, 1));
        assert!(!movable(80, 1));
    }

    #[test]
    fn columns_survive_a_round_trip_through_the_fraction() {
        for shared in [80_u16, 119, 150, 199, 255] {
            for columns in MIN_SIDE..=shared - MIN_SIDE {
                let fraction = fraction_of(columns, shared);
                assert_eq!(
                    controls_columns(fraction, shared),
                    columns,
                    "{columns}/{shared}"
                );
            }
        }
    }

    #[test]
    fn the_fraction_is_kept_to_three_decimals() {
        assert!((fraction_of(60, 199) - 0.302).abs() < 1e-12);
        assert!((fraction_of(100, 200) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn moving_the_bar_stops_at_both_limits() {
        assert_eq!(controls_columns(moved_to(-50, 199), 199), MIN_SIDE);
        assert_eq!(controls_columns(moved_to(1000, 199), 199), 199 - MIN_SIDE);
        assert_eq!(controls_columns(moved_to(75, 199), 199), 75);
    }

    #[test]
    fn a_nudge_moves_four_columns_and_stops_at_the_limits() {
        let at = |fraction: f64| controls_columns(fraction, 199);
        let start = 0.5;
        assert_eq!(at(nudged(start, 199, 1)), 104);
        assert_eq!(at(nudged(start, 199, -1)), 96);
        let mut fraction = start;
        for _ in 0..40 {
            fraction = nudged(fraction, 199, -1);
        }
        assert_eq!(at(fraction), MIN_SIDE);
        for _ in 0..80 {
            fraction = nudged(fraction, 199, 1);
        }
        assert_eq!(at(fraction), 199 - MIN_SIDE);
        // No room to move at 80 columns: the nudge leaves both panes at 40.
        assert_eq!(controls_columns(nudged(0.5, 80, 1), 80), MIN_SIDE);
    }
}
