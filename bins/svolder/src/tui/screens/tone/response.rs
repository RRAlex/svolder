// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's Response block: the chart, its axes and labels, and the curve's points.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph};
use svolder_profiles::tone::response::{self, BandParams, F_MAX, F_MIN};
use svolder_profiles::{EqParam, EqSpec};

use crate::tui::app::curve_drag;
use crate::tui::app::{App, Screen, TargetView};
use crate::tui::keys::KeyContext;
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::{self, MISSING};

use super::device;
use super::{
    GAIN_BLANK, GAIN_CEILING_DB, GAIN_FLOOR_DB, GAIN_LABEL_WIDTH, GAIN_LABELS, Graph, fit,
};

/// The words in the Response chart's top border.
const RESPONSE_ACTIONS: [Action; 3] = [
    Action {
        code: KeyCode::Char('f'),
        word: "flat",
    },
    Action {
        code: KeyCode::Char('p'),
        word: "presets",
    },
    Action {
        code: KeyCode::Char('r'),
        word: "refresh",
    },
];

/// The frequencies the x axis names, and what it calls them.
const FREQUENCY_LABELS: [(f64, &str); 9] = [
    (20.0, "20"),
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1k"),
    (2000.0, "2k"),
    (5000.0, "5k"),
    (20000.0, "20k"),
];

/// What the graph says in Loudness mode while the DAC plays at its full volume, where the
/// device's loudness contour adds nothing.
const LOUDNESS_AT_FULL: &str = "no effect at full volume";
/// What it says at any other volume, before the strength the contour is set to.
const LOUDNESS_ACTIVE: &str = "loudness active — the boost grows as the volume goes down";

/// The Response block's border and the padding inside it: its labels and its curve open and
/// close on no blank of their own, so the wide tier keeps two columns clear of them.
pub(super) fn response_block(tier: Tier) -> Block<'static> {
    Block::bordered().padding(tier.bare_padding())
}

/// Push the hits of the Response block's title row: the title leads back to the device screen,
/// the words in the border do what their keys do. Answers with those words.
fn push_response_title(area: Rect, app: &App, theme: &Theme, hits: &mut HitMap) -> Line<'static> {
    // The keys are never in the chart, so its frame is the one a palette with an emphasis
    // colour draws faint and its title the unfocused one.
    let (words, word_hits) = widgets::actions::actions_title(
        &RESPONSE_ACTIONS,
        None,
        widgets::actions::action_room(area.width, widgets::text::cells(" Response ")),
        Screen::Tone,
        &KeyContext::of(app),
        theme,
    );
    let title = Rect {
        height: 1.min(area.height),
        ..area
    };
    hits.push(title, Hit::Tab(Screen::Device));
    widgets::actions::push_action_hits(hits, area, &words, &word_hits);
    words
}

/// What the Response block draws its datasets from: the sampled curve, the overlay of an imported
/// preset's target, and the bands' points, with how the graph is to be read.
pub(super) struct ResponseSeries {
    /// Which of the graphs this is.
    pub(super) graph: Graph,
    /// Whether the line and the points are drawn muted.
    pub(super) muted: bool,
    /// The role the summed line is drawn in.
    pub(super) line: Role,
    /// The summed curve as (log10 Hz, dB) points, none in Loudness mode.
    pub(super) summed: Vec<(f64, f64)>,
    /// The bands the curve is drawn from, and the point each one's centre is marked at.
    pub(super) plot: Plot,
    /// The target and the gap under the fit, when a preset's target is on show.
    pub(super) overlay: Option<fit::Overlay>,
}

