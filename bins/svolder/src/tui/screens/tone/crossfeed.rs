// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Crossfeed section of the tone screen's computer side.
//!
//! Folded, it is one line between the Response graph and the Points table: `Crossfeed  on ·
//! Bauer · Chu Moy · 33 % · 700 Hz · 0.26 ms  (X)`. It unfolds while the keys are in it
//! ([`App::xf_unfolded`]): the line becomes the Model and Preset row and the rows Level,
//! Crossover and Delay follow, under the line in the narrow tiers (taking the graph's rows only
//! while focused) and, from 120 columns, in a block beside the Points table with the two
//! curves of the view's `curve`, so the graph keeps its height.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
use svolder_host::eq::crossfeed::CrossfeedView;

use crate::tui::app::crossfeed::{self, XfLine};
use crate::tui::app::{App, ToneRow, Xf};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens::device::{self, row_style};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH};

/// Columns of the label every row opens with.
const LABEL: usize = 10;
/// Rows of values the unfolded section has under its line: Level, Crossover and Delay.
const VALUE_ROWS: u16 = 3;
/// Columns the block beside the Points table takes in the wide tier.
pub(super) const BESIDE_WIDTH: u16 = 40;
/// What the folded line ends on, which `X` does.
const FOLDED_KEY: &str = "(X)";
/// The curves' vertical range in dB.
const CURVE_DB: [f64; 2] = [-30.0, 3.0];

/// The rows the section takes on a frame `area` large.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Layout {
    /// Rows of its line: 1 with crossfeed to show, else none.
    pub line: u16,
    /// Rows under the line, taken from the graph while the keys are in the section.
    pub below: u16,
    /// Whether the rest of the section sits beside the Points table.
    pub beside: bool,
}

impl Layout {
    /// No section: a daemon without crossfeed, or the device's side.
    pub const NONE: Layout = Layout {
        line: 0,
        below: 0,
        beside: false,
    };

    /// The rows the line and what sits under it take together.
    pub fn height(self) -> u16 {
        self.line + self.below
    }
}

/// The section's layout for `app` on a frame `area` large.
pub(super) fn layout(app: &App, area: Rect) -> Layout {
    if !app.crossfeed_shown() {
        return Layout::NONE;
    }
    let unfolded = app.xf_unfolded();
    let wide = Tier::of(area).wide;
    Layout {
        line: 1,
        below: if unfolded && !wide { VALUE_ROWS } else { 0 },
        beside: unfolded && wide,
    }
}

/// A row being put together, with the places in it a click lands on, in cells from its start.
struct Built {
    spans: Vec<Span<'static>>,
    hits: Vec<(usize, usize, Hit)>,
    col: usize,
}

impl Built {
    fn new() -> Built {
        Built {
            spans: Vec::new(),
            hits: Vec::new(),
            col: CURSOR_WIDTH,
        }
    }

    /// Append `text` in `style`, a click on it being `hit`.
    fn put(&mut self, text: impl Into<String>, style: ratatui::style::Style, hit: Option<Hit>) {
        let text = text.into();
        let width = widgets::text::cells(&text);
        if let Some(hit) = hit {
            self.hits.push((self.col, width, hit));
        }
        self.col += width;
        self.spans.push(Span::styled(text, style));
    }

    /// The line, the cursor in front of it, and where the row stands out when `focused`.
    fn finish(
        self,
        focused: bool,
        stale: bool,
        theme: &Theme,
        width: usize,
    ) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
        let line = widgets::rows::with_cursor(Line::from(self.spans), focused, theme);
        (row_style(line, focused, stale, theme, width), self.hits)
    }
}

