// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's side of the tone-control screen: its curve, drawn from the points, and the
//! Points table under it, `#  Type  Freq  Gain  Q`, up to 16 rows, scrolled so the focused
//! row is always in view.
//!
//! The table keeps one height whatever the count, so adding a point never moves the curve. A
//! value outside the chart's gains sits at its edge: it plays as written.

use ratatui::layout::Rect;
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::response::{self, BandParams};

use crate::tui::app::App;
use crate::tui::boxes::edit_line::{gain_text, q_text};
use crate::tui::theme::Role;
use crate::tui::widgets::MISSING;

use super::{GAIN_CEILING_DB, GAIN_FLOOR_DB, Graph, Plot, ResponseSeries};
use svolder_profiles::convert::frequency_text;

/// The table's column titles.
pub(super) const POINT_TITLES: &str = "   #  Type  Freq        Gain       Q";

/// The first point the table shows, so that the focused one is in view.
pub(crate) fn scroll_start(len: usize, visible: usize, focused: usize) -> usize {
    if len <= visible {
        return 0;
    }
    focused.saturating_sub(visible / 2).min(len - visible)
}

/// The graph's kind for the computer: muted while the EQ is off.
pub(super) fn graph(app: &App) -> Graph {
    let on = app.computer_eq().is_some_and(|eq| eq.on);
    match (on, app.computer_points().iter().any(|p| p.g != 0.0)) {
        (false, _) => Graph::FlatMode,
        (true, true) => Graph::Curve,
        (true, false) => Graph::FlatCurve,
    }
}

/// The type a view's word names; a word nobody sends reads as a peak.
fn kind_of(word: &str) -> FilterKind {
    svolder_host::eq::parse_kind(word).unwrap_or(FilterKind::Peaking)
}

/// The points as the filters the curve is summed from.
fn correction(app: &App) -> Correction {
    Correction {
        preamp_db: 0.0,
        filters: app
            .computer_points()
            .iter()
            .map(|p| SourceFilter {
                kind: kind_of(&p.kind),
                f: p.f,
                g: p.g,
                q: p.q,
            })
            .collect(),
    }
}

/// Where each point's centre is marked: its point index and the (log10 Hz, dB) it sits at,
/// held to the chart's edges.
pub(super) fn centres(app: &App) -> Vec<(usize, [(f64, f64); 1])> {
    app.computer_points()
        .iter()
        .enumerate()
        .map(|(at, p)| {
            (
                at,
                [(
                    p.f.clamp(super::F_MIN, super::F_MAX).log10(),
                    p.g.clamp(GAIN_FLOOR_DB, GAIN_CEILING_DB),
                )],
            )
        })
        .collect()
}

/// What the Response block draws on the computer side.
pub(super) fn series(app: &App, curve: Rect) -> ResponseSeries {
    let graph = graph(app);
    let freqs = response::log_freqs(2 * usize::from(curve.width));
    let summed = freqs
        .iter()
        .zip(response::correction_db(&correction(app), &freqs))
        .map(|(hz, db)| (hz.log10(), db))
        .collect();
    let muted = matches!(graph, Graph::FlatCurve | Graph::FlatMode);
    ResponseSeries {
        graph,
        muted,
        line: match muted {
            true => Role::Dim,
            false => Role::CurveSum,
        },
        summed,
        plot: Plot {
            filters: Vec::<BandParams>::new(),
            centres: centres(app),
        },
        overlay: None,
    }
}

/// Where a point's type, frequency, gain and Q sit in its row's text, each as a first cell and a
/// width, for the clicks on them.
pub(super) const FIELDS: [(usize, usize); 4] = [(4, 4), (10, 8), (20, 9), (33, 4)];

/// One point's row.
pub(super) fn row_text(app: &App, at: usize) -> String {
    let points = app.computer_points();
    let Some(p) = points.get(at) else {
        return MISSING.to_string();
    };
    format!(
        "{:>2}  {:<4}  {:>5} Hz  {:>6} dB    {:>4}",
        at + 1,
        p.kind,
        frequency_text(p.f),
        gain_text(p.g),
        q_text(p.q),
    )
}

/// The edit line in place of a point's row: `1  f [1000] Hz  g +3.0 dB  q 1.4`, the focused field
/// in brackets, and its keys or what it refused after.
pub(super) fn line_text(line: &crate::tui::app::PointLine) -> String {
    let mut text = format!("{:>2}  ", line.point + 1);
    for (at, ((name, unit), field)) in crate::tui::app::point_line_names()
        .iter()
        .zip(&line.fields)
        .enumerate()
    {
        let shown = match at == line.focus {
            true => format!("[{field}]"),
            false => format!(" {field} "),
        };
        text.push_str(&format!("{name}{shown}{unit}  "));
    }
    match &line.refusal {
        Some(why) => text.push_str(why),
        None => text.push_str("Tab moves · Enter writes · Esc cancels"),
    }
    text
}

/// A preamp in dB as the bottom border writes it: one decimal, a true minus, never `−0.0`.
pub(super) fn preamp_text(db: f64) -> String {
    let rounded = (db * 10.0).round() / 10.0 + 0.0;
    format!("preamp {rounded:.1} dB").replace('-', "\u{2212}")
}

/// What the Correction pane says on the computer: nothing is fitted there.
pub(super) const NOTHING_FITTED: &str =
    "on the computer nothing is fitted: every filter plays as written";