/// Sample the curve the Response block draws over `curve`, and gather the overlay and the points.
fn response_series(
    app: &App,
    eq: &EqSpec,
    target: Option<&TargetView>,
    curve: Rect,
) -> ResponseSeries {
    // Braille draws two dots across a cell, so the curve is sampled twice per column.
    if app.on_computer() {
        return super::points::series(app, curve);
    }
    let freqs = response::log_freqs(2 * usize::from(curve.width));
    let plot = plot(app, eq);
    let graph = Graph::of(app, eq);
    let summed: Vec<(f64, f64)> = match graph {
        // Flat mode draws the stored curve too, muted: what the bands would shape if the
        // bypass were not standing between them and what plays, so the line the bands' own
        // points sit on is never a flat one they are visibly off — see `Graph::FlatMode`.
        Graph::Curve | Graph::FlatCurve | Graph::FlatMode => freqs
            .iter()
            .zip(response::response_db(&plot.filters, &freqs))
            .map(|(hz, db)| (hz.log10(), db))
            .collect(),
        Graph::Loudness => Vec::new(),
    };
    // A muted graph draws its line and every point in the dim role here, the focused band's
    // too: the base coat a theme with no spectrum to draw in keeps, and the one the spectrum
    // (see `curve_of` and `curve_muted`) recolours desaturated over, at true colour and 256.
    let muted = matches!(graph, Graph::FlatCurve | Graph::FlatMode);
    let line = match muted {
        true => Role::Dim,
        false => Role::CurveSum,
    };

    // An imported preset's target is dotted under the fit, with the gap between them filled under
    // both. A muted curve is not what the device applies, so it has no gap to show against the
    // target. Neither is
    // drawn in Braille or Dot cells, the two the spectrum recolours.
    let overlay = target
        .filter(|_| graph != Graph::Loudness)
        .map(|view| fit::overlay(view, &plot.filters, curve));
    ResponseSeries {
        graph,
        muted,
        line,
        summed,
        plot,
        overlay,
    }
}

/// The Response block: the summed curve with a point per band — what the device applies in EQ
/// mode, or, in Flat mode, the stored curve it is not applying — or, in Loudness mode, a line
/// of text in the curve's place — and the axes they are read against. Its title is this
/// screen's own tab header, leading back to the device screen.
pub(super) fn draw_response(
    frame: &mut Frame,
    area: Rect,
    curve: Rect,
    app: &App,
    eq: &EqSpec,
    (theme, tier, target): (&Theme, Tier, Option<&TargetView>),
    hits: &mut HitMap,
) {
    let words = push_response_title(area, app, theme, hits);
    let block = response_block(tier)
        .border_style(device::contour(
            app,
            theme,
            false,
            device::border_style(app, theme),
        ))
        .title(Span::styled(" Response ", device::pane_title(theme, false)))
        .title(words.right_aligned());
    let inner = block.inner(area);
    let ResponseSeries {
        graph,
        muted,
        line,
        summed,
        plot,
        overlay,
    } = response_series(app, eq, target, curve);
    // The mouse reaches the curve wherever one is drawn: a press grabs a band's point there.
    if graph != Graph::Loudness {
        hits.push(curve, Hit::Curve);
    }
    let mut datasets = Vec::new();
    if let Some(overlay) = overlay.as_ref() {
        if !muted {
            datasets.push(
                Dataset::default()
                    .marker(Marker::Block)
                    .graph_type(GraphType::Scatter)
                    .style(theme.style(Role::CurveGap))
                    .data(&overlay.gap),
            );
        }
        datasets.push(
            Dataset::default()
                .marker(Marker::Quadrant)
                .graph_type(GraphType::Scatter)
                .style(theme.style(Role::CurveTarget))
                .data(&overlay.target),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.style(line))
            .data(&summed),
    );
    if graph != Graph::Loudness {
        datasets.extend(plot.centres.iter().map(|(band, point)| {
            let role = match (muted, *band == app.tone.band) {
                (true, _) => Role::Dim,
                (false, true) => Role::CurveSelected,
                (false, false) => Role::CurvePoint,
            };
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(theme.style(role))
                .data(point)
        }));
    }

    let axis = theme.style(Role::Axis);
    let chart = Chart::new(datasets)
        .block(block)
        // ratatui spreads an axis's labels evenly, which the frequencies of a log axis are
        // not, so one empty label keeps the row it draws them on and [`label_row`] fills it.
        .x_axis(
            Axis::default()
                .bounds([F_MIN.log10(), F_MAX.log10()])
                .labels([Line::raw("")])
                .style(axis),
        )
        // It spreads the gains' labels evenly too, rounding them onto rows its own way rather
        // than where its canvas draws a gain, so blank labels as wide as theirs keep their
        // column and [`gain_labels`] writes each on the row its gain is drawn in.
        .y_axis(
            Axis::default()
                .bounds([GAIN_FLOOR_DB, GAIN_CEILING_DB])
                .labels([GAIN_BLANK; 2])
                .style(axis),
        );
    frame.render_widget(chart, area);
    gain_labels(frame, inner, curve);
    if overlay.is_some() {
        fit::restyle(frame.buffer_mut(), curve, theme);
    }
    // A chart draws its dots in a colour alone, so a muted line and its points take the rest of
    // the dim role — the dim attribute at 16 colours — once they are drawn; the spectrum then
    // recolours the Braille and Dot cells desaturated wherever it has one to draw in. Nothing
    // else is drawn in the cells the curve goes in.
    if muted {
        frame.buffer_mut().set_style(curve, theme.style(Role::Dim));
    }
    frame.render_widget(
        Paragraph::new(Line::raw(label_row(inner, curve))),
        Rect {
            y: inner.bottom().saturating_sub(1),
            height: 1.min(inner.height),
            ..inner
        },
    );
    if graph == Graph::Loudness {
        let text = widgets::text::cut_to_cells(&loudness_text(app, eq), usize::from(curve.width));
        frame.render_widget(
            Paragraph::new(Line::styled(text, theme.style(Role::Text)).centered()),
            Rect {
                y: zero_row(curve),
                height: 1.min(curve.height),
                ..curve
            },
        );
    }
}

