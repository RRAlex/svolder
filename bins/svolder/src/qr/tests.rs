// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use qrcode::{Color, EcLevel, QrCode};

use super::*;

/// The half blocks read back as the code's own modules, inside a four-module quiet zone.
#[test]
fn the_half_blocks_read_back_as_the_codes_modules() {
    let link = "http://192.168.1.20:8738/#login=c3R1Yi1saW5rLWNvZGUtMDE";
    let lines = half_blocks(link).unwrap();
    let code = QrCode::with_error_correction_level(link, EcLevel::M).unwrap();
    let (width, colors) = (code.width(), code.to_colors());
    let size = width + 8;
    assert_eq!(lines.len(), size.div_ceil(2));
    assert!(lines.iter().all(|l| l.chars().count() == size));
    assert!(lines[0].chars().all(|c| c == ' ') && lines[1].chars().all(|c| c == ' '));
    for (row, line) in lines.iter().enumerate() {
        for (x, glyph) in line.chars().enumerate() {
            let (top, bottom) = match glyph {
                '█' => (true, true),
                '▀' => (true, false),
                '▄' => (false, true),
                ' ' => (false, false),
                other => panic!("{other:?} is not a half block"),
            };
            for (y, dark) in [(2 * row, top), (2 * row + 1, bottom)] {
                let inside = (4..4 + width).contains(&x) && (4..4 + width).contains(&y);
                let want = inside && colors[(y - 4) * width + (x - 4)] == Color::Dark;
                assert_eq!(dark, want, "module ({x}, {y})");
            }
        }
    }
}

/// Painted, each line is black on white by explicit colours and ends by resetting them.
#[test]
fn painted_lines_carry_their_own_colours() {
    let painted = painted(&["▀▄".to_string()]);
    assert_eq!(painted, ["\x1b[38;2;0;0;0;48;2;255;255;255m▀▄\x1b[0m"]);
}
