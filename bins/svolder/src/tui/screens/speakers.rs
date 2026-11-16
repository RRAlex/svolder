// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers' set list box, over the tone screen or the device screen.
//!
//! A modal box like the preset list: everything around it is dimmed and it names its own keys on
//! its last line. It shows the sets one to a row (the name, the choice with its technical term,
//! where the set stands, the licence), the focused row's licence line and source under them, and
//! a status line with the counts and the fetch in flight. The file prompt and the forget
//! question are boxes of their own size.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_ipc::SpeakersRoom;

use crate::tui::app::App;
use crate::tui::boxes::speakers::{
    Fetch, Filter, ImportLine, Row, SetsBox, Step, room_words, size_text,
};
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::row_style;
use super::devices::boxed;

/// The most columns the list takes, borders and all: it is the terminal's width less a margin
/// of one column on each side, up to this.
const WIDTH_MAX: u16 = 100;
/// The fewest columns the licence column is shown with; with less room it is left out (the
/// focused row's licence line under the rows says it whole).
const LICENCE_MIN: usize = 8;
/// Set rows the list shows at most.
const ROWS: usize = 8;
/// Columns between two columns of a row.
const GAP: usize = 2;
/// Columns of the download bar, without its percent.
const BAR: usize = 8;
/// The keys the list names.
const KEYS: [(&str, &str); 6] = [
    ("\u{2191}\u{2193}", "move"),
    ("Enter", "fetch/use"),
    ("x", "cancel"),
    ("f", "forget"),
    ("i", "file"),
    ("Esc", "close"),
];
/// The keys of the file prompt.
const FILE_KEYS: [(&str, &str); 3] = [("Tab", "room"), ("Enter", "imports"), ("Esc", "back")];
/// The keys of the forget question.
const FORGET_KEYS: [(&str, &str); 2] = [("y", "forgets"), ("Enter/n/Esc", "keeps")];
/// What the file prompt's path row is called, and the choice row.
const FILE: &str = "File";
/// The columns the labels of the file prompt take, the space before the value included.
const FIELD: usize = 8;
/// Columns the file prompt and the question take.
const SMALL: u16 = 60;

/// Draw whatever the set list is showing over `area`, and answer the box it drew in; nothing
/// while it is closed.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Option<Rect> {
    let list = app.speakers_box.list.as_ref()?;
    Some(match &list.step {
        Step::Choosing => draw_list(frame, area, app, list, (theme, hits)),
        Step::Importing(line) => draw_file(frame, area, app, line, theme),
        Step::Forgetting(_, label) => draw_forget(frame, area, app, label, theme),
    })
}

/// Where a set stands, in words and a role: the text of its state column.
fn state_of(row: &Row, fetch: Option<&Fetch>) -> (String, Role) {
    if let Some(fetch) = fetch.filter(|fetch| !fetch.import && fetch.id == row.id) {
        let text = match fetch.percent() {
            Some(pct) => {
                let filled = usize::try_from(pct).unwrap_or(0) * BAR / 100;
                format!(
                    "\u{2595}{}{}\u{258f} {pct}%",
                    "\u{2588}".repeat(filled),
                    "\u{2591}".repeat(BAR - filled)
                )
            }
            None => "starting\u{2026}".to_string(),
        };
        let text = match fetch.stopping {
            true => "stopping\u{2026}".to_string(),
            false => text,
        };
        return (text, Role::SelectName);
    }
    match (&row.damaged, row.chosen, row.fetched) {
        (Some(_), _, _) => ("damaged".to_string(), Role::Warn),
        (None, true, _) => ("chosen".to_string(), Role::SelectName),
        (None, false, true) => ("fetched".to_string(), Role::Text),
        (None, false, false) => (size_text(row.bytes), Role::Dim),
    }
}

/// The status line: the counts, and the fetch in flight or why the catalogue is missing.
fn status(app: &App, rows: usize) -> (String, Role) {
    let sets = &app.speakers_box;
    if let Some(fetch) = &sets.fetch {
        if fetch.stopping {
            return (format!("stopping {}\u{2026}", fetch.label), Role::Text);
        }
        let what = match fetch.import {
            true => format!("importing {}", fetch.label),
            false => match (fetch.percent(), fetch.total) {
                (Some(_), total) => format!(
                    "fetching {}, {} of {}",
                    fetch.label,
                    size_text(fetch.received),
                    size_text(total)
                ),
                (None, _) => format!("fetching {}", fetch.label),
            },
        };
        return (what, Role::Text);
    }
    if let Some(why) = &sets.catalogue_error {
        return (why.clone(), Role::Warn);
    }
    let fetched = app.sets_rows().iter().filter(|row| row.fetched).count();
    let mut text = format!("{rows} sets \u{b7} {fetched} fetched");
    if !app.session.daemon {
        text.push_str(" \u{b7} fetching needs svolderd");
    } else if sets.catalogue.is_none() {
        text.push_str(" \u{b7} asking the daemon\u{2026}");
    }
    if let Some(note) = sets.catalogue.as_ref().and_then(|c| c.note.as_ref()) {
        text.push_str(&format!(" \u{b7} {note}"));
    }
    (text, Role::Dim)
}

