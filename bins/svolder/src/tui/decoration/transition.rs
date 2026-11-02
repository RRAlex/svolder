// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How one screen gives way to the next, for the two `[tui]` transitions that draw over the
//! whole frame: `noise` and `melt`.
//!
//! Each is a passage of its own length that starts on a user-driven screen change and paints
//! over the frame the loop has just drawn, last of all: the new screen is drawn in full beneath
//! it every frame, so every key acts on it at once and every reading arrives as it arrives (the
//! passage only covers it, for a moment, and says nothing of its own). Unlike the rest of the
//! animation layer, these do change what a cell says while they run, which is why they last only
//! a fraction of a second, and why a frame drawn with no passage running is the screen alone.
//!
//! Nothing here reaches for randomness: which cell clears when is a hash of where it is, so a
//! passage plays the same way every time and a test can say what it draws.
//!
//! The trace, the transition that draws a pane round as it arrives, is the animation layer's own
//! ([`Effects::on_arrival`](super::effects::Effects::on_arrival)).

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::tui::theme::{Role, Theme};

/// How long the noise takes to clear, in milliseconds.
pub(crate) const NOISE_MS: u32 = 250;
/// The shade glyphs the noise is drawn in, densest first: a cell a long way from clearing is
/// the densest of them, and one about to clear the lightest.
const SHADES: [&str; 3] = ["▓", "▒", "░"];
/// How finely a cell's moment is chosen, in steps across the passage: fine enough that the
/// share of covered cells falls smoothly rather than in a few jumps.
const MOMENTS: u32 = 1_000;
/// How finely a noise cell's place along the palette's spectrum is chosen, in steps across
/// each half of it.
const SPREAD: u32 = 1_000;
/// What a cell's moment is hashed with, so that when it clears and what colour it is drawn
/// in are two unrelated things.
const MOMENT_SALT: u32 = 0;
/// What a cell's colour is hashed with.
const COLOUR_SALT: u32 = 1;
/// How long the melt takes, from its start to the last column gone off the bottom, in
/// milliseconds: the same at every width and every height.
pub(crate) const MELT_MS: u32 = 600;
/// How much later than the first column the last one starts to fall, in milliseconds.
const STAGGER_MS: u32 = 150;
/// How long one column takes to fall the whole height of the screen once it has started, in
/// milliseconds: what the melt leaves of its length once the last column has started.
const FALL_MS: u32 = MELT_MS - STAGGER_MS;
/// How many steps the columns' starts are chosen in: from one column to the next the start
/// moves a step at most either way, so neighbouring columns fall close together and the
/// bottom edge of the old screen is a ragged line rather than scattered teeth.
const STAGGER_STEPS: u32 = 15;
/// What a column's start is hashed with.
const MELT_SALT: u32 = 2;

/// One screen giving way to the next.
#[derive(Debug, Clone)]
pub(crate) struct Passage {
    /// Which transition this is.
    kind: Kind,
    /// How long it has been playing.
    elapsed: Duration,
    /// Whether it has painted a frame yet: its first is its very start.
    started: bool,
    /// The frame it plays over — the old screen's own for the melt, and whatever the first
    /// paint finds for the noise — which a frame of another size, a terminal resized under
    /// it, ends.
    area: Option<Rect>,
}

/// What one passage draws.
#[derive(Debug, Clone)]
enum Kind {
    /// The new screen through a field of shade glyphs that thins to nothing.
    Noise,
    /// The old screen's columns sliding down off the bottom.
    Melt {
        /// The last frame the terminal showed before the screen changed.
        old: Buffer,
        /// How long each column waits before it starts to fall, in milliseconds, left to
        /// right.
        delays: Vec<u32>,
    },
}

impl Passage {
    /// The noise, from its very start: every cell covered.
    pub(crate) fn noise() -> Passage {
        Passage {
            kind: Kind::Noise,
            elapsed: Duration::ZERO,
            started: false,
            area: None,
        }
    }

    /// The melt of `old`, the last frame the terminal showed, from its very start: the old
    /// screen whole, over the new one.
    pub(crate) fn melt(old: Buffer) -> Passage {
        let area = old.area;
        Passage {
            kind: Kind::Melt {
                delays: delays(area.width),
                old,
            },
            elapsed: Duration::ZERO,
            started: false,
            area: Some(area),
        }
    }

    /// How long this passage plays for.
    fn length(&self) -> Duration {
        let ms = match self.kind {
            Kind::Noise => NOISE_MS,
            Kind::Melt { .. } => MELT_MS,
        };
        Duration::from_millis(u64::from(ms))
    }

    /// Whether this passage has played out.
    pub(crate) fn over(&self) -> bool {
        self.elapsed >= self.length()
    }

    /// Paint this passage over `buf`, the frame just drawn, `last_tick` further on than the
    /// frame before it — or, on its first frame, at its very start whatever `last_tick` says,
    /// since the frame before this one was drawn before the screen changed at all — and say
    /// whether it is still playing.
    ///
    /// A frame of another size than the first one it painted is the terminal resized under
    /// it: the passage ends there, painting nothing, and the screen is drawn as it is.
    pub(crate) fn paint(&mut self, buf: &mut Buffer, theme: &Theme, last_tick: Duration) -> bool {
        if *self.area.get_or_insert(buf.area) != buf.area {
            self.elapsed = self.length();
            return false;
        }
        match self.started {
            true => self.elapsed += last_tick,
            false => self.started = true,
        }
        if self.over() {
            return false;
        }
        match &self.kind {
            Kind::Noise => {
                let alpha = self.elapsed.as_secs_f32() / self.length().as_secs_f32();
                noise(buf, theme, alpha);
            }
            Kind::Melt { old, delays } => melt(buf, old, delays, self.elapsed),
        }
        true
    }
}