/// The Model and Preset row, which is the folded section's line once the keys are in it:
/// `Crossfeed  on  ‹ Bauer ›  ‹ Chu Moy ›`.
fn preset_row(
    app: &App,
    view: &CrossfeedView,
    theme: &Theme,
    width: usize,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let plain = ratatui::style::Style::default();
    let (lit, dim) = (theme.style(Role::SelectName), theme.style(Role::Dim));
    let arrow = theme.style(Role::ModeArrow);
    let row = Hit::Crossfeed(Xf::Preset);
    let model = view
        .models
        .iter()
        .find(|m| m.word == view.model)
        .map_or(view.model.as_str(), |m| m.name.as_str());
    let preset = view.preset.as_deref().unwrap_or("Custom");
    let mut built = Built::new();
    built.put(format!("{:<LABEL$}", Xf::Preset.label()), plain, Some(row));
    built.put(
        if view.on { "on " } else { "off" },
        if view.on { lit } else { dim },
        Some(Hit::CrossfeedOn),
    );
    built.put("  ", plain, Some(row));
    built.put("\u{2039} ", arrow, Some(Hit::CrossfeedModel(-1)));
    built.put(model.to_string(), plain, Some(row));
    built.put(" \u{203a}", arrow, Some(Hit::CrossfeedModel(1)));
    built.put("  ", plain, Some(row));
    built.put("\u{2039} ", arrow, Some(Hit::CrossfeedStep(Xf::Preset, -1)));
    built.put(preset.to_string(), plain, Some(row));
    built.put(" \u{203a}", arrow, Some(Hit::CrossfeedStep(Xf::Preset, 1)));
    if let Some(from) = view.edited_from.as_ref().filter(|_| view.preset.is_none()) {
        built.put(format!("  edited from {from}"), dim, None);
    }
    let focused = app.tone.row == ToneRow::Crossfeed(Xf::Preset);
    built.finish(focused, !app.session.connected, theme, width)
}

/// A value row: `Level      ‹ 37 % · −8.6 dB ›`, or the edit line in its place.
fn value_row(
    app: &App,
    view: &CrossfeedView,
    row: Xf,
    theme: &Theme,
    width: usize,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let plain = ratatui::style::Style::default();
    let dim = theme.style(Role::Dim);
    let arrow = theme.style(Role::ModeArrow);
    let focused = app.tone.row == ToneRow::Crossfeed(row);
    let mut built = Built::new();
    built.put(
        format!("{:<LABEL$}", row.label()),
        plain,
        Some(Hit::Crossfeed(row)),
    );
    if let Some(line) = app.tone.xf_line.as_ref().filter(|line| line.row == row) {
        built.put(edit_text(line), plain, None);
        return built.finish(true, false, theme, width);
    }
    let settable = row != Xf::Delay || view.delay_settable;
    let value = match row {
        Xf::Level => crossfeed::amount_text(view),
        Xf::Crossover => crossfeed::crossover_text(view),
        _ => crossfeed::delay_text(view),
    };
    match settable {
        true => {
            built.put("\u{2039} ", arrow, Some(Hit::CrossfeedStep(row, -1)));
            built.put(value, plain, Some(Hit::Crossfeed(row)));
            built.put(" \u{203a}", arrow, Some(Hit::CrossfeedStep(row, 1)));
        }
        false => built.put(value, dim, Some(Hit::Crossfeed(row))),
    }
    built.finish(
        focused,
        !app.session.connected || !settable && !focused,
        theme,
        width,
    )
}

/// The edit line's text: `[37] %  Enter writes · Esc cancels`, or why the last key was refused.
fn edit_text(line: &XfLine) -> String {
    let tail = match &line.refusal {
        Some(why) => why.clone(),
        None => "Enter writes \u{b7} Esc cancels".to_string(),
    };
    format!("[{}] {}  {tail}", line.text, line.row.unit())
}

/// The folded line: `Crossfeed  on · Bauer · Chu Moy · 33 % · 700 Hz · 0.26 ms  (X)`.
fn folded_row(
    app: &App,
    view: &CrossfeedView,
    theme: &Theme,
    width: usize,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let plain = ratatui::style::Style::default();
    let dim = theme.style(Role::Dim);
    let row = Hit::Crossfeed(Xf::Preset);
    let body = match (&view.unavailable, view.on) {
        (Some(why), _) => format!("unavailable: {why}"),
        (None, false) => "off".to_string(),
        (None, true) => {
            let delay = match view.delay_settable {
                true => format!("{:.2} ms", view.delay_ms),
                false => "no delay".to_string(),
            };
            format!(
                "on \u{b7} {} \u{b7} {:.0} % \u{b7} {:.0} Hz \u{b7} {delay}",
                crossfeed::words_of(view),
                view.amount_pct,
                view.crossover_hz
            )
        }
    };
    let room = width.saturating_sub(CURSOR_WIDTH + LABEL + 2 + FOLDED_KEY.len() + 2);
    let style = match view.on && view.unavailable.is_none() {
        true => plain,
        false => dim,
    };
    let mut built = Built::new();
    // two blanks after the word, as the pick draws it
    built.put(format!("{:<LABEL$} ", Xf::Preset.label()), plain, Some(row));
    built.put(
        widgets::text::cut_to_cells(&body, room.saturating_sub(1)),
        style,
        Some(row),
    );
    built.put(format!("  {FOLDED_KEY}"), dim, Some(row));
    built.finish(false, !app.session.connected, theme, width)
}