/// The list: the filter row, a row per set, the focused set's licence line and source, the status
/// and the keys.
fn draw_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    list: &SetsBox,
    (theme, hits): (&Theme, &mut HitMap),
) -> Rect {
    let rows = app.sets_rows();
    let row = list.row.min(rows.len().saturating_sub(1));
    // Two borders, the filter, a blank, the licence and source lines, the status and the keys.
    let rows_room = usize::from(area.height.saturating_sub(9)).clamp(1, ROWS);
    let shown = rows.len().min(rows_room);
    let top = (row + 1).saturating_sub(shown);
    let width = area
        .width
        .saturating_sub(2)
        .clamp(1, WIDTH_MAX)
        .min(area.width);
    let height = u16::try_from(rows_room + 8).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let inner = Block::bordered().inner(box_area);
    let across = usize::from(inner.width);
    let fetch = app.speakers_box.fetch.as_ref();
    let states: Vec<(String, Role)> = rows.iter().map(|r| state_of(r, fetch)).collect();
    let rooms = [SpeakersRoom::With, SpeakersRoom::No]
        .into_iter()
        .map(|room| widgets::text::cells(&room_words(room)))
        .max()
        .unwrap_or(0);
    let state_width = states
        .iter()
        .skip(top)
        .take(shown)
        .map(|(text, _)| widgets::text::cells(text))
        .max()
        .unwrap_or(0);
    let licence_width = rows
        .iter()
        .skip(top)
        .take(shown)
        .map(|r| widgets::text::cells(&r.licence))
        .max()
        .unwrap_or(0);
    let name_want = rows
        .iter()
        .skip(top)
        .take(shown)
        .map(|r| widgets::text::cells(&r.label))
        .max()
        .unwrap_or(0);
    // The cursor, the `●`, a space, then the columns. The names get their room first: the
    // licence column takes what is left, cut short, and is left out below LICENCE_MIN.
    let fixed = CURSOR_WIDTH + 2 + GAP + rooms + GAP + state_width;
    let room = across.saturating_sub(fixed + 1);
    let licence_width = match room.saturating_sub(name_want + GAP) {
        left if left >= LICENCE_MIN.min(licence_width) && licence_width > 0 => {
            left.min(licence_width)
        }
        _ => 0,
    };
    let name_width = name_want.min(room.saturating_sub(match licence_width {
        0 => 0,
        shown => GAP + shown,
    }));
    let plain = ratatui::style::Style::default();
    let (dim, accent) = (theme.style(Role::Dim), theme.style(Role::SelectName));
    let arrow = theme.style(Role::ModeArrow);
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::raw(" Filter  "),
            Span::styled("\u{2039} ", arrow),
            Span::styled(list.filter.words(), accent),
            Span::styled(" \u{203a}", arrow),
            Span::styled(
                format!(
                    "      Tab: {} \u{b7} {} \u{b7} {}",
                    Filter::All.words(),
                    Filter::With.words(),
                    Filter::No.words()
                ),
                dim,
            ),
        ]),
        Line::default(),
    ];
    for (at, (r, (state, role))) in rows.iter().zip(&states).enumerate().skip(top).take(shown) {
        let focused = at == row;
        let mark = match r.chosen {
            true => "\u{25cf}",
            false => " ",
        };
        let name = widgets::text::pad_cells(
            &widgets::text::cut_to_cells(&r.label, name_width),
            name_width,
        );
        let room = widgets::text::pad_cells(&room_words(r.room), rooms);
        let state = widgets::text::pad_cells(state, state_width);
        let span = |text: String, role: Role| match focused {
            true => Span::raw(text),
            false => Span::styled(text, theme.style(role)),
        };
        let mut spans = vec![
            Span::styled(format!("{mark} "), if r.chosen { accent } else { plain }),
            span(name, Role::Text),
            Span::raw(" ".repeat(GAP)),
            span(room, Role::Dim),
            Span::raw(" ".repeat(GAP)),
            span(state, *role),
        ];
        if licence_width > 0 {
            spans.push(Span::raw(" ".repeat(GAP)));
            spans.push(span(
                widgets::text::cut_to_cells(&r.licence, licence_width),
                Role::Dim,
            ));
        }
        let line = Line::from(spans);
        lines.push(row_style(
            widgets::rows::with_cursor(line, focused, theme),
            focused,
            false,
            theme,
            across,
        ));
    }
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "   no sets",
            theme.style(Role::Faint),
        )));
    }
    while lines.len() < 2 + rows_room {
        lines.push(Line::default());
    }
    // The focused set's licence line and its source.
    let (licence, source) = match rows.get(row) {
        Some(r) => {
            let licence = match r.credit.is_empty() {
                true => r.licence.clone(),
                false => format!("{} \u{b7} {}", r.licence, r.credit),
            };
            let mut source = r.source_url.clone();
            if r.cut {
                let cut = match r.taps_ms > 0.0 {
                    true => crate::tui::app::speakers::length_words(r.taps_ms, true),
                    false => "cut".to_string(),
                };
                source = match source.is_empty() {
                    true => cut,
                    false => format!("{source} \u{b7} {cut}"),
                };
            }
            if let Some(why) = &r.damaged {
                source = why.clone();
            }
            (licence, source)
        }
        None => (String::new(), String::new()),
    };
    let cut = |text: &str| {
        format!(
            " {}",
            widgets::text::cut_to_cells(text, across.saturating_sub(2))
        )
    };
    lines.push(Line::from(Span::styled(cut(&licence), plain)));
    lines.push(Line::from(Span::styled(cut(&source), dim)));
    let (words, role) = status(app, rows.len());
    lines.push(Line::from(Span::styled(cut(&words), theme.style(role))));
    lines.push(widgets::surfaces::modal_keys(&KEYS, theme));
    let mut block =
        widgets::surfaces::modal_block(Block::bordered(), " Virtual speakers sets ", app, theme);
    if top > 0 {
        block = block.title(Line::from(MORE_UP).right_aligned());
    }
    if top + shown < rows.len() {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    // The filter and a blank line come before the first row.
    for (at, index) in (top..top + shown).enumerate() {
        let line = Rect {
            y: inner.y + 2 + u16::try_from(at).unwrap_or(u16::MAX),
            height: 1,
            ..inner
        };
        hits.push(line.intersection(inner), Hit::SetRow(index));
    }
    if !rows.is_empty() {
        let at = Rect {
            y: inner.y + 2 + u16::try_from(row - top).unwrap_or(0),
            height: 1,
            ..inner
        };
        hits.select(Selected::Sets, row, at.intersection(inner));
    }
    box_area
}

