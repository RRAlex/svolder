// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Scenes box, over any screen.
//!
//! A modal box like the preset list: everything around it is dimmed. Its top border carries the
//! title and, right-aligned, the keys (`↵ recall ─ n new ─ u update ─ c rename ─ x delete ─`, the
//! last `─` keeping the corner). One row per scene: `●` lit for the scene that is on and `○` for
//! the rest, the name, and the scene's parts as dim words; the cursor marks the focused row. The
//! box's last row stays free for what it asks: the `[y/N]` question and the name prompt.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use svolder_ipc::socket::protocol::{SceneRow, ScenesView};

use crate::tui::app::App;
use crate::tui::boxes::scenes::{NameLine, Question, Step};
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::row_style;
use super::devices::boxed;

/// The box's title.
const TITLE: &str = " Scenes ";
/// The keys on the top border, each with what it does.
const KEYS: [(&str, &str); 5] = [
    ("↵", "recall"),
    ("n", "new"),
    ("u", "update"),
    ("c", "rename"),
    ("x", "delete"),
];
/// What joins two keys on the border.
const JOIN: &str = " ─ ";
/// What the lamp of the scene that is on reads, and of every other.
const LAMP_ON: &str = "●";
const LAMP_OFF: &str = "○";
/// What joins a scene's parts.
const PARTS_JOIN: &str = " · ";
/// What follows the name of a part that cannot be recalled now.
const ABSENT: &str = " (not connected)";
/// Columns between a scene's name and its parts.
const GAP: usize = 2;
/// Rows the box shows at least, so it keeps a shape with one scene or none.
const MIN_ROWS: usize = 3;
/// What the box says over its rows while there is no scene.
const NONE_YET: &str = "no scenes yet: n saves the sound as it is now";
/// Cells the rename prompt gives the old name in its label.
const OLD_NAME_ROOM: usize = 16;
/// What the name prompt calls its line for a new scene.
const NAME_LABEL: &str = "Save current as: ";
/// What the name prompt's keys say, after the typed name.
const NAME_KEYS: &str = "↵ save · esc cancel";
/// What a rename prompt's keys say.
const RENAME_KEYS: &str = "↵ rename · esc cancel";
/// What the box asks before it deletes.
const DELETE_ASK: &str = "? It is removed for every device. [y/N]";

/// The border's key line: the keys in the key-hint role and what they do in the text, joined by
/// a rule, ending in a rule so the box's corner stays.
fn key_line(theme: &Theme) -> Line<'static> {
    let rule = theme.style(Role::Rule);
    let key = theme.style(Role::KeyHint).add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::raw(" ")];
    for (at, (code, does)) in KEYS.iter().enumerate() {
        if at > 0 {
            spans.push(Span::styled(JOIN, rule));
        }
        spans.push(Span::styled(*code, key));
        spans.push(Span::styled(format!(" {does}"), theme.style(Role::Text)));
    }
    spans.push(Span::styled(" ─", rule));
    Line::from(spans)
}

/// A scene's parts as words: who, and `(not connected)` after one that cannot be recalled now.
fn parts_of(scene: &SceneRow) -> String {
    scene
        .parts
        .iter()
        .map(|part| match part.present {
            true => part.who.clone(),
            false => format!("{}{ABSENT}", part.who),
        })
        .collect::<Vec<_>>()
        .join(PARTS_JOIN)
}

/// Whether the lamp of `scene` is lit: the daemon says it is on, and says it is settled.
fn lit(view: &ScenesView, scene: &SceneRow) -> bool {
    view.known && view.on.as_deref() == Some(scene.name.as_str())
}

