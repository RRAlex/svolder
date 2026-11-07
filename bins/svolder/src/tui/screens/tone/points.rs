// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's side of the tone-control screen: its curve, drawn from the points, and the
//! Points table under it, `#  Type  Freq  Gain  Q`, up to 16 rows, scrolled so the focused
//! row is always in view.
//!
//! The table keeps one height whatever the count, so adding a point never moves the curve. A
//! value outside the chart's gains sits at its edge: it plays as written.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::response::{self, BandParams};

use crate::tui::app::{App, Screen, ToneRow};
use crate::tui::boxes::edit_line::{gain_text, q_text};
use crate::tui::keys::Scene;
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::actions::Action;
use crate::tui::widgets::{self, MISSING};

use super::{GAIN_CEILING_DB, GAIN_FLOOR_DB, Graph, Plot, ResponseSeries};
use crate::tui::screens::device::{self, row_style};
use svolder_profiles::convert::frequency_text;

/// Rows of points the table shows at once; more scroll.
pub(super) const POINT_ROWS: usize = 8;

/// The words in the Points block's top border.
const POINT_ACTIONS: [Action; 3] = [
    Action {
        code: KeyCode::Char('a'),
        word: "a add",
    },
    Action {
        code: KeyCode::Char('x'),
        word: "x remove",
    },
    Action {
        code: KeyCode::Char('0'),
        word: "0 flatten",
    },
];

/// The table's column titles.
const POINT_TITLES: &str = "   #  Type  Freq        Gain       Q";

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

fn kind_of(word: &str) -> FilterKind {
    match word {
        "LSC" => FilterKind::LowShelf,
        "HSC" => FilterKind::HighShelf,
        _ => FilterKind::Peaking,
    }
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
const FIELDS: [(usize, usize); 4] = [(4, 4), (10, 8), (20, 9), (33, 4)];

/// One point's row.
fn row_text(app: &App, at: usize) -> String {
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
fn line_text(line: &crate::tui::app::PointLine) -> String {
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

/// The Points block: a row per point, the focused one marked, scrolled to keep it in view, with
/// the count in its title and the preamp on its bottom border.
pub(super) fn draw_points(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let focused = matches!(app.tone.row, ToneRow::Point(_));
    let points = app.computer_points();
    let max = app.computer_eq().map_or(16, |eq| usize::from(eq.max));
    let mark = format!("{} of {max}", points.len());
    let (words, word_hits) = widgets::actions::actions_title(
        &POINT_ACTIONS,
        Some(&mark),
        widgets::actions::action_room(area.width, widgets::text::cells(" Points · computer ")),
        Screen::Tone,
        &Scene::of(app),
        theme,
    );
    let mut block = tier
        .block()
        .border_style(device::contour(
            app,
            theme,
            focused,
            device::border_style(app, theme),
        ))
        .title(Span::styled(
            " Points · computer ",
            device::pane_title(theme, focused),
        ))
        .title(words.clone().right_aligned());
    let inner = block.inner(area);
    let visible = usize::from(inner.height).saturating_sub(1);
    let start = scroll_start(points.len(), visible, app.focused_point());
    let dim = theme.style(Role::Text).patch(theme.style(Role::Dim));
    let mut notes = Vec::new();
    if let Some(eq) = app.computer_eq() {
        notes.push(preamp_text(eq.preamp_db));
    }
    if start + visible < points.len() {
        notes.push("↓ more".to_string());
    }
    if start > 0 {
        notes.push("↑ more".to_string());
    }
    if !notes.is_empty() {
        block = block.title_bottom(Span::styled(format!(" {} ", notes.join(" · ")), dim));
    }
    let mut lines = vec![Line::from(Span::styled(
        POINT_TITLES,
        theme.style(Role::Dim),
    ))];
    let on = app.computer_eq().is_some_and(|eq| eq.on);
    for at in start..points.len().min(start + visible) {
        let selected = at == app.focused_point() && focused;
        let text = match app.tone.point_line.as_ref().filter(|line| line.point == at) {
            Some(line) => line_text(line),
            None => row_text(app, at),
        };
        let line = widgets::rows::with_cursor(Line::raw(text), selected, theme);
        lines.push(row_style(
            line,
            selected,
            !on && !selected,
            theme,
            usize::from(inner.width),
        ));
        let row = Rect {
            y: inner.y + u16::try_from(lines.len() - 1).unwrap_or(0),
            height: 1,
            ..inner
        }
        .intersection(inner);
        hits.push(row, Hit::Point(at));
        for (field, (start, width)) in FIELDS.into_iter().enumerate() {
            let cells = Rect {
                x: row.x.saturating_add(
                    u16::try_from(widgets::CURSOR_WIDTH + start).unwrap_or(u16::MAX),
                ),
                width: u16::try_from(width).unwrap_or(0),
                ..row
            }
            .intersection(row);
            hits.push(cells, Hit::PointField(at, field));
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    widgets::actions::push_action_hits(hits, area, &words, &word_hits);
}

/// What the Correction pane says on the computer: nothing is fitted there.
pub(super) const NOTHING_FITTED: &str =
    "on the computer nothing is fitted: every filter plays as written";
