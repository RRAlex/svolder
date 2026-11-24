// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config box over the device screen: the sink the check judges, its seven lines
//! each with a mark, what they add up to, and the hops that change the sound.
//!
//! A modal box like the quit question: everything around it is dimmed and it takes every key
//! until `k` or `Esc` closes it. Every word is the judge's; the box only lays them out, and says
//! in its own words where the loop's own read of the graph is on its way or found none.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_ipc::host::{SETUP_LIMIT, SetupOutcome, SetupSummary};

use super::device::path_line::{VALUE_JOIN, role};
use crate::tui::app::App;
use crate::tui::boxes::setup::{self, SetupBox};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::text::{cells, cut_to_cells, wrap};

/// What the box calls itself.
const TITLE: &str = " PipeWire config ";
/// What the box's top border says at the right.
const CLOSE: &str = " k or esc close ";
/// The cells the lines' names take, the longest (`no filter in the sink`) fitting.
const LABEL: usize = 21;
/// The cells before a line's words: the box's blank, the mark, a blank, the name and two blanks.
const WORDS_AT: usize = 1 + 1 + 1 + LABEL + 2;
/// The cells before the words of the sink, `From player` and `To DAC` lines: a blank, the
/// longest name and two blanks.
const IO_LABEL: usize = 1 + 11 + 2;
/// The heading of the section that names every hop that changes the sound.
const CHANGES_HEADING: &str = "What changes the sound";
/// The widest the box grows, border included.
const MAX_WIDTH: u16 = 100;

/// Draw the box over `area` and answer the rectangle it took.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) -> Rect {
    let width = area.width.min(MAX_WIDTH);
    let inner = usize::from(width.saturating_sub(2));
    let mut lines = lines(app, theme, inner);
    // A box too tall for the screen drops its separating blanks before it drops the limit.
    if lines.len() + 2 > usize::from(area.height) {
        lines.retain(|line| !line.spans.is_empty());
    }
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(area.height)
        .min(area.height);
    let box_area = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    widgets::surfaces::modal_ground(frame, box_area, theme);
    let close = Line::from(Span::styled(CLOSE, theme.style(Role::Dim))).right_aligned();
    let block =
        widgets::surfaces::modal_block(Block::bordered(), TITLE, app, theme).title_top(close);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    box_area
}

/// The box's lines for `inner` cells: the sink with what comes in from the player and what the
/// card is open at, a blank, the seven lines with their words wrapped
/// under the words column, a blank, the summary, the hops that change the sound (`changes`), what
/// the box adds of its own read and the limit.
fn lines(app: &App, theme: &Theme, inner: usize) -> Vec<Line<'static>> {
    let check = app.setup_check();
    let mut lines = Vec::new();
    let sink = match check.sink.is_empty() {
        true => "not read".to_string(),
        false => check.sink.clone(),
    };
    let room_io = inner.saturating_sub(IO_LABEL + 1).max(1);
    for (name, words) in [
        ("Sink", sink.as_str()),
        ("From player", check.io.from_player.as_str()),
        ("To DAC", check.io.to_dac.as_str()),
    ] {
        if words.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {name:<width$}", width = IO_LABEL - 1),
                theme.style(Role::Dim),
            ),
            Span::raw(cut_to_cells(words, room_io)),
        ]));
    }
    lines.push(Line::default());
    let room = inner.saturating_sub(WORDS_AT + 1).max(1);
    for line in &check.lines {
        let role = outcome_role(line.outcome);
        let words = wrap(&line.words, room);
        for (at, part) in words.iter().enumerate() {
            let lead = match at {
                0 => vec![
                    Span::raw(" "),
                    Span::styled(
                        setup::mark(line.outcome),
                        theme.style(mark_role(line.outcome)),
                    ),
                    Span::raw(" "),
                    Span::styled(pad(line.label()), theme.style(role)),
                    Span::raw("  "),
                ],
                _ => vec![Span::raw(" ".repeat(WORDS_AT))],
            };
            let mut spans = lead;
            spans.push(Span::styled(part.clone(), theme.style(role)));
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::default());
    let summary = match check.summary {
        SetupSummary::Untouched => Role::Good,
        SetupSummary::Changes(_) => Role::Warn,
        _ => Role::Dim,
    };
    lines.push(Line::from(Span::styled(
        format!(
            " {}",
            cut_to_cells(&check.summary_words(), inner.saturating_sub(1))
        ),
        theme.style(summary),
    )));
    if let Some(note) = own_read_note(app) {
        lines.push(Line::from(Span::styled(
            format!(" {}", cut_to_cells(note, inner.saturating_sub(1))),
            theme.style(Role::Dim),
        )));
    }
    changes(app, theme, room, &mut lines);
    lines.push(Line::default());
    for part in wrap(SETUP_LIMIT, inner.saturating_sub(2).max(1)) {
        lines.push(Line::from(Span::styled(
            format!(" {part}"),
            theme.style(Role::Dim),
        )));
    }
    lines
}

