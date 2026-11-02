// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shimmer: the bright band across the row the user just changed, and its reminders while
//! the row rests.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use tachyonfx::{Effect, Interpolation, fx};

use super::{EffectKey, Effects};
use crate::tui::mouse::Selected;

/// How fast the shimmer's band travels, in cells per second: the same on a narrow pane as on a
/// wide one, slow enough for the eye to follow it from one side of a row to the other.
pub(crate) const SHIMMER_CELLS_PER_S: f32 = 80.0;
/// The shortest the shimmer takes to cross a row, in milliseconds, so that a row only a few
/// cells wide still shows the band for a frame or two.
pub(crate) const SHIMMER_FLOOR_MS: u32 = 200;
/// How long one half of the fault pulse takes, in milliseconds: the rows take this long to
/// reach the alert colour and the same again to come back out of it.
pub(super) const PRESENCE_MS: u32 = 1_500;
/// How many cells wide the shimmer's bright band is.
pub(super) const SHIMMER_BAND: u16 = 3;
/// When the shimmer comes back over the selected row after the user's last key: soon after
/// they stop, then further and further apart, and never again until the next key.
pub(crate) const REMINDERS: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(120),
];

impl Effects {
    /// Note that the user acted at `now` (a key, or a mouse action standing in for one). Resets
    /// the reminder schedule, so the shimmer measures back from the last action, not the first.
    pub fn on_key(&mut self, now: Instant) {
        self.end_reform();
        self.keyed = Some(now);
        self.reminded = 0;
        self.acted = true;
    }

    /// Take the row the keys select as of this frame (`None` where no list has them) and say
    /// whether the user moved the cursor onto it: they acted since the last frame took this, and
    /// the cursor is in the same list as it was, on another row.
    ///
    /// A list arriving (the Telemetry screen shown, a menu opened) is no move, nor is a redraw
    /// that leaves the cursor where it was, whatever moved the rows around it: a resize and a
    /// reading the device pushed change no row's place in the list.
    pub fn on_selection(&mut self, now: Option<(Selected, usize)>) -> bool {
        let acted = std::mem::take(&mut self.acted);
        let before = std::mem::replace(&mut self.selected, now);
        match (before, now) {
            (Some((was, from)), Some((is, to))) => acted && was == is && from != to,
            _ => false,
        }
    }

    /// Whether the next reminder falls due at `now`, consuming it off the schedule: the
    /// instants of [`REMINDERS`] in turn after the user's last key, one reminder each.
    ///
    /// Only the shimmer answers a reminder, so the schedule advances only where it could be
    /// shown: not before the first key, not once the last reminder has played, and not while the
    /// layer or the shimmer's own switch is off — those leave the schedule unspent rather than
    /// consuming it unseen. The runner (the light round the focused frame) answers a focus
    /// change only ([`Effects::on_lap`]) and never this schedule.
    pub fn remind(&mut self, now: Instant) -> bool {
        if !self.enabled() || !self.config.shimmer {
            return false;
        }
        let (Some(keyed), Some(due)) = (self.keyed, REMINDERS.get(self.reminded)) else {
            return false;
        };
        if now.saturating_duration_since(keyed) < *due {
            return false;
        }
        self.reminded += 1;
        true
    }

    /// Cross `cells` with a bright band, for a row the user just changed or is being reminded
    /// of. The band is [`SHIMMER_BAND`] cells wide and travels from one side to the other at
    /// [`SHIMMER_CELLS_PER_S`] ([`shimmer_ms`] for the row), starting and ending clear of the
    /// row so every cell is lit exactly once and none is left brighter than it was.
    ///
    /// Returns whether the band was armed: false with the shimmer switch off, the layer off, or
    /// an empty row.
    pub fn on_shimmer(&mut self, cells: Rect) -> bool {
        if !self.runs(cells) || !self.config.shimmer {
            return false;
        }
        self.arm(EffectKey::Shimmer, cells, shimmer(cells.width));
        true
    }
}

/// How long the shimmer takes to cross a row `width` cells wide, in milliseconds: the row and
/// the band's own width at [`SHIMMER_CELLS_PER_S`], never less than [`SHIMMER_FLOOR_MS`].
pub(crate) fn shimmer_ms(width: u16) -> u32 {
    let cells = f32::from(width) + f32::from(SHIMMER_BAND);
    ((cells / SHIMMER_CELLS_PER_S * 1_000.0).round() as u32).max(SHIMMER_FLOOR_MS)
}

/// The shimmer itself: a bright band [`SHIMMER_BAND`] cells wide sweeping at a steady pace
/// across the effect's area, `width` cells wide, over [`shimmer_ms`].
///
/// The band changes only colour and weight, so a row says exactly what it said before the band
/// crossed it.
fn shimmer(width: u16) -> Effect {
    let lit = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    fx::effect_fn_buf(
        (),
        (shimmer_ms(width), Interpolation::Linear),
        move |(), ctx, buf| {
            let area = ctx.area;
            // Starts one band-width off the row's left and ends the same distance off its
            // right, so the band enters and leaves rather than appearing already part-lit.
            let travel = f32::from(area.width + SHIMMER_BAND);
            let front = ctx.alpha() * travel;
            let back = front - f32::from(SHIMMER_BAND);
            for x in area.x..area.right() {
                let along = f32::from(x - area.x);
                if along < back || along >= front {
                    continue;
                }
                for y in area.y..area.bottom() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_style(lit);
                    }
                }
            }
        },
    )
}
