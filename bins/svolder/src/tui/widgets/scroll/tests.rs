// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::*;
use crate::tui::theme::Theme;

/// A window marks the side it has rows hidden on, in the margin column it is given and
/// nowhere else: `▲` on its first row's line with rows above it, `▼` on its last row's line
/// with rows below it, both at once in the middle of a long list, and nothing for a window
/// that shows every row.
#[test]
fn a_window_marks_the_sides_it_hides_rows_on() {
    let theme = Theme::new(false);
    let margin = Rect::new(9, 1, 1, 4);
    let marked = |start: usize, total: usize| {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 6));
        scroll_marks(&mut buf, margin, start, total, &theme);
        let symbols: Vec<(u16, u16, String)> = (0..6)
            .flat_map(|y| (0..10).map(move |x| (x, y)))
            .filter_map(|(x, y)| {
                let cell = buf.cell((x, y)).expect("a cell");
                (cell.symbol() != " ").then(|| (x, y, cell.symbol().to_string()))
            })
            .collect();
        symbols
    };
    let up = |y| (9, y, MORE_ABOVE.to_string());
    let down = |y| (9, y, MORE_BELOW.to_string());
    assert_eq!(marked(0, 4), Vec::new(), "a window that fits is marked");
    assert_eq!(marked(0, 10), vec![down(4)], "the top of a long list");
    assert_eq!(marked(6, 10), vec![up(1)], "the end of a long list");
    assert_eq!(
        marked(3, 10),
        vec![up(1), down(4)],
        "the middle of a long list"
    );
}
