// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Loudness section of the tone screen's computer side, and the row the device screen's Tone
//! pane shares with it.
//!
//! Folded, it is two lines under the Points table, in the chain's order before Crossfeed:
//! `Loudness  ‹ on › ██████░░░░ 50 %` and a dim readout of what the stage does at the volume
//! the view read. It unfolds while the keys are in it ([`App::ld_unfolded`]): the Reference row
//! comes under the strength row and the readout under that. Every number is the view's.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
use svolder_host::eq::loudness::LoudnessView;

use crate::tui::app::computer_loudness::{DAC_HINT, readout, reference_text, strength_text};
use crate::tui::app::{App, Ld, ToneRow};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens::device;
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

/// The rows the section takes: none without loudness to show, two folded (the strength row and
/// its readout), three unfolded (the Reference row too).
pub(super) fn height(app: &App) -> u16 {
    match (app.ld_shown(), app.ld_unfolded()) {
        (false, _) => 0,
        (true, false) => 2,
        (true, true) => 3,
    }
}

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
    built.put(format!("{:<LABEL$}", Ld::Strength.label()), head, Some(row));
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

/// The Reference row: `Reference  ‹ 100 % ›  r: use current volume (35 %)`.
fn reference_row(app: &App, view: &LoudnessView, theme: &Theme, width: usize) -> LdRow {
    let plain = Style::default();
    let focused = app.tone.row == ToneRow::Ld(Ld::Reference);
    let (head, lit, _) = switch::styles(theme, plain, focused);
    let arrow = theme.style(Role::ModeArrow);
    let row = Hit::Ld(Ld::Reference);
    let mut built = Built::new();
    built.put(
        format!("{:<LABEL$}", Ld::Reference.label()),
        head,
        Some(row),
    );
    built.put("\u{2039} ", arrow, Some(Hit::LdStep(Ld::Reference, -1)));
    built.put(
        reference_text(view),
        if focused { lit } else { plain },
        Some(row),
    );
    built.put(" \u{203a}", arrow, Some(Hit::LdStep(Ld::Reference, 1)));
    let key = match view.volume_pct {
        Some(volume) => format!("  r: use the current volume ({volume:.0} %)"),
        None => "  r: needs the volume, not read yet".to_string(),
    };
    let room = width.saturating_sub(built.col());
    built.put(
        widgets::text::cut_to_cells(&key, room),
        theme.style(Role::Dim),
        None,
    );
    let (line, spots) = built.finish_marked(focused, !app.session.connected, theme, width);
    LdRow { line, spots }
}

/// Where a readout line sits under its row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::screens) enum Under {
    /// The tone screen's: indented under the label, with the treble gain while unfolded.
    Screen { treble: bool },
    /// The Tone pane's: indented under the cursor only, the dim line the pick shows.
    Pane,
}

/// The dim readout line under the rows.
pub(in crate::tui::screens) fn readout_row(
    app: &App,
    view: &LoudnessView,
    theme: &Theme,
    width: usize,
    under: Under,
) -> LdRow {
    let (indent, treble) = match under {
        Under::Screen { treble } => (LABEL, treble),
        Under::Pane => (0, false),
    };
    let room = width.saturating_sub(CURSOR_WIDTH + indent);
    let text = readout(view, app.dac_loudness_on(), treble, room);
    dim_row(app, theme, width, indent, text)
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

/// Draw one built row into `cell` and push the hits of its parts; the bar's cells are pushed as
/// a bar, so a press reads a value off them.
pub(in crate::tui::screens) fn put_row(
    frame: &mut Frame,
    hits: &mut HitMap,
    cell: Rect,
    row: LdRow,
) {
    frame.render_widget(Paragraph::new(row.line), cell);
    for (at, cells, hit) in row.spots {
        let spot = Rect {
            x: cell.x.saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: u16::try_from(cells).unwrap_or(0),
            ..cell
        };
        match hit {
            Hit::LdBar => hits.push_bar(spot.intersection(cell), spot, hit),
            _ => hits.push(spot.intersection(cell), hit),
        }
    }
}

/// The block beside the Points table while the section is unfolded in the wide tier: the stage's
/// curve as the view draws it, in dB against frequency over 20 Hz to 20 kHz.
pub(super) fn draw_block(frame: &mut Frame, area: Rect, app: &App, (theme, tier): (&Theme, Tier)) {
    let Some(view) = app.loudness_view() else {
        return;
    };
    let block = tier
        .block()
        .border_style(device::contour(
            app,
            theme,
            true,
            device::border_style(app, theme),
        ))
        .title(Span::styled(" Loudness ", device::pane_title(theme, true)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if view.curve.hz.is_empty() || inner.height < 4 {
        return;
    }
    let key = Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled(
            format!("\u{2500} the stage at {:.1} dB depth", view.depth_db),
            theme.style(Role::CurveSum),
        ),
    ]);
    frame.render_widget(Paragraph::new(key), Rect { height: 1, ..inner });
    let low = view
        .curve
        .db
        .iter()
        .copied()
        .fold(0.0_f64, f64::min)
        .floor()
        - 1.0;
    let data: Vec<(f64, f64)> = view
        .curve
        .hz
        .iter()
        .zip(&view.curve.db)
        .map(|(hz, db)| (hz.log10(), *db))
        .collect();
    let axis = theme.style(Role::Axis);
    // the 0 dB line, dim, as the Response graph has one
    let zero = [(20f64.log10(), 0.0), (20_000f64.log10(), 0.0)];
    let chart = Chart::new(vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.style(Role::Dim))
            .data(&zero),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.style(Role::CurveSum))
            .data(&data),
    ])
    .x_axis(
        Axis::default()
            .bounds([20f64.log10(), 20_000f64.log10()])
            .style(axis),
    )
    .y_axis(Axis::default().bounds([low, 1.0]).style(axis));
    frame.render_widget(
        chart,
        Rect {
            y: inner.y + 1,
            height: inner.height - 1,
            ..inner
        },
    );
}

/// Draw the section into `area`: the strength row, the Reference row while unfolded, and the
/// readout line. Pushes the hits.
pub(super) fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, hits: &mut HitMap) {
    let Some(view) = app.loudness_view() else {
        return;
    };
    let width = usize::from(area.width);
    let mut rows = vec![strength_row(app, view, theme, width)];
    if app.ld_unfolded() {
        rows.push(reference_row(app, view, theme, width));
    }
    let treble = app.ld_unfolded();
    rows.push(readout_row(
        app,
        view,
        theme,
        width,
        Under::Screen { treble },
    ));
    for (at, row) in rows.into_iter().enumerate() {
        let y = area.y + u16::try_from(at).unwrap_or(0);
        if y >= area.bottom() {
            break;
        }
        let cell = Rect {
            y,
            height: 1,
            ..area
        };
        put_row(frame, hits, cell, row);
    }
}

#[cfg(test)]
mod tests;
