// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::tui::test_support::sixteen_colour;

/// A key row measures its pieces in display cells: a wide glyph takes two, and the window cuts
/// on cell boundaries, so the line it gives is never wider than the room it was given.
#[test]
fn the_key_row_windows_by_display_cells_not_characters() {
    let pieces = [
        Piece {
            text: "世界".into(),
            role: None,
            hint: Some(0),
        },
        plain("x"),
    ];
    // "世界x" is 5 cells and 3 characters. Three cells from the start: 世 and a blank for
    // 界's cut half.
    let footer = window(&pieces, &sixteen_colour(), 0, 3);
    let text: String = footer
        .line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(Span::raw(text.as_str()).width(), 3, "{text:?}");
    assert_eq!(text, "世 ");
    // The label spans the cells it was drawn in.
    assert_eq!((footer.labels[0].x, footer.labels[0].width), (0, 3));
    // Two cells in: the second glyph whole, then x.
    let tail = window(&pieces, &sixteen_colour(), 2, 3);
    let text: String = tail.line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "界x");
}

/// A wide glyph the window's right edge cuts is drawn as a blank, so the line is exactly the room
/// it was given.
#[test]
fn a_wide_glyph_cut_at_the_windows_right_edge_is_a_blank() {
    let pieces = [plain("a"), plain("世"), plain("b")];
    let footer = window(&pieces, &sixteen_colour(), 0, 2);
    let text: String = footer
        .line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(text, "a ");
    assert_eq!(Span::raw(text.as_str()).width(), 2);
}

/// A key line wide enough to draw the full form of the setup wizard's, with room to spare.
const FULL_WIDTH: u16 = 160;

/// The wizard on a step that offers choices and has a step before it.
fn choosing() -> Footing {
    Footing::resting(keys::Scene {
        can_go_back: true,
        ..keys::Scene::default()
    })
}

/// What the wizard's key line reads as `width` cells wide.
fn footer_text(footing: &Footing, width: u16) -> String {
    footer_line(footing, &sixteen_colour(), Screen::Setup, width)
        .line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn a_key_line_with_room_names_every_key_and_what_it_does() {
    let text = footer_text(&choosing(), FULL_WIDTH);
    assert!(
        text.contains("Setup")
            && text.contains("↑↓ choose")
            && text.contains("Enter confirm")
            && text.contains("Esc back"),
        "the full key line is not there: |{text}|"
    );
}

/// A key line too narrow for its keys is cut at the screen's edge, where it rests.
#[test]
fn a_narrow_key_line_is_cut_at_the_edge() {
    let footing = choosing();
    let full = footer_text(&footing, FULL_WIDTH);
    assert!(full.contains("Enter confirm"), "|{full}|");
    let cut = footer_text(&footing, 8);
    assert_eq!(cut.chars().count(), 8, "|{cut}|");
    assert_eq!(cut, footer_text(&footing, 8), "the cut line moved");
}

/// A key line wide enough to draw the full form, and with room to spare past it, carries no
/// cell in the selected style at all: nothing on the key line is ever focused, so the pad
/// [`pad_selected`] gives a focused row elsewhere never has a key of its own to land on.
#[test]
fn a_full_key_line_has_no_cell_in_the_selected_style() {
    let footer = footer_line(
        &choosing(),
        &sixteen_colour(),
        Screen::Setup,
        FULL_WIDTH + 10,
    );
    let selected = sixteen_colour().style(Role::Selected);
    assert!(
        footer.line.spans.iter().all(|span| span.style != selected),
        "a key line carries the selected style somewhere: {:?}",
        footer
            .line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    );
}
