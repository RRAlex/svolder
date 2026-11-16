// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers' part of the tone screen's Space section, unfolded.
//!
//! While the keys are in the section and the virtual speakers are the choice, the section is a
//! block of its own as wide as the screen, between the graph and the Points table: the Space row
//! (the choice), the Set row (the set with its choice and term, `S` opens the list), then facts
//! read off the view: the licence line, where the speakers were measured, how long the responses
//! are, how much quieter than Off they play and where the set comes from. From 120 columns the
//! two curves (what the same ear and the other ear hear of a speaker) stand beside the facts;
//! below it they are two strips of level blocks under them. The facts are never typed into: the
//! keys' two stops are the choice and the set.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use svolder_host::eq::SpeakersView;
use svolder_host::eq::speakers::SpeakersCurve;

use crate::tui::app::speakers as words;
use crate::tui::app::{App, ToneRow, Xf};
use crate::tui::layout_tier::Tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens::device;
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::crossfeed::{Built, LABEL, draw_curves, put_row, space_row};

/// A built row, the places in it a click lands on, and what a click anywhere on it is.
type Row = (Line<'static>, Vec<(usize, usize, Hit)>, Option<Hit>);

/// Rows of the block in the narrow tiers: the borders, the choice, the set, the licence, the
/// angles with the length, the level and the two strips.
const NARROW_HEIGHT: u16 = 8;
/// Rows of the block from 120 columns: the borders, the choice, the set, the licence, the angles,
/// the length, the level and the source.
const WIDE_HEIGHT: u16 = 9;
/// Columns of the facts beside the curves in the wide tier.
const FACTS_WIDTH: u16 = 74;
/// The level blocks a strip is drawn with, lowest first.
const BLOCKS: [char; 8] = [
    '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}',
];
/// The strips' range in dB.
const STRIP_DB: [f64; 2] = [-30.0, 3.0];
/// The most columns a strip takes.
const STRIP_MAX: usize = 24;
/// What the Set row ends on: the key that opens the list.
const SETS_KEY: &str = "S sets\u{2026}";

/// Rows the unfolded block takes.
pub(super) fn height(wide: bool) -> u16 {
    match wide {
        true => WIDE_HEIGHT,
        false => NARROW_HEIGHT,
    }
}

/// What the same ear and the other ear hear, in dB over the view's grid: the left speaker to the
/// left ear and the right to the right ear averaged, and the two crossings averaged.
fn same_other(curve: &SpeakersCurve) -> (Vec<f64>, Vec<f64>) {
    let mean = |a: &[f64], b: &[f64]| -> Vec<f64> {
        a.iter().zip(b).map(|(a, b)| (a + b) / 2.0).collect()
    };
    (
        mean(&curve.ll_db, &curve.rr_db),
        mean(&curve.lr_db, &curve.rl_db),
    )
}

/// `db` as a strip of `cols` level blocks, one per column over the grid.
fn strip(db: &[f64], cols: usize) -> String {
    if db.is_empty() || cols == 0 {
        return String::new();
    }
    (0..cols)
        .map(|col| {
            let at = match cols {
                1 => 0,
                _ => col * (db.len() - 1) / (cols - 1),
            };
            let level = (db[at].clamp(STRIP_DB[0], STRIP_DB[1]) - STRIP_DB[0])
                / (STRIP_DB[1] - STRIP_DB[0]);
            BLOCKS[((level * (BLOCKS.len() - 1) as f64).round() as usize).min(BLOCKS.len() - 1)]
        })
        .collect()
}

/// The angles in the view's sentence with the degree sign: `left 30.0° · right 330.0° · 1.5 m`.
fn angles_text(view: &SpeakersView) -> String {
    view.angles
        .replace(" deg", "\u{b0}")
        .replace(", ", " \u{b7} ")
}

/// A source address without its scheme.
fn source_text(view: &SpeakersView) -> String {
    view.source_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

/// One fact row: `Licence   Apache-2.0 · credit`.
fn fact(
    label: &str,
    text: &str,
    theme: &Theme,
    width: usize,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let mut built = Built::new();
    built.put(format!("{label:<LABEL$}"), theme.style(Role::Dim), None);
    built.put(
        widgets::text::cut_to_cells(text, width.saturating_sub(CURSOR_WIDTH + LABEL + 1)),
        ratatui::style::Style::default(),
        None,
    );
    built.finish(false, false, theme, width)
}

/// The Set row: `Set  ‹ No room · HRTF   SADIE II, D2 ›   S sets…`.
fn set_row(
    app: &App,
    view: &SpeakersView,
    theme: &Theme,
    width: usize,
) -> (Line<'static>, Vec<(usize, usize, Hit)>) {
    let plain = ratatui::style::Style::default();
    let (dim, arrow) = (theme.style(Role::Dim), theme.style(Role::ModeArrow));
    let row = Hit::Crossfeed(Xf::Set);
    let mut built = Built::new();
    built.put(format!("{:<LABEL$}", Xf::Set.label()), plain, Some(row));
    built.put("\u{2039} ", arrow, Some(Hit::CrossfeedStep(Xf::Set, -1)));
    let name = match (&view.room_name, &view.term) {
        (Some(room), Some(term)) => format!("{room} \u{b7} {term}   {}", view.label),
        _ => "none chosen".to_string(),
    };
    let room =
        width.saturating_sub(CURSOR_WIDTH + LABEL + 2 + 2 + widgets::text::cells(SETS_KEY) + 3);
    built.put(widgets::text::cut_to_cells(&name, room), plain, Some(row));
    built.put(" \u{203a}", arrow, Some(Hit::CrossfeedStep(Xf::Set, 1)));
    built.put("   ", plain, Some(row));
    built.put(
        SETS_KEY,
        dim,
        Some(Hit::Key(crossterm::event::KeyCode::Char('S'))),
    );
    let focused = app.tone.row == ToneRow::Crossfeed(Xf::Set);
    built.finish(focused, !app.session.connected, theme, width)
}

/// The two strips under the facts: `Same ear  ▇▇▆▅…   Other ear  ▃▃▃▄…`.
fn strips_row(view: &SpeakersView, theme: &Theme, width: usize) -> Line<'static> {
    let (same, other) = same_other(&view.curve);
    let labels = "Same ear  ".len() + "   Other ear  ".len();
    let cols = (width.saturating_sub(CURSOR_WIDTH + labels) / 2).min(STRIP_MAX);
    Line::from(vec![
        Span::raw(" ".repeat(CURSOR_WIDTH)),
        Span::styled("Same ear  ", theme.style(Role::Dim)),
        Span::styled(strip(&same, cols), theme.style(Role::CurveSum)),
        Span::styled("   Other ear  ", theme.style(Role::Dim)),
        Span::styled(strip(&other, cols), theme.style(Role::SelectName)),
    ])
}

/// Draw the block into `area`, which is [`height`] rows tall, and push the hits of its rows.
pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    (theme, tier): (&Theme, Tier),
    hits: &mut HitMap,
) {
    let Some(view) = app.speakers() else {
        return;
    };
    // `,` and `.` step what the row the keys are on holds: the set on the Set row, else the choice.
    let steps = match app.tone.row == ToneRow::Crossfeed(Xf::Set) {
        true => "set",
        false => "choice",
    };
    let keys = Line::from(Span::styled(
        format!(" \u{2423} on/off \u{b7} , . {steps} \u{b7} S sets "),
        theme.style(Role::Dim),
    ))
    .right_aligned();
    let block = tier
        .block()
        .border_style(device::contour(
            app,
            theme,
            true,
            device::border_style(app, theme),
        ))
        .title(Span::styled(" Space ", device::pane_title(theme, true)))
        .title(keys);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Where the facts go: the whole width, or the left of it with the curves beside.
    let facts_width = match tier.wide {
        true => inner.width.min(FACTS_WIDTH),
        false => inner.width,
    };
    let facts = Rect {
        width: facts_width,
        ..inner
    };
    let width = usize::from(facts.width);
    let mut rows: Vec<Row> = vec![
        {
            let (line, spots) = space_row(app, theme, width);
            (line, spots, Some(Hit::Crossfeed(Xf::Space)))
        },
        {
            let (line, spots) = set_row(app, view, theme, width);
            (line, spots, Some(Hit::Crossfeed(Xf::Set)))
        },
    ];
    let mut facts_rows = vec![("Licence", words::licence_text(view))];
    // A set whose source said nothing of its angles (an own WAV) has no Angles row.
    let angles = angles_text(view);
    match (tier.wide, angles.is_empty()) {
        (_, true) => facts_rows.push(("Length", words::length_text(view))),
        (true, false) => {
            facts_rows.push(("Angles", angles));
            facts_rows.push(("Length", words::length_text(view)));
        }
        (false, false) => facts_rows.push((
            "Angles",
            format!("{angles} \u{b7} {}", words::length_text(view)),
        )),
    }
    facts_rows.push(("Level", words::level_text(view)));
    if tier.wide {
        facts_rows.push(("Source", source_text(view)));
    }
    for (label, text) in &facts_rows {
        let (line, spots) = fact(label, text, theme, width);
        rows.push((line, spots, None));
    }
    for (at, (line, spots, hit)) in rows.into_iter().enumerate() {
        let y = facts.y + u16::try_from(at).unwrap_or(0);
        if y >= facts.bottom() {
            break;
        }
        let cell = Rect {
            y,
            height: 1,
            ..facts
        };
        if let Some(hit) = hit {
            hits.push(cell, hit);
        }
        put_row(frame, hits, cell, (line, spots));
    }
    if view.curve.hz.is_empty() {
        return;
    }
    match tier.wide {
        false => {
            let y = facts.y + u16::try_from(2 + facts_rows.len()).unwrap_or(0);
            if y < facts.bottom() {
                frame.render_widget(
                    Paragraph::new(strips_row(view, theme, width)),
                    Rect {
                        y,
                        height: 1,
                        ..facts
                    },
                );
            }
        }
        true => {
            let gap = 2;
            let left = facts_width + gap;
            if inner.width > left + 20 {
                let (same, other) = same_other(&view.curve);
                let chart = Rect {
                    x: inner.x + left,
                    width: inner.width - left,
                    ..inner
                };
                draw_curves(frame, chart, (&view.curve.hz, &same, &other), theme);
            }
        }
    }
}

#[cfg(test)]
mod tests;