/// One scene's row: the cursor, the lamp, the name and the parts as dim words, the whole row
/// selected under the cursor.
fn scene_line(
    view: &ScenesView,
    scene: &SceneRow,
    focused: bool,
    name_width: usize,
    theme: &Theme,
    max_width: usize,
) -> Line<'static> {
    let lamp = match lit(view, scene) {
        true => Span::styled(LAMP_ON, theme.style(Role::Good)),
        false => Span::raw(LAMP_OFF),
    };
    let room = max_width.saturating_sub(CURSOR_WIDTH + 2 + name_width + GAP);
    let parts = widgets::text::cut_to_cells(&parts_of(scene), room);
    let parts = match focused {
        true => Span::raw(parts),
        false => Span::styled(parts, theme.style(Role::Faint)),
    };
    let name = widgets::text::cut_to_cells(&scene.name, name_width);
    let spans = vec![
        lamp,
        Span::raw(" "),
        Span::raw(widgets::text::pad_cells(&name, name_width)),
        Span::raw(" ".repeat(GAP)),
        parts,
    ];
    row_style(
        widgets::rows::with_cursor(Line::from(spans), focused, theme),
        focused,
        false,
        theme,
        max_width,
    )
}

/// What the question `question` asks, for the box's last row `room` cells wide: the name or the
/// daemon's words are cut with `…` where the row is too short, and the `[y/N]` stays.
fn question_words(question: &Question, room: usize) -> String {
    let (before, flexible, after) = match question {
        Question::Update(name) => (
            "Save the sound as it is now over ",
            name.as_str(),
            "? [y/N]",
        ),
        Question::Delete(name) => ("Delete ", name.as_str(), DELETE_ASK),
        Question::Both { message, .. } => ("", message.as_str(), " [y/N]"),
    };
    let fixed = widgets::text::cells(before) + widgets::text::cells(after);
    let flexible = match widgets::text::cells(flexible) + fixed <= room {
        true => flexible.to_string(),
        false => widgets::text::cut_to_cells(flexible, room.saturating_sub(fixed)),
    };
    format!("{before}{flexible}{after}")
}

/// Draw the box over `area` and answer the box it drew in; nothing while the box is closed or the
/// view is gone.
///
/// The row the keys select, and each scene row's cells, are recorded in `hits`.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Option<Rect> {
    let list = app.scenes.list.as_ref()?;
    let view = app.scenes.view.as_ref()?;
    let count = view.scenes.len();
    let row = list.row.min(count.saturating_sub(1));

    let keys = key_line(theme);
    let longest_name = view
        .scenes
        .iter()
        .map(|scene| widgets::text::cells(&scene.name))
        .max()
        .unwrap_or(0);
    let longest_row = view
        .scenes
        .iter()
        .map(|scene| widgets::text::cells(&parts_of(scene)))
        .max()
        .unwrap_or(0)
        + CURSOR_WIDTH
        + 2
        + longest_name
        + GAP;
    let ask = match &list.step {
        // The daemon's question is wrapped over the rows it needs, so it asks the box for no width.
        Step::Asking(Question::Both { .. }) => 0,
        Step::Asking(question) => widgets::text::cells(&question_words(question, usize::MAX)) + 1,
        Step::Naming(_) => widgets::text::cells(NAME_LABEL) + 1 + widgets::text::cells(NAME_KEYS),
        Step::Choosing => 0,
    };
    // The border holds the title, the keys and a rule between; the rows and the last row the
    // widest of what they hold. One blank cell closes each row on the right.
    let border = widgets::text::cells(TITLE) + keys.width() + 2;
    let across = [longest_row, ask, border]
        .into_iter()
        .max()
        .unwrap_or(0)
        .max(widgets::text::cells(NONE_YET) + 2)
        + 1;
    let width = u16::try_from(across + 2)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let inner_width = usize::from(width.saturating_sub(2));

    // A blank row opens the box, the last rows hold what it asks (a wrapped question takes as many
    // as it needs), and two borders.
    let ask_rows = match &list.step {
        Step::Asking(Question::Both { message, .. }) => {
            widgets::text::wrap(&format!("{message} [y/N]"), inner_width.saturating_sub(2)).len()
        }
        _ => 1,
    };
    let room = usize::from(area.height).saturating_sub(3 + ask_rows).max(1);
    let shown = count.max(MIN_ROWS).min(room);
    let top = (row + 1)
        .saturating_sub(shown)
        .min(count.saturating_sub(shown));
    let name_width = longest_name.min(inner_width.saturating_sub(CURSOR_WIDTH + 2 + GAP + 8));
    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    if count == 0 {
        lines.push(Line::from(Span::styled(
            format!(" {NONE_YET}"),
            theme.style(Role::Dim),
        )));
    }
    for (at, scene) in view.scenes.iter().enumerate().skip(top).take(shown) {
        lines.push(scene_line(
            view,
            scene,
            at == row,
            name_width,
            theme,
            inner_width.saturating_sub(1),
        ));
    }
    while lines.len() < 1 + shown {
        lines.push(Line::default());
    }
    let last = lines.len();
    let say = |text: String| Line::from(Span::styled(format!(" {text}"), theme.style(Role::Text)));
    match &list.step {
        Step::Choosing => lines.push(match app.scenes.pending.as_deref() {
            Some(words) => Line::from(Span::styled(format!(" {words}"), theme.style(Role::Dim))),
            None => Line::default(),
        }),
        Step::Naming(line) => lines.push(prompt_line(line, inner_width, theme)),
        // The daemon's question is the whole of it over as many rows as it needs, `[y/N]` after
        // its last word.
        Step::Asking(Question::Both { message, .. }) => lines.extend(
            widgets::text::wrap(&format!("{message} [y/N]"), inner_width.saturating_sub(2))
                .into_iter()
                .map(say),
        ),
        Step::Asking(question) => {
            lines.push(say(question_words(question, inner_width.saturating_sub(2))))
        }
    }
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let mut block = widgets::surfaces::modal_block(Block::bordered(), TITLE, app, theme)
        .title(keys.right_aligned());
    if top > 0 {
        block = block.title_bottom(Line::from(MORE_UP).right_aligned());
    } else if top + shown < count {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    let inner = Block::bordered().inner(box_area);
    for (at, index) in (top..(top + shown).min(count)).enumerate() {
        let line = Rect {
            y: inner.y + 1 + u16::try_from(at).unwrap_or(u16::MAX),
            height: 1,
            ..inner
        };
        hits.push(line.intersection(inner), Hit::SceneRow(index));
    }
    if count > 0 && row >= top {
        let at = Rect {
            y: inner.y + 1 + u16::try_from(row - top).unwrap_or(0),
            height: 1,
            ..inner
        };
        hits.select(Selected::Scenes, row, at.intersection(inner));
    }
    // The name prompt's terminal cursor stands where the next letter goes.
    if let Step::Naming(line) = &list.step {
        let label = prompt_label(line);
        let room = prompt_room(line, inner_width);
        let (_, column) = widgets::text::typed_window(&line.text, line.caret, room);
        let x = inner.x + u16::try_from(1 + widgets::text::cells(&label) + column).unwrap_or(0);
        let y = inner.y + u16::try_from(last).unwrap_or(0);
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position(Position { x, y });
        }
    }
    Some(box_area)
}

