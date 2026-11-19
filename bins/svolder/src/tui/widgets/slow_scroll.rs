// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slow scroll of a cut line: it holds on its start, glides left until its end shows, holds
//! there, jumps back and goes again. The Controls pane's Path row, where it has no spare rows
//! to wrap onto.

use std::time::{Duration, Instant};

/// How long the line holds on its start, and again on its end.
pub const HOLD: Duration = Duration::from_secs(2);
/// How long the glide takes for one cell: four cells a second, which is the app's own idle tick.
pub const CELL: Duration = Duration::from_millis(250);

/// Where a cut line has been scrolled to, worked out from a clock.
///
/// The offset is a function of the time the scroll has run, so a loop that ticks late or twice
/// shows the same offset for the same moment; time spent still is not counted.
#[derive(Debug, Clone, Copy)]
pub struct SlowScroll {
    /// The first cell of the line drawn from.
    pub offset: u16,
    /// When the cycle began, moved on by every moment spent still.
    began: Instant,
    /// When the scroll was last advanced.
    last: Instant,
}

impl SlowScroll {
    /// A scroll at its start as of `now`.
    pub fn new(now: Instant) -> SlowScroll {
        SlowScroll {
            offset: 0,
            began: now,
            last: now,
        }
    }

    /// Bring the offset to `now` for a line that is cut by `travel` cells (`None`: not cut, or
    /// not in sight, which sends the scroll back to its start and keeps it there). While `still`
    /// the offset stays where it is and the time passing does not count. Answers whether the
    /// offset changed.
    pub fn advance(&mut self, now: Instant, travel: Option<usize>, still: bool) -> bool {
        let before = self.offset;
        let Some(travel) = travel else {
            *self = SlowScroll::new(now);
            return before != 0;
        };
        let passed = now.saturating_duration_since(self.last);
        self.last = now;
        if still {
            self.began += passed;
            return false;
        }
        self.offset = offset_at(now.saturating_duration_since(self.began), travel);
        self.offset != before
    }
}

/// The offset a cut line of `travel` cells is at once the scroll has run for `elapsed`: the
/// first [`HOLD`] on the start, then a cell every [`CELL`] to the end, [`HOLD`] there, and round
/// again from the start.
pub fn offset_at(elapsed: Duration, travel: usize) -> u16 {
    let glide = CELL * u32::try_from(travel).unwrap_or(u32::MAX);
    let round = HOLD + glide + HOLD;
    let into = Duration::from_nanos(
        u64::try_from(elapsed.as_nanos() % round.as_nanos().max(1)).unwrap_or(0),
    );
    let moved = match into.checked_sub(HOLD) {
        None => 0,
        Some(gone) => usize::try_from(gone.as_millis() / CELL.as_millis())
            .unwrap_or(usize::MAX)
            .min(travel),
    };
    u16::try_from(moved).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests;