/// Where each band's point is drawn in `curve`: the band, 0-based, and the cell its centre lands
/// in — its column along the log frequency axis and its row down the gain axis — found the way
/// the chart places it ([`column_at`], [`row_at`]). A band the chart draws no point for is left
/// out: one the device has not reported in full, one whose gain is off the chart, and every band
/// in Loudness mode, which draws no curve.
pub(crate) fn points(app: &App, curve: Rect) -> Vec<(usize, (u16, u16))> {
    let Some(eq) = app.profile.eq.as_ref() else {
        return Vec::new();
    };
    if Graph::of(app, eq) == Graph::Loudness {
        return Vec::new();
    }
    let centres = match app.on_computer() {
        true => super::points::centres(app),
        false => plot(app, eq).centres,
    };
    centres
        .iter()
        .filter_map(|&(band, [(log_hz, db)])| {
            Some((band, (column_at(curve, log_hz)?, row_at(curve, db)?)))
        })
        .collect()
}

/// The band whose centre is nearest column `x` of `curve` in log frequency, among the bands the
/// curve is drawn from — the lower band where two are as near — or `None` where none is drawn.
pub(crate) fn nearest_centre(app: &App, curve: Rect, x: u16) -> Option<usize> {
    let eq = app.profile.eq.as_ref()?;
    if Graph::of(app, eq) == Graph::Loudness {
        return None;
    }
    let x = f64::from(x.saturating_sub(curve.x));
    let mut nearest: Option<(usize, f64)> = None;
    let centres = match app.on_computer() {
        true => super::points::centres(app),
        false => plot(app, eq).centres,
    };
    for &(band, [(log_hz, _)]) in &centres {
        let off = (along(curve, log_hz) - x).abs();
        if nearest.is_none_or(|(_, best)| off < best) {
            nearest = Some((band, off));
        }
    }
    nearest.map(|(band, _)| band)
}

