// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Space panel and the device Tone pane share of the crossfeed: the row builder, the
//! row painter and the chart of the two curves.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use crate::tui::mouse::Hit;
use crate::tui::screens::device::row_style;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH};

/// The curves' vertical range in dB.
const CURVE_DB: [f64; 2] = [-30.0, 3.0];

/// A row being put together, with the places in it a click lands on, in cells from its start.
pub(in crate::tui::screens) struct Built {
    spans: Vec<Span<'static>>,
    hits: Vec<(usize, usize, Hit)>,
    col: usize,
}

impl Built {
    pub(in crate::tui::screens) fn new() -> Built {
        Built {
            spans: Vec::new(),
            hits: Vec::new(),
            col: CURSOR_WIDTH,
        }
    }

    /// The cells of the row so far, the cursor's included.
    pub(in crate::tui::screens) fn col(&self) -> usize {
        self.col
    }

    /// Append `spans` that are `cells` wide together, a click on them being `hit`.
    pub(in crate::tui::screens) fn put_spans(
        &mut self,
        spans: Vec<Span<'static>>,
        cells: usize,
        hit: Option<Hit>,
    ) {
        if let Some(hit) = hit {
            self.hits.push((self.col, cells, hit));
        }
        self.col += cells;
        self.spans.extend(spans);
    }

    /// Append `text` in `style`, a click on it being `hit`.
    pub(in crate::tui::screens) fn put(
        &mut self,
        text: impl Into<String>,
        style: ratatui::style::Style,
        hit: Option<Hit>,
    ) {
        let text = text.into();
        let width = widgets::text::cells(&text);
        if let Some(hit) = hit {
            self.hits.push((self.col, width, hit));
        }
        self.col += width;
        self.spans.push(Span::styled(text, style));
    }

    /// The line, the cursor in front of it, and where the row stands out when `focused`.
    pub(in crate::tui::screens) fn finish(
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

impl Built {
    /// The line with the cursor in front of it where `focused`, and no box over the row: the
    /// focus is drawn by the parts that took `switch::styles`' chosen style.
    pub(in crate::tui::screens) fn finish_marked(
        self,
        focused: bool,
        stale: bool,
        theme: &Theme,
        width: usize,
    ) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
        let line = widgets::rows::with_cursor(Line::from(self.spans), focused, theme);
        (row_style(line, false, stale, theme, width), self.hits)
    }
}

/// Draw two curves over `area`: a key line first, then the chart of `same` and `other` in dB
/// against `hz`, over 20 Hz to 20 kHz.
pub(super) fn draw_curves(
    frame: &mut Frame,
    area: Rect,
    (hz, same_db, other_db): (&[f64], &[f64], &[f64]),
    theme: &Theme,
) {
    let (same, other) = (theme.style(Role::CurveSum), theme.style(Role::SelectName));
    let key = Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled("\u{2500} same ear  ", same),
        Span::styled("\u{2500} other ear", other),
    ]);
    frame.render_widget(Paragraph::new(key), Rect { height: 1, ..area });
    let series = |db: &[f64]| -> Vec<(f64, f64)> {
        hz.iter()
            .zip(db)
            .map(|(hz, db)| (hz.log10(), db.clamp(CURVE_DB[0], CURVE_DB[1])))
            .collect()
    };
    let (same_data, other_data) = (series(same_db), series(other_db));
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
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        },
    );
}
