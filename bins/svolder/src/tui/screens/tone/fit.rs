// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tone screen adds for an imported preset: the target curve, dotted under the fit;
//! the gap where the fit misses it, filled in the warning colour; and, in the wide tier, a side
//! pane naming the correction's source, its filters and the fit.
//!
//! The target is kept by [`TargetView`] at a fixed number of points, so a frame only reads it
//! at its own columns. Everything here is worked out from that and the bands the chart draws.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_profiles::tone::response::{self, BandParams};

use crate::tui::app::{App, TargetView};
use crate::tui::screens::device;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

use super::{GAIN_CEILING_DB, GAIN_FLOOR_DB, frequency_at, gain_at, row_at};

/// Columns the side pane takes in the wide tier, borders included.
pub(super) const SIDE_WIDTH: u16 = 34;
/// The least the fit may miss the target by, in dB, before the gap between them is filled.
pub(super) const GAP_DB: f64 = 2.0;
/// What the side pane is titled.
const SIDE_TITLE: &str = " Correction ";
/// Blank rows between the side pane's last filter or preamp row and the fit line.
const SIDE_GAP: usize = 1;

/// The two datasets the chart draws under the curve, as (log10 Hz, dB) points.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Overlay {
    /// The target, one point per column, drawn dotted.
    pub(super) target: Vec<(f64, f64)>,
    /// The fill between the fit and the target: one point per cell, in every column whose
    /// miss exceeds [`GAP_DB`].
    pub(super) gap: Vec<(f64, f64)>,
}

/// The target and the gap for `filters`, the bands the chart draws, over `curve`, the cells the
/// chart draws its curve in. A value off the chart's gains is clipped as the curve is: the
/// target's point is not drawn, and a gap running off an edge ends at it.
pub(super) fn overlay(view: &TargetView, filters: &[BandParams], curve: Rect) -> Overlay {
    let columns: Vec<f64> = (curve.x..curve.right())
        .map(|x| frequency_at(curve, x))
        .collect();
    let fit = response::response_db(filters, &columns);
    let mut overlay = Overlay::default();
    for (&hz, fit) in columns.iter().zip(fit) {
        let Some(target) = view.db_at(hz) else {
            continue;
        };
        let log_hz = hz.log10();
        if (GAIN_FLOOR_DB..=GAIN_CEILING_DB).contains(&target) {
            overlay.target.push((log_hz, target));
        }
        if (fit - target).abs() <= GAP_DB {
            continue;
        }
        let (low, high) = (fit.min(target), fit.max(target));
        if high < GAIN_FLOOR_DB || low > GAIN_CEILING_DB {
            continue;
        }
        let top = row_at(curve, high.min(GAIN_CEILING_DB)).unwrap_or(curve.y);
        let bottom = row_at(curve, low.max(GAIN_FLOOR_DB)).unwrap_or(curve.y);
        overlay
            .gap
            .extend((top..=bottom).map(|row| (log_hz, gain_at(curve, row))));
    }
    overlay
}

/// The quadrant glyphs the target's dotted line is drawn in.
const QUADRANTS: &str = "▖▗▘▙▚▛▜▝▞▟";
/// The glyph the chart draws the gap in, which [`restyle`] swaps for [`SHADE`].
const FILL: char = '█';
/// The lighter fill the gap shows as.
const SHADE: &str = "░";

/// Give the target's cells and the gap's, found in `curve` by their glyphs, their roles' whole
/// style, and the gap's cells their lighter fill. A chart draws a dataset in its colour alone,
/// so what a role says with an attribute — the
/// dim of the target at 16 colours and in monochrome, the gap's dim in monochrome — is put on
/// afterwards. Nothing else in the curve's cells is drawn in these glyphs.
pub(super) fn restyle(buffer: &mut Buffer, curve: Rect, theme: &Theme) {
    let target = theme.style(Role::CurveTarget);
    let gap = theme.style(Role::CurveGap);
    for y in curve.y..curve.bottom() {
        for x in curve.x..curve.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            let mut glyphs = cell.symbol().chars();
            match (glyphs.next(), glyphs.next()) {
                (Some(glyph), None) if QUADRANTS.contains(glyph) => {
                    cell.set_style(target);
                }
                (Some(FILL), None) => {
                    cell.set_symbol(SHADE).set_style(gap);
                }
                _ => {}
            }
        }
    }
}

/// The side pane in `area`: the correction's source and how many filters it has, one row per
/// filter, the preamp, and the fit line, wrapped. Filters the pane has no room for end in
/// `… N more`.
pub(super) fn draw_side(
    frame: &mut Frame,
    area: Rect,
    view: &TargetView,
    app: &App,
    theme: &Theme,
) {
    let block = Block::bordered()
        .border_style(device::contour(
            app,
            theme,
            false,
            device::border_style(app, theme),
        ))
        .title(Span::styled(SIDE_TITLE, device::pane_title(theme, false)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width).saturating_sub(2);
    let fit = widgets::text::wrap(&view.fit, width);
    // The heading, the preamp, the gap and the fit lines are always there; the filters take
    // what is left.
    let fixed = 2 + SIDE_GAP + fit.len();
    let room = usize::from(inner.height).saturating_sub(fixed);
    let shown = match view.filters.len() > room {
        true => room.saturating_sub(1),
        false => view.filters.len(),
    };
    let dim = theme.style(Role::Dim);
    let mut lines = vec![Line::from(Span::styled(
        widgets::text::cut_to_cells(&view.source, width),
        theme.style(Role::Text),
    ))];
    lines.extend(
        view.filters
            .iter()
            .take(shown)
            .map(|filter| Line::raw(widgets::text::cut_to_cells(filter, width))),
    );
    if shown < view.filters.len() {
        lines.push(Line::from(Span::styled(
            format!("… {} more", view.filters.len() - shown),
            dim,
        )));
    }
    lines.push(Line::raw(widgets::text::cut_to_cells(&view.preamp, width)));
    lines.extend(std::iter::repeat_n(Line::default(), SIDE_GAP));
    lines.extend(
        fit.into_iter()
            .map(|line| Line::from(Span::styled(line, dim))),
    );
    let text = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), text);
}

/// The Correction pane on the computer: the preset the points came from and the preamp, and that
/// nothing is fitted there.
pub(super) fn draw_computer_side(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::bordered()
        .border_style(device::contour(
            app,
            theme,
            false,
            device::border_style(app, theme),
        ))
        .title(Span::styled(SIDE_TITLE, device::pane_title(theme, false)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width).saturating_sub(2);
    let mut lines = Vec::new();
    if let Some(eq) = app.computer_eq() {
        let name = eq.preset.clone().unwrap_or_else(|| "custom".to_string());
        lines.push(Line::from(Span::styled(
            widgets::text::cut_to_cells(&name, width),
            theme.style(Role::Text),
        )));
        let preamp = super::points::preamp_text(eq.preamp_db);
        lines.push(Line::raw(widgets::text::cut_to_cells(&preamp, width)));
        lines.push(Line::default());
    }
    let dim = theme.style(Role::Dim);
    lines.extend(
        widgets::text::wrap(super::points::NOTHING_FITTED, width)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, dim))),
    );
    let text = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), text);
}

#[cfg(test)]
mod tests;