/// How many columns into `curve` the chart puts a frequency whose base-10 logarithm is
/// `log_hz`, before it rounds to a cell: the log axis from [`F_MIN`] to [`F_MAX`] spread over the
/// curve's first to its last column, worked out as ratatui's own canvas works it out so a point
/// is found exactly where the chart drew it.
pub(super) fn along(curve: Rect, log_hz: f64) -> f64 {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    (log_hz - left) * (f64::from(curve.width) - 1.0) / (right - left)
}

/// The column of `curve` the chart draws a point at a frequency whose base-10 logarithm is
/// `log_hz` in, or `None` off the log axis, where it draws none.
pub(super) fn column_at(curve: Rect, log_hz: f64) -> Option<u16> {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    if curve.width == 0 || !(left..=right).contains(&log_hz) {
        return None;
    }
    Some(curve.x + along(curve, log_hz).round() as u16)
}

/// The frequency, in Hz, column `x` of `curve` stands for on the log axis: the inverse of where
/// the chart draws one ([`column_at`]), so the frequency found for a cell is drawn back in that
/// very cell. A column off either end of the curve is taken as that end.
pub(crate) fn frequency_at(curve: Rect, x: u16) -> f64 {
    let (left, right) = (F_MIN.log10(), F_MAX.log10());
    let last = f64::from(curve.width.saturating_sub(1));
    if last == 0.0 {
        return F_MIN;
    }
    let along = f64::from(x.clamp(curve.x, curve.right() - 1) - curve.x);
    10f64.powf(left + along * (right - left) / last)
}

/// The gain, in dB, row `y` of `curve` stands for: the inverse of where the chart draws one
/// ([`row_at`]), so the gain found for a cell is drawn back in that very cell. A row above or
/// below the curve is taken as its top or its bottom, the chart's own highest and lowest gains.
pub(crate) fn gain_at(curve: Rect, y: u16) -> f64 {
    let last = f64::from(curve.height.saturating_sub(1));
    if last == 0.0 {
        return GAIN_CEILING_DB;
    }
    let down = f64::from(y.clamp(curve.y, curve.bottom() - 1) - curve.y);
    GAIN_CEILING_DB - down * (GAIN_CEILING_DB - GAIN_FLOOR_DB) / last
}

/// The row of `curve` the chart draws a point at `db` in, from the top of its cells, the way
/// ratatui's canvas rounds it, or `None` off the gains the chart spans, where it draws none.
pub(super) fn row_at(curve: Rect, db: f64) -> Option<u16> {
    if curve.height == 0 || !(GAIN_FLOOR_DB..=GAIN_CEILING_DB).contains(&db) {
        return None;
    }
    let down = (GAIN_CEILING_DB - db) * (f64::from(curve.height) - 1.0)
        / (GAIN_CEILING_DB - GAIN_FLOOR_DB);
    Some(curve.y + down.round() as u16)
}

/// Whether row `y` of `curve` is the one a flat curve is drawn on ([`zero_row`]), a row above or
/// below the curve taken as its top or its bottom as [`gain_at`] takes it.
pub(crate) fn on_zero_row(curve: Rect, y: u16) -> bool {
    curve.height > 0 && y.clamp(curve.y, curve.bottom() - 1) == zero_row(curve)
}

/// The row of `curve` a flat curve is drawn on: where 0 dB falls between the gains the chart
/// spans, from the top of its cells.
pub(super) fn zero_row(curve: Rect) -> u16 {
    row_at(curve, 0.0).unwrap_or(curve.y)
}

/// Write the y axis's labels into the column the chart keeps for them inside `inner`, left of
/// `curve`: each on the row its gain is drawn in ([`row_at`]), so the `0` sits on the row a flat
/// curve is drawn on at every size, as the frequencies sit on the columns theirs are.
fn gain_labels(frame: &mut Frame, inner: Rect, curve: Rect) {
    for (db, text) in GAIN_LABELS {
        let Some(y) = row_at(curve, db) else {
            continue;
        };
        let cells = Rect {
            x: inner.x,
            y,
            width: GAIN_LABEL_WIDTH,
            height: 1,
        }
        .intersection(inner);
        frame.render_widget(Paragraph::new(Line::raw(text)), cells);
    }
}

