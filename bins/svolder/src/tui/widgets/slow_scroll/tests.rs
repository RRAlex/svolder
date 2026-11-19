// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The offsets a scroll of a line cut by `travel` cells reads at each of `at` (milliseconds
/// after it began), on a clock the test moves: no real time passes.
fn offsets(travel: usize, at: &[u64]) -> Vec<u16> {
    let start = Instant::now();
    let mut scroll = SlowScroll::new(start);
    at.iter()
        .map(|ms| {
            scroll.advance(start + Duration::from_millis(*ms), Some(travel), false);
            scroll.offset
        })
        .collect()
}

/// The line holds 2 s on its start, glides a cell every 250 ms, holds 2 s on its end and jumps
/// back to go again.
#[test]
fn the_line_holds_glides_holds_and_jumps_back() {
    // Travel 8: the glide takes 2 s, so a round is 6 s.
    assert_eq!(
        offsets(
            8,
            &[
                0, 1_000, 1_999, 2_000, 2_249, 2_250, 3_000, 3_999, 4_000, 5_999, 6_000, 6_001,
                8_000, 8_250
            ]
        ),
        vec![0, 0, 0, 0, 0, 1, 4, 7, 8, 8, 0, 0, 0, 1]
    );
}

/// The glide is four cells a second: 4 s into a long line it has moved 8 cells.
#[test]
fn the_glide_goes_four_cells_a_second() {
    assert_eq!(offsets(100, &[2_000 + 4_000]), vec![16]);
    assert_eq!(offsets(100, &[2_000 + 10_000]), vec![40]);
}

/// A tick that comes late or twice reads the same offset for the same moment.
#[test]
fn a_late_or_doubled_tick_reads_the_same_offset() {
    assert_eq!(offsets(40, &[5_000, 5_000, 9_999]), vec![12, 12, 31]);
}

/// A line that is not cut (or not in sight) sits at its start and starts its hold afresh when it
/// is cut again.
#[test]
fn a_line_not_cut_rests_at_its_start() {
    let start = Instant::now();
    let mut scroll = SlowScroll::new(start);
    scroll.advance(start + Duration::from_millis(5_000), Some(40), false);
    assert_eq!(scroll.offset, 12);
    let changed = scroll.advance(start + Duration::from_millis(6_000), None, false);
    assert!(changed && scroll.offset == 0, "not sent back");
    // Cut again: the 2 s hold runs from the moment it was reset.
    scroll.advance(start + Duration::from_millis(7_000), Some(40), false);
    scroll.advance(start + Duration::from_millis(7_900), Some(40), false);
    assert_eq!(scroll.offset, 0);
    scroll.advance(start + Duration::from_millis(8_250), Some(40), false);
    assert_eq!(scroll.offset, 1);
}

/// While the scroll is held still (the pointer on the row, the keys on the transport) it stays
/// where it is, and the time spent still does not count when it goes on.
#[test]
fn a_still_scroll_stays_and_loses_no_time_to_it() {
    let start = Instant::now();
    let ms = |ms| start + Duration::from_millis(ms);
    let mut scroll = SlowScroll::new(start);
    scroll.advance(ms(3_000), Some(40), false);
    assert_eq!(scroll.offset, 4);
    for t in [4_000, 20_000, 60_000] {
        let changed = scroll.advance(ms(t), Some(40), true);
        assert!(!changed && scroll.offset == 4, "moved while still at {t}");
    }
    // Let go at 60 s: it goes on from cell 4, a step 250 ms on.
    scroll.advance(ms(60_000), Some(40), false);
    assert_eq!(scroll.offset, 4);
    scroll.advance(ms(60_250), Some(40), false);
    assert_eq!(scroll.offset, 5);
}

/// `advance` says when the offset changed, which is when a redraw shows something new.
#[test]
fn advance_reports_only_a_changed_offset() {
    let start = Instant::now();
    let mut scroll = SlowScroll::new(start);
    let at = |ms| start + Duration::from_millis(ms);
    assert!(!scroll.advance(at(500), Some(40), false));
    assert!(!scroll.advance(at(1_999), Some(40), false));
    assert!(scroll.advance(at(2_250), Some(40), false));
    assert!(!scroll.advance(at(2_300), Some(40), false));
    assert!(scroll.advance(at(2_500), Some(40), false));
}
