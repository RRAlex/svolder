// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One-row braille history graphs over [`svolder_profiles::history`]: the TUI's series of every
//! reading and of the host capture, and the drawing of their columns.
//!
//! A series keeps its samples as its [`Window`] says (the TUI's: 250 ms bins for 60 minutes);
//! [`columns`] turns it into `count` half-cell columns covering the span that ends at `now`;
//! [`normalise`] puts them on a 0 to 1 scale; [`draw`] packs them into braille cells, two
//! columns to a cell, four dots tall.

use std::time::{Duration, Instant};

use ratatui::style::Style;
use ratatui::text::Span;
use svolder_profiles::Profile;
use svolder_profiles::history::Readings;
use svolder_proto::FunctionId;

pub use svolder_profiles::history::{
    Aggregate, Link, Series, Window, columns, normalise, normalise_together,
};

use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::rows::own_colours;

/// Every series the TUI keeps: one per reading, by function address, and one per channel of the
/// host capture (left, right).
#[derive(Debug, Clone)]
pub struct Histories {
    /// The readings' series, created on a function's first numeric reading.
    readings: Readings,
    /// The host capture's left and right levels.
    host: [Series; 2],
}

impl Default for Histories {
    fn default() -> Self {
        Histories {
            readings: Readings::new(Window::TUI),
            host: [
                Series::new(Aggregate::Max, Window::TUI),
                Series::new(Aggregate::Max, Window::TUI),
            ],
        }
    }
}

impl Histories {
    /// Records `value` for function `id` at `at` ([`Readings::record`]).
    pub fn record(&mut self, profile: &Profile, id: FunctionId, at: Instant, value: f64) {
        self.readings.record(profile, id, at, value);
    }

    /// Start the readings' series with a daemon's `series`, answered when its wall clock read
    /// `now_ms`, `now` being this moment: each sample is placed as long before `now` as it was
    /// before `now_ms`, ahead of what the series holds ([`Readings::seed`]). A series whose id
    /// does not parse is skipped.
    pub fn seed(&mut self, series: &[svolder_ipc::SeriesOut], now: Instant, now_ms: u64) {
        for one in series {
            let Ok(id) = one.id.parse::<FunctionId>() else {
                continue;
            };
            let samples = one.points.iter().filter_map(|&(at_ms, value)| {
                let age = Duration::from_millis(now_ms.saturating_sub(at_ms));
                Some((now.checked_sub(age)?, value))
            });
            self.readings.seed(id, one.aggregate, samples);
        }
    }

    /// Records the host capture's two channel levels at `at`.
    pub fn record_host(&mut self, at: Instant, left: f64, right: f64) {
        self.host[0].push(at, left);
        self.host[1].push(at, right);
    }

    /// The series of function `id`, once it has had a numeric reading.
    pub fn reading(&self, id: FunctionId) -> Option<&Series> {
        self.readings.series(id)
    }

    /// The host capture's left and right series.
    pub fn host(&self) -> &[Series; 2] {
        &self.host
    }
}