/// The file prompt: the path as typed with the cursor at its end, the choice the file goes
/// under, and the keys.
fn draw_file(frame: &mut Frame, area: Rect, app: &App, line: &ImportLine, theme: &Theme) -> Rect {
    let box_area = boxed(area, SMALL.min(area.width), 6);
    let inner = Block::bordered().inner(box_area);
    let across = usize::from(inner.width);
    let room = across.saturating_sub(1 + FIELD);
    let end = line.text.chars().count();
    let (typed, column) = widgets::text::typed_window(&line.text, end, room);
    let (dim, accent) = (theme.style(Role::Dim), theme.style(Role::SelectName));
    let lines = vec![
        Line::from(vec![
            Span::raw(" "),
            widgets::rows::label(FILE, FIELD),
            Span::raw(typed),
        ]),
        Line::from(vec![
            Span::raw(" "),
            widgets::rows::label("Choice", FIELD),
            Span::styled(room_words(line.room), accent),
            Span::styled("  (a SOFA file or a WAV)", dim),
        ]),
        Line::default(),
        widgets::surfaces::modal_keys(&FILE_KEYS, theme),
    ];
    let block = widgets::surfaces::modal_block(Block::bordered(), " Import a file ", app, theme);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    let x = inner.x + u16::try_from(1 + FIELD + column).unwrap_or(u16::MAX);
    if x < inner.right() && inner.height > 0 {
        frame.set_cursor_position(Position { x, y: inner.y });
    }
    box_area
}

/// The question a forget asks.
fn draw_forget(frame: &mut Frame, area: Rect, app: &App, label: &str, theme: &Theme) -> Rect {
    let box_area = boxed(area, SMALL.min(area.width), 5);
    let inner = Block::bordered().inner(box_area);
    let across = usize::from(inner.width);
    let text = format!(
        " Forget {}?",
        widgets::text::cut_to_cells(label, across.saturating_sub(10))
    );
    let lines = vec![
        Line::from(text),
        Line::default(),
        widgets::surfaces::modal_keys(&FORGET_KEYS, theme),
    ];
    let block = widgets::surfaces::modal_block(Block::bordered(), " Forget a set ", app, theme);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    box_area
}

#[cfg(test)]
mod tests;
