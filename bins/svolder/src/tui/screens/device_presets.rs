// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The box of a device's own presets, over the device screen.
//!
//! It is a modal box like the tone control's preset list: everything around it is dimmed, and it
//! names its own keys on its last lines. It shows the slots, a `Built in` header and the built-in
//! settings, or the `[y/N]` question a save asks.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::app::App;
use crate::tui::boxes::device_presets::{self, DeviceList, Question, Row, Step};
use crate::tui::mouse::{HitMap, Selected};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::scroll::{MORE_DOWN, MORE_UP};
use crate::tui::widgets::{self, CURSOR_WIDTH};

use super::device::row_style;
use super::devices::boxed;

/// The keys that act on the list as a whole.
const LIST_KEYS: [(&str, &str); 2] = [("Enter", "loads"), ("Esc", "closes")];
/// The key that saves the bands over the focused slot.
const SAVE_KEY: (&str, &str) = ("u", "saves");
/// Why the save key is dimmed while a built-in is focused.
const SLOTS_ONLY: &str = "device slots only";
/// What marks the row of the built-in the bands read.
const ON: &str = "●";
/// The keys that answer the question a save asks; no is the default.
const SAVE_ANSWER_KEYS: [(&str, &str); 2] = [("y", "saves"), ("Enter/n/Esc", "cancels")];
/// Blank cells the box keeps inside its right border after its longest line, as the one space
/// every line opens with keeps off its left.
const TRAIL: usize = 1;
/// Rows the box takes besides the device's rows: two borders, the blank line and two lines of
/// keys.
const CHROME: u16 = 5;

/// Draw whatever the list is showing over `area` and answer the box it drew in; nothing while
/// the list is closed.
///
/// The row the keys select in the list is recorded in `hits`.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> Option<Rect> {
    let list = app.presets.device_list.as_ref()?;
    Some(match &list.step {
        Step::Choosing => draw_list(frame, area, app, list, (theme, hits)),
        Step::Asking(question) => draw_question(frame, area, app, question, theme),
    })
}

/// Draw the rows over `area`: the slots, the header and the built-ins, the one the bands read
/// marked `●`, then the keys, the save key dimmed with the reason while a built-in is focused.
/// A list longer than the box holds shows the rows round the focused one and says on its border
/// which way the rest are.
fn draw_list(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    list: &DeviceList,
    (theme, hits): (&Theme, &mut HitMap),
) -> Rect {
    let rows = device_presets::rows(&app.profile);
    let on = device_presets::on(app);
    let row = list.row.min(rows.len().saturating_sub(1));
    let focused = rows.get(row).copied();
    let names: Vec<String> = rows.iter().map(|row| row.label(&app.profile)).collect();
    let longest = names
        .iter()
        .map(|name| widgets::text::cells(name))
        .max()
        .unwrap_or(0);
    let keys = [
        widgets::surfaces::modal_keys(&LIST_KEYS, theme),
        save_keys(matches!(focused, Some(Row::Setting(_))), theme),
    ];
    // The box is as wide as the widest thing it can show, the reason included, so it keeps its
    // size while the focus moves between a slot and a built-in.
    let widest = [
        CURSOR_WIDTH + widgets::text::cells(ON) + 1 + longest,
        keys[0].width(),
        save_keys(true, theme).width(),
        save_keys(false, theme).width(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    let width = u16::try_from(widest + TRAIL + 2).unwrap_or(u16::MAX);
    let room = usize::from(area.height.saturating_sub(CHROME)).max(1);
    let shown = rows.len().min(room);
    let top = (row + 1).saturating_sub(shown);
    let max_width = usize::from(width.saturating_sub(2));
    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .zip(&names)
        .enumerate()
        .skip(top)
        .take(shown)
        .map(|(at, (kind, name))| match kind {
            Row::Header => header_line(name, theme),
            Row::Slot(_) | Row::Setting(_) => {
                let is_on =
                    matches!((kind, on), (Row::Setting(setting), Some(on)) if *setting == on);
                row_line(name, is_on, at == row, theme, max_width)
            }
        })
        .collect();
    lines.push(Line::default());
    lines.extend(keys);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    let title = format!(" Presets · {} ", app.profile.display);
    let mut block = widgets::surfaces::modal_block(Block::bordered(), &title, app, theme);
    if top > 0 {
        block = block.title(Line::from(MORE_UP).right_aligned());
    }
    if top + shown < rows.len() {
        block = block.title_bottom(Line::from(MORE_DOWN).right_aligned());
    }
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
    if !rows.is_empty() {
        let inner = Block::bordered().inner(box_area);
        let at = Rect {
            y: inner.y + u16::try_from(row - top).unwrap_or(0),
            height: 1,
            ..inner
        };
        hits.select(Selected::Presets, row, at.intersection(inner));
    }
    widgets::surfaces::fade_list(
        frame.buffer_mut(),
        app.list_fade,
        Block::bordered().inner(box_area),
        top,
        shown,
        row,
        theme,
    );
    box_area
}

/// One slot's or built-in's row: the cursor, `●` for the built-in that is on, and its name.
fn row_line(name: &str, on: bool, focused: bool, theme: &Theme, max_width: usize) -> Line<'static> {
    let mark = match on {
        true => Span::styled(ON, theme.style(Role::Good)),
        false => Span::raw(" "),
    };
    let line = Line::from(vec![mark, Span::raw(" "), Span::raw(name.to_string())]);
    row_style(
        widgets::rows::with_cursor(line, focused, theme),
        focused,
        false,
        theme,
        max_width,
    )
}

/// The header over the built-ins: held back in the faint grey, in the columns the names use, with
/// no cursor.
fn header_line(name: &str, theme: &Theme) -> Line<'static> {
    let indent = " ".repeat(CURSOR_WIDTH + widgets::text::cells(ON) + 1);
    Line::from(vec![
        Span::raw(indent),
        Span::styled(name.to_string(), theme.style(Role::Faint)),
    ])
}

/// The key that saves: as every modal box names its keys, or, with a built-in focused, in the
/// faint grey with the reason it does nothing after it.
fn save_keys(builtin: bool, theme: &Theme) -> Line<'static> {
    if !builtin {
        return widgets::surfaces::modal_keys(&[SAVE_KEY], theme);
    }
    let faint = theme.style(Role::Faint);
    Line::from(vec![
        Span::raw(" "),
        Span::styled(SAVE_KEY.0, faint),
        Span::styled(format!(" {} · {SLOTS_ONLY}", SAVE_KEY.1), faint),
    ])
}

/// Draw the question a save asks over `area`: which slot it is about, and the keys that answer
/// it.
fn draw_question(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    question: &Question,
    theme: &Theme,
) -> Rect {
    let Question::Save(slot) = question;
    let lines = vec![
        Line::from(Span::raw(format!(
            " Save the bands to preset {slot}? [y/N]"
        ))),
        Line::default(),
        widgets::surfaces::modal_keys(&SAVE_ANSWER_KEYS, theme),
    ];
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest + TRAIL + 2).unwrap_or(u16::MAX);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let box_area = boxed(area, width, height);
    widgets::surfaces::modal_ground(frame, box_area, theme);
    frame.render_widget(
        Paragraph::new(lines).block(widgets::surfaces::modal_block(
            Block::bordered(),
            " Save ",
            app,
            theme,
        )),
        box_area,
    );
    box_area
}

#[cfg(test)]
mod tests;