/// The unix wall clock in milliseconds: what the link history is stamped and read against,
/// because the daemon's history crosses processes. Everything in the TUI that reads the wall
/// clock for it goes through here, so a test holds it.
pub fn wall_ms() -> u64 {
    #[cfg(test)]
    if let Some(ms) = TEST_CLOCK.with(std::cell::Cell::get) {
        return ms;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
thread_local! {
    /// The wall clock a test holds [`wall_ms`] at; `None` reads the real one. It is per thread.
    static TEST_CLOCK: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Holds [`wall_ms`] at `ms` on this thread, for the rest of the thread: each test runs on a
/// thread of its own, so none inherits another's.
#[cfg(test)]
pub fn hold_wall_ms(ms: u64) {
    TEST_CLOCK.with(|clock| clock.set(Some(ms)));
}

/// How a graph's columns become dots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One row filled from the bottom to the level, 1 to 4 dots a column; no data is blank.
    Fill,
    /// One row, a dot at the top for on (0.5 and up) and at the bottom for off, both in the OK
    /// colour: the dot's position says which.
    Dots,
    /// [`Shape::Dots`] for a fault: a cell with an active (on) dot draws in the warning colour.
    Fault,
    /// [`Shape::Dots`] for the link: up in the OK colour, a cell with a down dot in the warning
    /// colour.
    Link,
    /// Two rows: the upper fills up from the midline, the lower fills down from it.
    Mirrored,
}

/// The dots of a half-cell column from the bottom up, left column then right.
const UP: [[u32; 4]; 2] = [[0x40, 0x04, 0x02, 0x01], [0x80, 0x20, 0x10, 0x08]];

/// The dots of half-cell column `side` (0 left, 1 right) filled to `level` (0 to 1), 1 to 4 of
/// them so a present sample is never blank: the bottom ones for `upward`, the top ones otherwise.
fn fill_bits(side: usize, level: f64, upward: bool) -> u32 {
    let dots = 1 + (level.clamp(0.0, 1.0) * 3.0).round() as usize;
    (0..dots).fold(0, |bits, i| bits | UP[side][if upward { i } else { 3 - i }])
}

/// A cell's character from its dot bits: a space when none are lit.
fn glyph(bits: u32) -> char {
    match bits {
        0 => ' ',
        bits => char::from_u32(0x2800 + bits).unwrap_or(' '),
    }
}

/// The braille characters of one row of `shape` for `columns`, each with the role it draws in.
/// A dot cell draws in the warning colour when either of its columns is the shape's alarm.
fn row_cells(shape: Shape, columns: &[Option<f64>], upward: bool) -> Vec<(char, Role)> {
    columns
        .chunks(2)
        .map(|pair| match shape {
            Shape::Fill | Shape::Mirrored => {
                let bits = pair
                    .iter()
                    .enumerate()
                    .filter_map(|(side, column)| Some(fill_bits(side, (*column)?, upward)))
                    .fold(0, |all, bits| all | bits);
                (glyph(bits), Role::Bar)
            }
            Shape::Dots | Shape::Fault | Shape::Link => {
                let mut bits = 0;
                let mut alarm = false;
                for (side, column) in pair.iter().enumerate() {
                    let Some(value) = column else { continue };
                    let on = *value >= 0.5;
                    alarm |= match shape {
                        Shape::Fault => on,
                        Shape::Link => !on,
                        _ => false,
                    };
                    bits |= UP[side][if on { 3 } else { 0 }];
                }
                (glyph(bits), if alarm { Role::Warn } else { Role::MeterOk })
            }
        })
        .collect()
}

/// Groups cells into spans, one per run of the same role, styled through the theme (which drops
/// the colour in monochrome and leaves the dots alone).
fn spans(cells: Vec<(char, Role)>, theme: &Theme) -> Vec<Span<'static>> {
    let mut out: Vec<(String, Style)> = Vec::new();
    for (c, role) in cells {
        let style = match c {
            ' ' => Style::default(),
            _ => own_colours(theme.style(role)),
        };
        match out.last_mut() {
            Some((text, last)) if *last == style => text.push(c),
            _ => out.push((c.to_string(), style)),
        }
    }
    out.into_iter()
        .map(|(text, style)| Span::styled(text, style))
        .collect()
}

/// The rows of a graph: `upper` drawn in `shape`, and for [`Shape::Mirrored`] `lower` below it
/// filling downward (`lower` is ignored by the other shapes). Columns are on 0 to 1 (bools as 0
/// and 1), two to a cell; an odd last column leaves its cell's right half blank. Each row is
/// `ceil(columns / 2)` cells wide.
pub fn draw(
    shape: Shape,
    upper: &[Option<f64>],
    lower: &[Option<f64>],
    theme: &Theme,
) -> Vec<Vec<Span<'static>>> {
    let mut rows = vec![spans(row_cells(shape, upper, true), theme)];
    if shape == Shape::Mirrored {
        rows.push(spans(row_cells(shape, lower, false), theme));
    }
    rows
}

#[cfg(test)]
mod tests;
