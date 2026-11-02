// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scrolling a list that is taller than its pane: where the window starts, the words a border
//! says while rows are hidden, and the marks in the margin beside the first and last visible row.
//!
//! Every list screen picks its window with [`scroll_start`] from the selected row, so the
//! selection is always on the screen, and draws [`scroll_marks`] over what it drew.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::theme::{Role, Theme};

/// Where the visible window into `total` rows starts, keeping `selected` inside a window
/// `height` rows tall and never scrolled past the last full window.
pub fn scroll_start(total: usize, height: usize, selected: usize) -> usize {
    if height == 0 || total <= height {
        return 0;
    }
    let max_start = total - height;
    selected.saturating_sub(height - 1).min(max_start)
}

/// What a list's top border says while there are rows above the ones it shows.
pub const MORE_UP: &str = " more ↑ ";

/// What a list's bottom border says while there are rows below the ones it shows.
pub const MORE_DOWN: &str = " more ↓ ";

/// What marks the first visible row of a window with rows hidden above it.
pub const MORE_ABOVE: &str = "▲";

/// What marks the last visible row of a window with rows hidden below it.
pub const MORE_BELOW: &str = "▼";

/// Say which way a window into `total` rows, from row `start` on, has rows it isn't showing:
/// [`MORE_ABOVE`] on the first visible row's line while rows are hidden above it, [`MORE_BELOW`]
/// on the last visible row's line while rows are hidden below it, both in the dim role over the
/// text (never over the colour of a contour they sit on — the dim role alone would keep that at
/// 16 colours, which has no colour of its own).
///
/// `margin` is the one-cell-wide column the marks go in, spanning the first to last visible
/// row's lines: a screen's own right border where the rows have one, else the last column of
/// the rows' own area, always a margin and never over a value. A window showing every row is
/// left unmarked.
pub fn scroll_marks(buf: &mut Buffer, margin: Rect, start: usize, total: usize, theme: &Theme) {
    if margin.is_empty() {
        return;
    }
    let shown = usize::from(margin.height);
    let above = start > 0;
    let below = start + shown < total;
    let marks = [
        (above, margin.y, MORE_ABOVE),
        (below, margin.bottom() - 1, MORE_BELOW),
    ];
    for (hidden, y, mark) in marks {
        if let Some(cell) = buf.cell_mut((margin.x, y)).filter(|_| hidden) {
            let dim = theme.style(Role::Text).patch(theme.style(Role::Dim));
            cell.set_symbol(mark).set_style(dim);
        }
    }
}

#[cfg(test)]
mod tests;