/// Draw one built row into `cell` and push the hits of its parts.
fn put_row(
    frame: &mut Frame,
    hits: &mut HitMap,
    cell: Rect,
    (line, spots): (Line<'static>, Vec<(usize, usize, Hit)>),
) {
    frame.render_widget(Paragraph::new(line), cell);
    for (at, cells, hit) in spots {
        let spot = Rect {
            x: cell.x.saturating_add(u16::try_from(at).unwrap_or(u16::MAX)),
            width: u16::try_from(cells).unwrap_or(0),
            ..cell
        }
        .intersection(cell);
        hits.push(spot, hit);
    }
}

/// Draw the section: its line (and the rows under it) into `area`, and where the layout has
/// `beside` the rest into that block of the Points table's row. Pushes the hits.
pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    beside: Option<Rect>,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let Some(view) = app.crossfeed() else {
        return;
    };
    let width = usize::from(area.width);
    let unfolded = app.xf_unfolded();
    let first = match unfolded {
        true => preset_row(app, view, theme, width),
        false => folded_row(app, view, theme, width),
    };
    // A click anywhere on the row's own cells is the row; the narrower hits sit over it.
    let line_row = Rect { height: 1, ..area }.intersection(area);
    hits.push(line_row, Hit::Crossfeed(Xf::Preset));
    put_row(frame, hits, area, first);
    if !unfolded {
        return;
    }
    match beside {
        None => {
            for (at, row) in [Xf::Level, Xf::Crossover, Xf::Delay]
                .into_iter()
                .enumerate()
            {
                let y = area.y + 1 + u16::try_from(at).unwrap_or(0);
                if y >= area.bottom() {
                    break;
                }
                let cell = Rect {
                    y,
                    height: 1,
                    ..area
                };
                hits.push(cell, Hit::Crossfeed(row));
                put_row(frame, hits, cell, value_row(app, view, row, theme, width));
            }
        }
        Some(block_area) => draw_block(frame, block_area, view, app, (theme, tier), hits),
    }
}

/// The block beside the Points table: the three value rows, then the two curves.
fn draw_block(
    frame: &mut Frame,
    area: Rect,
    view: &CrossfeedView,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let focused = app.xf_row().is_some_and(|row| row != Xf::Preset);
    let block = tier
        .block()
        .border_style(device::contour(
            app,
            theme,
            focused,
            device::border_style(app, theme),
        ))
        .title(Span::styled(
            " Crossfeed ",
            device::pane_title(theme, focused),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);
    for (at, row) in [Xf::Level, Xf::Crossover, Xf::Delay]
        .into_iter()
        .enumerate()
    {
        let y = inner.y + u16::try_from(at).unwrap_or(0);
        if y >= inner.bottom() {
            return;
        }
        let cell = Rect {
            y,
            height: 1,
            ..inner
        };
        hits.push(cell, Hit::Crossfeed(row));
        put_row(frame, hits, cell, value_row(app, view, row, theme, width));
    }
    // The picture of the two curves, from the view's own numbers, where it fits.
    let top = inner.y + VALUE_ROWS + 1;
    if inner.bottom() < top + 4 || view.curve.hz.is_empty() {
        return;
    }
    let (same, other) = (theme.style(Role::CurveSum), theme.style(Role::SelectName));
    let key = Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled("\u{2500} same ear  ", same),
        Span::styled("\u{2500} other ear", other),
    ]);
    frame.render_widget(
        Paragraph::new(key),
        Rect {
            y: top - 1,
            height: 1,
            ..inner
        },
    );
    let series = |db: &[f64]| -> Vec<(f64, f64)> {
        view.curve
            .hz
            .iter()
            .zip(db)
            .map(|(hz, db)| (hz.log10(), db.clamp(CURVE_DB[0], CURVE_DB[1])))
            .collect()
    };
    let (same_data, other_data) = (series(&view.curve.same_db), series(&view.curve.other_db));
    let datasets = vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(other)
            .data(&other_data),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(same)
            .data(&same_data),
    ];
    let axis = theme.style(Role::Axis);
    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .bounds([20f64.log10(), 20_000f64.log10()])
                .style(axis),
        )
        .y_axis(Axis::default().bounds(CURVE_DB).style(axis));
    frame.render_widget(
        chart,
        Rect {
            y: top,
            height: inner.bottom() - top,
            ..inner
        },
    );
}

#[cfg(test)]
mod tests;