/// What the name prompt calls its line: it saves the sound as it is now, or renames a scene.
fn prompt_label(line: &NameLine) -> String {
    match line.renaming.as_deref() {
        // A long old name is cut, so the typed name and the keys' words keep their room.
        Some(old) => format!(
            "Rename {} to: ",
            widgets::text::cut_to_cells(old, OLD_NAME_ROOM)
        ),
        None => NAME_LABEL.to_string(),
    }
}

/// The cells the typed name has on the prompt's row: what the box's width leaves after the
/// label and the keys' words.
fn prompt_room(line: &NameLine, inner_width: usize) -> usize {
    let keys = match line.renaming {
        Some(_) => RENAME_KEYS,
        None => NAME_KEYS,
    };
    let label = widgets::text::cells(&prompt_label(line));
    inner_width
        .saturating_sub(1 + label + 2 + widgets::text::cells(keys) + 1)
        .max(8)
}

/// The name prompt's row: the label, the name as typed, and the keys' words dim after it.
fn prompt_line(line: &NameLine, inner_width: usize, theme: &Theme) -> Line<'static> {
    let room = prompt_room(line, inner_width);
    let (shown, _) = widgets::text::typed_window(&line.text, line.caret, room);
    let keys = match line.renaming {
        Some(_) => RENAME_KEYS,
        None => NAME_KEYS,
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(prompt_label(line), theme.style(Role::Text)),
        Span::raw(widgets::text::pad_cells(&shown, room)),
        Span::raw("  "),
        Span::styled(keys, theme.style(Role::Dim)),
    ])
}

#[cfg(test)]
mod tests;
