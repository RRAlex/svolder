// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menu `Esc` opens on the device screen: the small wordmark, the versions, and the three
//! things the keys elsewhere also reach — Settings, Help and Quit.
//!
//! It is a modal box like the help overlay: everything around it is dimmed, the box is a fixed
//! [`WIDTH`]×[`HEIGHT`] in the middle of the screen at every terminal size, and it takes every key
//! until it closes. The cursor over its entries is
//! [`Overlays::menu`](crate::tui::app::Overlays::menu).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::app::App;
use crate::tui::decoration::wordmark::{self, Size};
use crate::tui::mouse::{Hit, HitMap, Selected};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

/// The entries, top to bottom. The first letter of each is the key that takes it.
pub const ENTRIES: [&str; 3] = ["Settings", "Help", "Quit"];
/// Columns the box takes: the small mark's thirty, two blank cells each side, and the border.
pub const WIDTH: u16 = 36;
/// Rows the box takes: a blank, the mark's two, the version, a blank, the three entries, a blank,
/// and the border.
pub const HEIGHT: u16 = 11;
/// Blank cells the mark keeps from each side of the box's inside.
const MARK_INDENT: usize = 2;
/// Blank cells the version row keeps from the border on its right.
const VERSION_TRAIL: usize = 1;

/// The version row: this svolder's version, dim, then — on a daemon's session — the daemon's
/// own, dim where it is the same (another build of it included) and in the warn colour where it
/// differs, as the skew toast says.
pub fn version_line(app: &App, theme: &Theme) -> Line<'static> {
    let own = env!("CARGO_PKG_VERSION");
    let mut spans = vec![Span::styled(format!("v{own}"), theme.style(Role::Dim))];
    if let Some(daemon) = &app.session.daemon_version {
        let role = match svolder_ipc::socket::daemon_version::skew(daemon, own) {
            None => Role::Dim,
            Some(_) => Role::Warn,
        };
        spans.push(Span::styled(
            format!(" · daemon v{daemon}"),
            theme.style(role),
        ));
    }
    spans.push(Span::raw(" ".repeat(VERSION_TRAIL)));
    Line::from(spans).right_aligned()
}

/// One entry, centred: its first letter bold in the key-hint role and the rest in the text, or
/// the whole of it selected under the cursor.
fn entry_line(name: &'static str, selected: bool, theme: &Theme) -> Line<'static> {
    if selected {
        return Line::from(Span::styled(name, theme.style(Role::Selected))).centered();
    }
    let (first, rest) = name.split_at(1);
    Line::from(vec![
        Span::styled(
            first,
            theme
                .style(Role::KeyHint)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(rest, theme.style(Role::Text)),
    ])
    .centered()
}

/// The row under the menu's box that holds the demo's join command over `area`, when there is
/// one: it is drawn at full strength, outside the box.
pub fn join_row(area: Rect, app: &App) -> Option<Rect> {
    let command = app.session.join_hint.as_ref()?;
    let box_area = super::devices::boxed(area, WIDTH, HEIGHT);
    if box_area.bottom() >= area.bottom() {
        return None;
    }
    let width = u16::try_from(Line::from(format!(" Join with: {command} ")).width())
        .unwrap_or(u16::MAX)
        .min(area.width);
    Some(Rect::new(
        area.x + (area.width - width) / 2,
        box_area.bottom(),
        width,
        1,
    ))
}

/// Draw the menu over `area`, record its clickable cells, and answer the box it drew in.
///
/// An entry's row, from border to border, is its hit; the dimmed cells around the box are the
/// hit that closes it.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, theme: &Theme, hits: &mut HitMap) -> Rect {
    let box_area = super::devices::boxed(area, WIDTH, HEIGHT);
    widgets::surfaces::modal_ground(frame, box_area, theme);

    let indent = Span::raw(" ".repeat(MARK_INDENT));
    let mut lines = vec![Line::default()];
    for mark in wordmark::lines(Size::Small, theme) {
        let mut spans = vec![indent.clone()];
        spans.extend(mark.spans);
        lines.push(Line::from(spans));
    }
    lines.push(version_line(app, theme));
    lines.push(Line::default());
    let cursor = app.overlays.menu.unwrap_or(0);
    for (at, name) in ENTRIES.iter().enumerate() {
        lines.push(entry_line(name, at == cursor, theme));
    }
    lines.push(Line::default());

    frame.render_widget(
        Paragraph::new(lines).block(widgets::surfaces::modal_block(
            Block::bordered(),
            "",
            app,
            theme,
        )),
        box_area,
    );

    // The demo keeps the command that joins its daemon on the row under the box, raised like
    // the box over its own width only, so the dimmed screen stays on either side of it.
    if let (Some(command), Some(row)) = (&app.session.join_hint, join_row(area, app)) {
        let label = Line::from(Span::styled(
            format!(" Join with: {command} "),
            theme.style(Role::Text),
        ));
        widgets::surfaces::raise(frame, row, theme);
        frame.render_widget(Paragraph::new(label), row);
    }

    // The cells around the box close it, as four strips so none covers the box.
    let (right, bottom) = (box_area.right(), box_area.bottom());
    for strip in [
        Rect::new(area.x, area.y, area.width, box_area.y - area.y),
        Rect::new(area.x, bottom, area.width, area.bottom() - bottom),
        Rect::new(area.x, box_area.y, box_area.x - area.x, box_area.height),
        Rect::new(right, box_area.y, area.right() - right, box_area.height),
    ] {
        hits.push(strip, Hit::MenuOutside);
    }
    // Entries start on the inner row after the blank, the mark, the version and a blank.
    let first = box_area.y + 1 + 1 + 2 + 1 + 1;
    for at in 0..ENTRIES.len() {
        let row = Rect::new(box_area.x + 1, first + at as u16, box_area.width - 2, 1);
        hits.push(row, Hit::MenuEntry(at));
        if app.overlays.menu == Some(at) {
            hits.select(Selected::Menu, at, row);
        }
    }
    box_area
}

#[cfg(test)]
mod tests;