/// What the graph says in Loudness mode, from the DAC's own volume, since the loudness contour
/// grows as that goes down: [`LOUDNESS_AT_FULL`] at its top step, and anywhere else — and
/// while the volume is unread — [`LOUDNESS_ACTIVE`] with the strength the contour is set to.
pub(super) fn loudness_text(app: &App, eq: &EqSpec) -> String {
    let full = app.volume().is_some_and(|volume| {
        matches!(
            (app.raw_of(volume.id), volume.range),
            (Some(raw), Some((_, top))) if raw >= top
        )
    });
    if full {
        return LOUDNESS_AT_FULL.to_string();
    }
    let strength = app
        .raw_of(eq.loudness_id())
        .map_or_else(|| MISSING.to_string(), |raw| raw.to_string());
    format!("{LOUDNESS_ACTIVE} · strength {strength}")
}

/// What the chart is drawn from.
pub(super) struct Plot {
    /// The filter every band with a full set of readings describes.
    pub(super) filters: Vec<BandParams>,
    /// Those bands again, each with the one point its centre is marked at.
    pub(super) centres: Vec<(usize, [(f64, f64); 1])>,
}

/// The bands the curve is drawn from: the peaking filter each one describes, and the point
/// its centre sits at with the band it belongs to.
///
/// Each band is drawn at the steps it is on its way to ([`curve_drag::drawn_raw`]): a step a band
/// key, a drag or a recall has asked for, before the device has confirmed it, so the point
/// follows the keys and the pointer as they move it. A band that has neither a reading nor a
/// write asked for of its frequency, its gain and its Q is in neither: nothing here guesses at a
/// value the device has not sent.
pub(super) fn plot(app: &App, eq: &EqSpec) -> Plot {
    let mut plot = Plot {
        filters: Vec::new(),
        centres: Vec::new(),
    };
    for band in 0..eq.bands.len() {
        let raw = |param| {
            eq.band_id(band, param)
                .and_then(|id| curve_drag::drawn_raw(app, id))
                .and_then(|raw| u8::try_from(raw).ok())
        };
        let f0 = raw(EqParam::Frequency).and_then(|raw| eq.frequency(band, raw));
        let gain_db = raw(EqParam::Gain).map(|raw| eq.gain_db(raw));
        let q = raw(EqParam::Q).map(|raw| eq.q(raw));
        let (Some(f0), Some(gain_db), Some(q)) = (f0, gain_db, q) else {
            continue;
        };
        plot.filters.push(BandParams { f0, gain_db, q });
        plot.centres.push((band, [(f0.log10(), gain_db)]));
    }
    plot
}

/// The x axis's labels, each centred on the column its frequency is drawn in, written into a
/// row as wide as the block's inside. A label that would run into the one before it is left
/// out rather than drawn somewhere it does not mean.
fn label_row(inner: Rect, curve: Rect) -> String {
    let mut row = vec![' '; usize::from(inner.width)];
    let decades = (F_MAX / F_MIN).log10();
    let span = f64::from(curve.width.saturating_sub(1));
    let left = usize::from(curve.x.saturating_sub(inner.x));
    let mut free = 0;
    for (hz, text) in FREQUENCY_LABELS {
        let width = widgets::text::cells(text);
        let Some(last) = row.len().checked_sub(width) else {
            continue;
        };
        let centre = left + ((hz / F_MIN).log10() / decades * span).round() as usize;
        let start = centre.saturating_sub(width / 2).min(last);
        if start < free {
            continue;
        }
        for (n, letter) in text.chars().enumerate() {
            row[start + n] = letter;
        }
        free = start + width + 1;
    }
    row.into_iter().collect()
}
