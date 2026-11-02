// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::text::Span;

use super::*;
use crate::tui::test_support::sixteen_colour;
use crate::tui::theme::Role;

/// A line that fits a field is shown whole; a longer one keeps the cursor in the field,
/// with `…` for what is cut off at either end but never under the cursor.
#[test]
fn a_typed_line_longer_than_its_field_scrolls_with_the_cursor() {
    assert_eq!(typed_window("custom 1", 8, 10), ("custom 1".to_string(), 8));
    assert_eq!(typed_window("custom 1", 3, 10), ("custom 1".to_string(), 3));
    // Ten letters in ten cells leave the cursor at the end no cell of its own, so the line
    // moves one on.
    assert_eq!(
        typed_window("abcdefghij", 10, 10),
        ("…cdefghij".to_string(), 9)
    );
    let long = "abcdefghijklmnopqrstuvwxyz";
    assert_eq!(typed_window(long, 26, 10), ("…stuvwxyz".to_string(), 9));
    assert_eq!(typed_window(long, 0, 10), ("abcdefghi…".to_string(), 0));
    assert_eq!(typed_window(long, 4, 10), ("abcdefghi…".to_string(), 4));
    // The cursor is on the `p` it stands before, in the field's last cell.
    assert_eq!(typed_window(long, 15, 10), ("…hijklmnop".to_string(), 9));
    assert_eq!(typed_window(long, 5, 0), (String::new(), 0));
    // The caret's column counts display cells: a wide glyph before it takes two.
    assert_eq!(typed_window("世界", 2, 10), ("世界".to_string(), 4));
    // Cut by the window's edge, a wide glyph's place is kept as the `…` and a blank.
    assert_eq!(
        typed_window("世界世界世界", 6, 7),
        ("… 世界".to_string(), 6)
    );
    assert_eq!(
        typed_window("世界世界世界", 0, 7),
        ("世界… ".to_string(), 0)
    );
    // A wide glyph under the caret that ends the window keeps both its cells: no `…` on it.
    assert_eq!(typed_window("世界世界", 0, 2), ("世".to_string(), 0));
    assert_eq!(cut_to_cells(long, 5), "abcd…");
    assert_eq!(cut_to_cells("abc", 5), "abc");
    assert_eq!(cut_to_cells("abc", 0), "");
}

/// `pad_selected` appends one blank cell in `style` only where `spans` run short of
/// `max_width`; content already at that budget is left exactly as it is.
#[test]
fn pad_selected_appends_one_cell_only_where_there_is_room() {
    let style = sixteen_colour().style(Role::Selected);
    let mut short = vec![Span::raw("0123456789")];
    pad_selected(&mut short, style, 11);
    assert_eq!(short.len(), 2, "no cell was appended");
    assert_eq!(short[1].content, " ");
    assert_eq!(short[1].style, style);

    let mut full = vec![Span::raw("0123456789")];
    pad_selected(&mut full, style, 10);
    assert_eq!(full.len(), 1, "a row already at its budget grew");
}

/// `wrap` breaks at blanks, measures by display width, and breaks a word wider than a line
/// where the line ends, the rest joining the next word.
#[test]
fn wrap_breaks_at_blanks_by_display_width() {
    assert_eq!(wrap("", 10), Vec::<String>::new());
    assert_eq!(wrap("one  two three", 7), ["one two", "three"]);
    assert_eq!(wrap("abcdefghij kl", 4), ["abcd", "efgh", "ij", "kl"]);
    assert_eq!(wrap("abcdef g", 4), ["abcd", "ef g"]);
    // A wide glyph is two cells: two of them fill a four-cell line.
    assert_eq!(wrap("日本語 日", 4), ["日本", "語", "日"]);
}
