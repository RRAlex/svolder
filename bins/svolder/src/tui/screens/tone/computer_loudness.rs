// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's Loudness row on the device screen's Tone pane: `Loudness  ‹ on › ██████░░░░
//! 50 %` and a dim readout of what the stage does at the volume the view read. The tone screen's
//! Loudness panel has its own rows ([`stages`](super::stages)). Every number is the view's.

use ratatui::style::Style;
use ratatui::text::Line;
use svolder_host::eq::loudness::LoudnessView;

use crate::tui::app::computer_loudness::{DAC_HINT, readout, strength_text};
use crate::tui::app::{App, Ld, ToneRow};
use crate::tui::mouse::Hit;
use crate::tui::screens::switch;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::crossfeed::Built;

/// Columns of the label every row opens with.
pub(in crate::tui::screens) const LABEL: usize = 10;
/// The widest the strength bar is drawn, in cells.
const BAR_MAX: usize = 10;
/// The narrowest.
const BAR_MIN: usize = 3;

/// A row put together: its line, the places in it a click lands on (cells from the line's start,
/// the cursor included) and where its bar is drawn, if it has one.
pub(in crate::tui::screens) struct LdRow {
    /// The line, the cursor in front of it where focused.
    pub line: Line<'static>,
    /// `(start, width, hit)` of each part a click lands on.
    pub spots: Vec<(usize, usize, Hit)>,
}

/// The on/off word of the row, and the blanks after its `›` that keep the bar where it is for
/// the other word.
fn word_of(view: &LoudnessView) -> (&'static str, &'static str) {
    match view.on {
        true => ("on", " "),
        false => ("off", ""),
    }
}

/// The strength row: `Loudness  ‹ on › ██████░░░░ 50 %`, in `width` cells, the focus drawn as
/// the other choices draw it ([`switch::styles`]).
pub(in crate::tui::screens) fn strength_row(
    app: &App,
    view: &LoudnessView,
    theme: &Theme,
    width: usize,
) -> LdRow {
    let plain = Style::default();
    let focused = app.tone.row == ToneRow::Ld(Ld::Strength);
    let (head, lit, other) = switch::styles(theme, plain, focused);
    let arrow = theme.style(Role::ModeArrow);
    let row = Hit::Ld(Ld::Strength);
    let value = format!(" {}", strength_text(view));
    // the cursor, the label, `‹ off ›`, a blank, the bar, the value
    let fixed = CURSOR_WIDTH + LABEL + 2 + 3 + 2 + 1 + widgets::text::cells(&value);
    let bar = width.saturating_sub(fixed).clamp(BAR_MIN, BAR_MAX);
    let mut built = Built::new();
    built.put(format!("{:<LABEL$}", Ld::On.label()), head, Some(row));
    built.put("\u{2039} ", arrow, Some(Hit::LdOn));
    let (word, pad) = word_of(view);
    built.put(
        word,
        if view.on || focused { lit } else { other },
        Some(Hit::LdOn),
    );
    built.put(" \u{203a}", arrow, Some(Hit::LdOn));
    built.put(format!(" {pad}"), plain, Some(row));
    // The bar is the strength as the view says it, over the range the view says.
    let range = view.ranges.strength_pct;
    let raw = view.strength_pct.round() as i64;
    let spans = widgets::rows::bar_spans(
        theme,
        raw,
        (range.min.round() as i64, range.max.round() as i64),
        bar,
        focused,
        |_| theme.style(Role::Bar),
    );
    built.put_spans(spans, bar, Some(Hit::LdBar));
    built.put(value, plain, Some(row));
    let (line, spots) = built.finish_marked(focused, !app.session.connected, theme, width);
    LdRow { line, spots }
}

/// The dim readout line under the Tone pane's row, indented under the cursor only.
pub(in crate::tui::screens) fn readout_row(
    app: &App,
    view: &LoudnessView,
    theme: &Theme,
    width: usize,
) -> LdRow {
    let room = width.saturating_sub(CURSOR_WIDTH);
    let text = readout(view, app.dac_loudness_on(), false, room);
    dim_row(app, theme, width, 0, text)
}

/// The dim line saying the device's own loudness plays too, under the Tone pane's readout.
pub(in crate::tui::screens) fn hint_row(app: &App, theme: &Theme, width: usize) -> LdRow {
    dim_row(app, theme, width, 0, DAC_HINT.to_string())
}

/// A dim line `indent` cells in from the cursor's column, a click on it being the strength row.
fn dim_row(app: &App, theme: &Theme, width: usize, indent: usize, text: String) -> LdRow {
    let mut built = Built::new();
    built.put(
        " ".repeat(indent),
        Style::default(),
        Some(Hit::Ld(Ld::Strength)),
    );
    built.put(text, theme.style(Role::Dim), Some(Hit::Ld(Ld::Strength)));
    let (line, spots) = built.finish(false, !app.session.connected, theme, width);
    LdRow { line, spots }
}

#[cfg(test)]
mod tests;