/// The noise `alpha` of the way through, over the whole of `buf`: a cell whose moment has not
/// come yet is covered by a shade glyph in its own colour along the palette's spectrum, and one
/// whose moment has passed shows the screen beneath.
///
/// At 0 every cell is covered and at 1 none is; a cell cleared at one alpha is clear at every
/// later one, so the covered share only ever falls. At 16 colours and in monochrome, where the
/// palette has no spread of colours to draw in, the glyphs take the dim style instead.
fn noise(buf: &mut Buffer, theme: &Theme, alpha: f32) {
    let area = buf.area;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let moment = moment(x, y);
            if alpha >= moment {
                continue;
            }
            // How much of its wait the cell has still to go, which picks how dense its glyph is.
            let left = (moment - alpha) / moment;
            let shade = match left {
                left if left > 2.0 / 3.0 => SHADES[0],
                left if left > 1.0 / 3.0 => SHADES[1],
                _ => SHADES[2],
            };
            let style = match theme.along_spectrum(place(x, y)) {
                Some(colour) => Style::default().fg(colour),
                None => theme.style(Role::Dim),
            };
            if let Some(cell) = buf.cell_mut((x, y)) {
                let ground = cell.bg;
                cell.reset();
                cell.set_symbol(shade).set_style(style);
                cell.bg = ground;
            }
        }
    }
}

/// The melt `elapsed` into it, over the whole of `buf`: each column of `old` slid down by as
/// many rows as it has fallen, the new screen showing above it, and nothing of `old` left in a
/// column that has fallen the whole height.
///
/// At its start every column is where it was, so the frame is `old` exactly; each column then
/// waits its own delay and falls at the one speed every column falls at, [`FALL_MS`] for the
/// whole height of the screen, so the last of them is gone at [`MELT_MS`] however tall the
/// screen is.
fn melt(buf: &mut Buffer, old: &Buffer, delays: &[u32], elapsed: Duration) {
    let area = buf.area;
    for (x, delay) in (area.x..area.right()).zip(delays) {
        let offset = fallen(*delay, elapsed, area.height);
        for y in area.y + offset..area.bottom() {
            if let (Some(from), Some(to)) = (old.cell((x, y - offset)), buf.cell_mut((x, y))) {
                *to = from.clone();
            }
        }
    }
}

/// How many rows a column that waits `delay` milliseconds before it falls has slid down,
/// `elapsed` into the melt, on a screen `height` rows tall: none before its delay is up, all
/// of them once [`FALL_MS`] has gone by after it, and never fewer at a later moment than at an
/// earlier one.
fn fallen(delay: u32, elapsed: Duration, height: u16) -> u16 {
    let falling = elapsed.as_secs_f32() * 1_000.0 - delay as f32;
    let share = (falling / FALL_MS as f32).clamp(0.0, 1.0);
    (share * f32::from(height)) as u16
}

/// How long each of `width` columns waits before it falls, in milliseconds, left to right.
///
/// The starts take a walk along the columns, over [`STAGGER_STEPS`] steps — one step at most
/// either way from one column to the next, each a hash of the column — and the walk is spread
/// over `0..=`[`STAGGER_MS`] whatever ground it covered, so the first column to fall starts at
/// once and the last starts [`STAGGER_MS`] in, at every width.
fn delays(width: u16) -> Vec<u32> {
    let mut at = hash(0, 0, MELT_SALT) % (STAGGER_STEPS + 1);
    let steps: Vec<u32> = (0..width)
        .map(|column| {
            if column > 0 {
                // 0, 1 or 2: a step back, none, or a step on.
                let step = hash(column, 0, MELT_SALT) % 3;
                at = (at + step).saturating_sub(1).min(STAGGER_STEPS);
            }
            at
        })
        .collect();
    let low = steps.iter().copied().min().unwrap_or(0);
    let high = steps.iter().copied().max().unwrap_or(0);
    steps
        .iter()
        .map(|step| match high > low {
            true => (step - low) * STAGGER_MS / (high - low),
            false => 0,
        })
        .collect()
}

/// When the cell at `(x, y)` clears, as a share of the passage, `0.0..=1.0` and never 0 itself,
/// so every cell is covered at the very start.
fn moment(x: u16, y: u16) -> f32 {
    (hash(x, y, MOMENT_SALT) % MOMENTS + 1) as f32 / MOMENTS as f32
}

/// Where along the palette's spectrum the cell at `(x, y)` is drawn, `-1.0..=1.0`.
fn place(x: u16, y: u16) -> f32 {
    (hash(x, y, COLOUR_SALT) % (2 * SPREAD + 1)) as f32 / SPREAD as f32 - 1.0
}

/// A well-mixed 32-bit hash of a cell's position and `salt`: the same position always gives
/// the same answer, and neighbouring positions give unrelated ones.
fn hash(x: u16, y: u16, salt: u32) -> u32 {
    let mut h = (u32::from(x) << 16 | u32::from(y)) ^ salt.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    h
}

#[cfg(test)]
mod tests;
