// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The marquee, which carries words too wide for their row past it: the Status pane's
//! `Now playing` row.

use std::time::{Duration, Instant};

/// How long the marquee holds a cell before moving to the next, which is the app's own idle
/// tick: the text moves at reading pace and never asks the loop to redraw faster.
const STEP: Duration = Duration::from_millis(250);
/// How long it rests at each end of the text before setting off again, so the first and last
/// keys can be read.
const PAUSE: Duration = Duration::from_secs(1);
/// How much before its time a step is taken anyway: the loop's 250 ms clock wakes a hair before
/// or after each of its deadlines, and a step must not wait a whole further tick for that.
const SLACK: Duration = Duration::from_millis(25);

/// Where a line too wide for its room has been carried, and how much further it goes on its
/// own: the Status pane's `Now playing` row.
///
/// The offset is in cells from the start of the line. The line goes out to the far end and back
/// for as long as it is too wide.
#[derive(Debug, Clone, Copy)]
pub struct Marquee {
    /// The first cell of the line drawn from.
    pub offset: u16,
    /// When the offset last moved, which is what paces it.
    pub last_step: Instant,
    /// Whether the line is on its way back to where it set off.
    back: bool,
}

impl Marquee {
    /// A marquee about to carry a line out and back, as of `now`.
    pub fn new(now: Instant) -> Marquee {
        Marquee {
            offset: 0,
            last_step: now,
            back: false,
        }
    }

    /// Send the line out again from where it set off, which is what a new track on the
    /// `Now playing` row earns.
    pub fn reset(&mut self, now: Instant) {
        *self = Marquee::new(now);
    }

    /// Carry the line on by a cell if it is time, with `text` cells of line to show in
    /// `room` cells of screen.
    ///
    /// One cell per [`STEP`], with [`PAUSE`] at each end of the travel. A line that fits rests at
    /// the start.
    ///
    /// Each step is timed from when the one before it was due, not from when it was taken, and
    /// may be taken up to [`SLACK`] early, so a clock ticking every [`STEP`] moves the line a
    /// cell every tick however each tick lands around its deadline, and a faster clock moves it
    /// a cell per step all the same. A step taken more than a step late (the loop held up for a
    /// while) times the next one from now instead, so the line never races to catch up.
    pub fn advance(&mut self, now: Instant, text: usize, room: usize) {
        let travel = u16::try_from(text.saturating_sub(room)).unwrap_or(u16::MAX);
        if travel == 0 {
            self.offset = 0;
            return;
        }
        self.offset = self.offset.min(travel);
        let due = match self.offset == 0 || self.offset == travel {
            true => PAUSE,
            false => STEP,
        };
        let since = now.saturating_duration_since(self.last_step);
        if since + SLACK < due {
            return;
        }
        self.last_step = match since < due + STEP {
            true => self.last_step + due,
            false => now,
        };
        match self.back {
            true => {
                self.offset -= 1;
                if self.offset == 0 {
                    self.back = false;
                }
            }
            false => {
                self.offset += 1;
                self.back = self.offset >= travel;
            }
        }
    }

    /// Where the line is drawn from, with `travel` cells of it past the right edge.
    pub fn at(&self, travel: usize) -> usize {
        usize::from(self.offset).min(travel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far the marquee has carried a line of `text` cells through `room` of screen after
    /// `ticks` of the app's own tick, starting from a line that is about to set off.
    fn carried(text: usize, room: usize, ticks: u32) -> Vec<u16> {
        let start = Instant::now();
        let mut marquee = Marquee::new(start);
        (1..=ticks)
            .map(|tick| {
                marquee.advance(start + STEP * tick, text, room);
                marquee.offset
            })
            .collect()
    }

    #[test]
    fn a_line_that_fits_never_moves() {
        assert!(carried(20, 40, 40).iter().all(|offset| *offset == 0));
    }

    #[test]
    fn the_marquee_goes_out_and_back_for_as_long_as_the_line_is_too_wide() {
        let start = Instant::now();
        let mut marquee = Marquee::new(start);
        let offsets: Vec<u16> = (1..=200)
            .map(|tick| {
                marquee.advance(start + STEP * tick, 44, 40);
                marquee.offset
            })
            .collect();
        let ends = offsets
            .windows(2)
            .filter(|pair| pair[0] == 3 && pair[1] == 4)
            .count();
        assert!(ends > 2, "the line stopped going out: {offsets:?}");
        marquee.reset(start);
        assert_eq!(marquee.offset, 0, "a reset marquee did not start again");
    }

    /// Ticks of a 250 ms clock that land a few microseconds either side of their deadlines
    /// still move the line a cell each, once the pause at the start is over.
    #[test]
    fn a_clock_a_hair_early_or_late_still_moves_the_line_a_cell_a_tick() {
        let start = Instant::now();
        let mut marquee = Marquee::new(start);
        let offsets: Vec<u16> = (4..=12u32)
            .map(|tick| {
                let on_time = start + STEP * tick;
                let at = match tick % 3 {
                    0 => on_time - Duration::from_micros(3),
                    1 => on_time + Duration::from_micros(2),
                    _ => on_time,
                };
                marquee.advance(at, 60, 40);
                marquee.offset
            })
            .collect();
        assert_eq!(offsets, (1..=9).collect::<Vec<u16>>(), "not a cell a tick");
    }

    /// A fast clock moves the line a cell per step, as the slow one does, and a loop held up for
    /// a while does not send the line racing to catch up.
    #[test]
    fn a_fast_or_a_held_up_clock_keeps_the_pace() {
        let start = Instant::now();
        let mut marquee = Marquee::new(start);
        let fast = Duration::from_millis(33);
        let mut at = start + PAUSE;
        marquee.advance(at, 200, 40);
        let from = marquee.offset;
        for _ in 0..150 {
            at += fast;
            marquee.advance(at, 200, 40);
        }
        // 150 ticks of 33 ms are 4.95 s: nineteen steps of 250 ms, the twentieth not yet due.
        assert_eq!(
            marquee.offset - from,
            19,
            "not a cell per step on a fast clock"
        );

        let held = marquee.offset;
        at += Duration::from_secs(10);
        marquee.advance(at, 200, 40);
        marquee.advance(at + fast, 200, 40);
        assert_eq!(marquee.offset, held + 1, "a held-up loop raced to catch up");
    }

    #[test]
    fn the_line_holds_still_at_each_end() {
        let start = Instant::now();
        let mut marquee = Marquee::new(start);
        // Half a second at the start is still inside the pause.
        marquee.advance(start + Duration::from_millis(500), 44, 40);
        assert_eq!(marquee.offset, 0);
        marquee.advance(start + PAUSE, 44, 40);
        assert_eq!(marquee.offset, 1, "the line never set off");
    }
}
