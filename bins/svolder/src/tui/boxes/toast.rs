// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The toast: what the last thing asked for came to, in a small bordered box over the bottom
//! right corner of the screen.
//!
//! One toast shows at a time. It is drawn over everything, boxes included, and is never dimmed
//! or animated; keys go to the screen while it shows, and a click on it dismisses it.

use std::time::Duration;

use ratatui::Frame;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::layout_tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

/// The most lines a toast wraps onto.
const MAX_LINES: usize = 3;
/// The cells a toast adds round its text: a border and one blank cell on each side.
const FRAME_WIDTH: u16 = 4;
/// How many cells clear of the terminal's right edge the toast's right border sits.
const RIGHT_MARGIN: u16 = 2;

/// What a toast reports, which decides its border and how long it stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// A plain notice: a border in the accent, three seconds.
    Info,
    /// A refusal, an error or a warning: a border in the warn role, six seconds.
    Failure,
    /// A wait in progress: a dim border, until the wait stops saying it.
    Wait,
}

impl ToastKind {
    /// How long a toast of this kind stays after it was set, or after a wait last said it.
    pub fn lifetime(self) -> Duration {
        match self {
            ToastKind::Info => Duration::from_secs(3),
            ToastKind::Failure => Duration::from_secs(6),
            ToastKind::Wait => Duration::from_millis(500),
        }
    }

    /// The role the border is drawn in.
    fn border(self) -> Role {
        match self {
            ToastKind::Info => Role::Cursor,
            ToastKind::Failure => Role::Warn,
            ToastKind::Wait => Role::Dim,
        }
    }
}

/// One toast: the words and the kind they are said in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// What the toast says.
    pub text: String,
    /// What it reports.
    pub kind: ToastKind,
    /// How long it stays when that is not its kind's own lifetime.
    pub lasts: Option<Duration>,
}

impl Toast {
    /// How long this toast stays after it was set: its own lifetime, else its kind's.
    pub fn lifetime(&self) -> Duration {
        self.lasts.unwrap_or_else(|| self.kind.lifetime())
    }
}

/// Where a toast saying `text` sits on a screen whose body is `screen`, and the lines it wraps to.
///
/// The box is its lines plus two border rows, with its bottom border on the body's last row
/// and its right border two cells in from the body's right edge, which is the terminal's below
/// the wide tier. It is as wide as the longest line plus four cells, at most 60% of the body's
/// width. Text wider than that wraps at spaces
/// onto up to three lines, and the third ends in `…` if it still does not fit.
pub fn area(screen: Rect, text: &str) -> (Rect, Vec<String>) {
    let cap = u16::try_from(u32::from(screen.width) * 6 / 10).unwrap_or(u16::MAX);
    let room = usize::from(cap.saturating_sub(FRAME_WIDTH).max(1));
    let lines = wrap(text, room);
    let widest = lines
        .iter()
        .map(|line| widgets::text::cells(line))
        .max()
        .unwrap_or(0);
    let wide = u16::try_from(widest)
        .unwrap_or(u16::MAX)
        .saturating_add(FRAME_WIDTH)
        .min(screen.width);
    let high = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(screen.height);
    let right = screen.right().saturating_sub(RIGHT_MARGIN);
    let rect = Rect {
        x: right.saturating_sub(wide).max(screen.x),
        y: screen.bottom() - high,
        width: wide,
        height: high,
    };
    (rect, lines)
}

/// `text` broken at spaces into lines of at most `room` cells, at most [`MAX_LINES`] of them,
/// the last cut short with an ellipsis where text is left over. A word wider than a line is
/// broken across lines.
fn wrap(text: &str, room: usize) -> Vec<String> {
    let mut lines = widgets::text::wrap(text, room);
    if lines.len() > MAX_LINES {
        let rest = lines.split_off(MAX_LINES - 1).join(" ");
        lines.push(widgets::text::cut_to_cells(&rest, room));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Draw `toast` over the bottom right corner of `screen` and record it as a mouse target.
///
/// Called last of all, so it covers whatever is under it, a modal box included, in the theme's
/// own colours, and its hit is the last pushed, which a click finds first. Returns the box.
pub fn draw(
    frame: &mut Frame,
    screen: Rect,
    toast: &Toast,
    theme: &Theme,
    hits: &mut HitMap,
) -> Rect {
    let (_, [_, body]) = layout_tier::split(screen);
    let (rect, lines) = area(body, &toast.text);
    widgets::surfaces::raise(frame, rect, theme);
    let block = Block::bordered().border_style(theme.style(toast.kind.border()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let text: Vec<Line> = lines
        .into_iter()
        .map(|line| Line::from(Span::styled(format!(" {line}"), theme.style(Role::Text))))
        .collect();
    frame.render_widget(Paragraph::new(text), inner);
    hits.push(rect, Hit::Toast);
    rect
}

/// The cells of a toast, taken out of a frame to be put back after the animation layer has
/// painted over it.
pub struct Lifted {
    area: Rect,
    cells: Vec<Cell>,
}

/// Copy the cells of `area` out of `buffer`.
pub fn lift(buffer: &Buffer, area: Rect) -> Lifted {
    let cells = (area.top()..area.bottom())
        .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
        .map(|at| buffer.cell(at).cloned().unwrap_or_default())
        .collect();
    Lifted { area, cells }
}

impl Lifted {
    /// Write the cells back where they were taken from.
    pub fn put_back(self, buffer: &mut Buffer) {
        let places = (self.area.top()..self.area.bottom())
            .flat_map(|y| (self.area.left()..self.area.right()).map(move |x| (x, y)));
        for (at, cell) in places.zip(self.cells) {
            if let Some(slot) = buffer.cell_mut(at) {
                *slot = cell;
            }
        }
    }
}

#[cfg(test)]
mod tests;