/// The section under the summary that names what changes the sound: a blank, the heading and
/// one row per hop of the Path row's `changes` (its label, then its words each in its own tone).
/// Nothing when the row names no hop.
fn changes(app: &App, theme: &Theme, room: usize, lines: &mut Vec<Line<'static>>) {
    let Some(hops) = app.path_row().map(|row| row.changes) else {
        return;
    };
    if hops.is_empty() {
        return;
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!(" {CHANGES_HEADING}"),
        theme.style(Role::Text),
    )));
    for hop in &hops {
        // Whole words, each in its own tone, joined by a dim ` · `; a line breaks only between
        // two words, never before a join.
        let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new()];
        let mut used = 0;
        for (at, word) in hop.words.iter().enumerate() {
            let join = if at == 0 { 0 } else { cells(VALUE_JOIN) };
            let own = cells(&word.text);
            if used > 0 && used + join + own > room {
                rows.push(Vec::new());
                used = 0;
            } else if used > 0 {
                rows.last_mut()
                    .expect("a row")
                    .push(Span::styled(VALUE_JOIN, theme.style(Role::Dim)));
                used += join;
            }
            rows.last_mut().expect("a row").push(Span::styled(
                cut_to_cells(&word.text, room),
                theme.style(role(word.tone)),
            ));
            used += own.min(room);
        }
        for (at, spans) in rows.into_iter().enumerate() {
            let mut line = match at {
                0 => vec![
                    Span::raw("   "),
                    Span::styled(pad(&hop.label), theme.style(Role::Text)),
                    Span::raw("  "),
                ],
                _ => vec![Span::raw(" ".repeat(WORDS_AT))],
            };
            line.extend(spans);
            lines.push(Line::from(line));
        }
    }
}

/// What the box says of its own read, where no daemon gives the route: that the first one is on
/// its way, or that it found no graph.
fn own_read_note(app: &App) -> Option<&'static str> {
    if app.host.daemon.is_some() {
        return None;
    }
    let SetupBox { own, failed, .. } = app.setup_box()?;
    match (own, failed) {
        (_, true) => Some(setup::UNREAD),
        (None, false) => Some(setup::READING),
        (Some(_), false) => None,
    }
}

/// `text` padded with blanks to the name column's width.
fn pad(text: &str) -> String {
    let fill = LABEL.saturating_sub(cells(text));
    format!("{text}{}", " ".repeat(fill))
}

/// The colour a line's name and words are in: a failing line stands out, the rest are text.
fn outcome_role(outcome: SetupOutcome) -> Role {
    match outcome {
        SetupOutcome::Fail => Role::Warn,
        SetupOutcome::Pass | SetupOutcome::Unknown => Role::Text,
    }
}

/// The colour of a line's mark.
fn mark_role(outcome: SetupOutcome) -> Role {
    match outcome {
        SetupOutcome::Pass => Role::Good,
        SetupOutcome::Fail => Role::Warn,
        SetupOutcome::Unknown => Role::Dim,
    }
}
