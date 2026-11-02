// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::*;

const URL: &str = "http://192.168.1.20:8738/#login=abcdef";

/// A buffer whose second and third rows hold `URL` in two lines of 20 and 18 characters.
fn drawn() -> (Buffer, Vec<Rect>) {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 5));
    let style = Style::new().add_modifier(Modifier::UNDERLINED);
    buffer.set_string(1, 1, &URL[..20], style);
    buffer.set_string(1, 2, &URL[20..], style);
    (buffer, vec![Rect::new(1, 1, 20, 1), Rect::new(1, 2, 18, 1)])
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("text")
}

/// Every line of the link opens the same link (one `id=`) before its cells and closes it after
/// them, in the cells' own style, and the cursor is saved and restored around the lot.
#[test]
fn each_line_is_wrapped_in_one_link_with_one_id() {
    let (buffer, lines) = drawn();
    let out = text(&around(&buffer, &lines, URL));
    let open = format!("\x1b]8;id={ID};{URL}\x1b\\");
    assert_eq!(out.matches(&open).count(), 2, "{out:?}");
    assert_eq!(out.matches("\x1b]8;;\x1b\\").count(), 2, "{out:?}");
    assert!(
        out.starts_with("\x1b7") && out.ends_with("\x1b8"),
        "{out:?}"
    );
    let first = out.find(&open).unwrap();
    let close = out.find("\x1b]8;;\x1b\\").unwrap();
    let between = &out[first + open.len()..close];
    assert!(between.contains(&URL[..20]), "{between:?}");
    assert!(!between.contains(&URL[20..]), "{between:?}");
    assert!(
        between.contains("\x1b[4m"),
        "the underline is kept: {between:?}"
    );
    // Every open has its close before the next open: nothing after a line is a link.
    let second = out[first + open.len()..].find(&open).unwrap() + first + open.len();
    assert!(close < second, "{out:?}");
}

/// Cells that do not read as the link, a link with a control character in it, and no lines at
/// all write nothing: a terminal is never told a link that is not what is drawn.
#[test]
fn nothing_is_written_unless_the_cells_read_as_the_link() {
    let (mut buffer, lines) = drawn();
    assert!(around(&buffer, &lines, "http://other.example/").is_empty());
    assert!(around(&buffer, &[], URL).is_empty());
    buffer.set_string(5, 1, "XX", Style::new());
    assert!(around(&buffer, &lines, URL).is_empty(), "drawn over");
    let (buffer, lines) = drawn();
    let evil = format!("{URL}\x1b]52;c;AAAA\x07");
    assert!(around(&buffer, &lines, &evil).is_empty());
}
