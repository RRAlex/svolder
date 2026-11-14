// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cover art beside the Status pane's rows: where its box is, what the words give up for it,
//! and that it never covers a value.

use image::{DynamicImage, RgbImage};
use ratatui::layout::Size;
use ratatui_image::picker::Picker;

use super::*;
use crate::tui::art::Picture;
use crate::tui::art::engine::build;

/// A picture with a gradient, so every cell of it differs from the next.
fn gradient() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(96, 96, |x, y| {
        image::Rgb([(x * 2) as u8, (y * 2) as u8, 128])
    }))
}

/// The mockup's app with a cover to show: the terminal's cells are 8 by 16 pixels.
fn covered() -> App {
    let mut app = hearing(Some(track("So What")));
    app.host.cover.hash = Some("0123456789abcdef".into());
    app.host.cover.font = Some((8, 16));
    app
}

/// `app` with the art for its box in a frame of `area`, drawn as half-blocks.
fn with_art(mut app: App, area: Rect) -> App {
    let size = art_box(area, &app).expect("a box for the art");
    let protocol = build(&Picker::halfblocks(), gradient(), size).expect("a picture");
    app.host.cover.picture = Some(Picture {
        protocol,
        hash: "0123456789abcdef".into(),
        sent: Default::default(),
    });
    app
}

fn area(width: u16, height: u16) -> Rect {
    Rect::new(0, 0, width, height)
}

#[test]
fn the_box_is_the_status_panes_rows_by_the_cells_a_square_takes() {
    let app = covered();
    // 80x24: the pane's four rows, 4 * 16 px high, 8 px a column: eight columns.
    assert_eq!(
        art_box(area(80, 24), &app),
        Some(Size {
            width: 8,
            height: 4
        })
    );
    // Tall cells make a narrower box, wide ones a wider.
    let mut tall = covered();
    tall.host.cover.font = Some((10, 30));
    assert_eq!(art_box(area(80, 24), &tall).map(|s| s.width), Some(12));
    // A taller pane (the Device row and the Link row appear with graphs in the wide tier).
    let rows = art_box(area(126, 40), &app).map(|s| s.height);
    assert!(rows > Some(4), "{rows:?}");
}

#[test]
fn there_is_no_box_without_a_cover_a_font_a_now_playing_row_or_room() {
    let mut app = covered();
    app.host.cover.hash = None;
    assert_eq!(art_box(area(80, 24), &app), None, "no cover");
    let mut app = covered();
    app.host.cover.font = None;
    assert_eq!(art_box(area(80, 24), &app), None, "art off");
    let mut app = covered();
    app.host.lost_bus();
    app.host.cover.hash = Some("0123456789abcdef".into());
    assert_eq!(art_box(area(80, 24), &app), None, "no Now playing row");
    // Below the width at which the words keep their 64 columns beside the art.
    assert_eq!(art_box(area(70, 24), &covered()), None, "too narrow");
    assert_eq!(art_box(area(60, 24), &covered()), None, "narrower");
}

#[test]
fn the_art_is_drawn_at_the_right_of_the_pane_and_the_words_give_way_to_it() {
    let app = with_art(covered(), area(80, 24));
    let buffer = screen(&app);
    let rows = text(&buffer);
    // The pane's four rows are 19..=22 at 80x24: the art's 8 columns are the last of the
    // interior, against the border at 79.
    for y in 19..=22u16 {
        for x in 71..79u16 {
            let symbol = buffer.cell((x, y)).expect("a cell").symbol().to_string();
            assert!(
                symbol == "▀" || symbol == "▄",
                "({x}, {y}) is not art: {:?}",
                rows[usize::from(y)]
            );
        }
        assert_eq!(buffer.cell((79, y)).expect("the border").symbol(), "│");
    }
    // The track's words are all still there, with the player's name left where it fits.
    assert!(
        rows[22].starts_with("│ Now playing: So What — Miles Davis   mpv"),
        "{:?}",
        rows[22]
    );
    // Nothing above the pane is touched.
    let bare = text(&screen(&covered()));
    assert_eq!(rows[..18], bare[..18]);
}

#[test]
fn a_value_is_never_under_the_art() {
    // The Status pane's own two rows, the Level row and Now playing: each is fit to what the art
    // leaves, so what it says ends before the art's first column, and says all it said.
    let app = with_art(covered(), area(80, 24));
    let rows = text(&screen(&app));
    for (y, line) in rows.iter().enumerate().take(23).skip(19) {
        let row: Vec<char> = line.chars().collect();
        let words: String = row[..69].iter().collect();
        assert_eq!(
            row[69], ' ',
            "the words run up to the art on row {y}: {line:?}"
        );
        assert!(words.starts_with('│'));
    }
    assert!(
        rows[19].contains("Sample rate 48 kHz   fw 3"),
        "{:?}",
        rows[19]
    );
    assert!(rows[20].contains("Raw: 9=u8 64"), "{:?}", rows[20]);
}

#[test]
fn art_too_big_for_a_smaller_box_is_not_drawn_and_the_words_take_the_row_back() {
    // The terminal shrank from a big window to 80x24 under a picture built for the big one: it
    // does not fit the new box, so it is not drawn, and the words have their whole row until
    // the picture for the new box lands.
    let app = with_art(covered(), area(126, 40));
    let buffer = screen(&app);
    let rows = text(&buffer);
    assert!(
        rows.iter()
            .all(|row| !row.contains('▀') && !row.contains('▄')),
        "art was drawn in a box too small for it:\n{}",
        rows.join("\n")
    );
    assert_eq!(rows, text(&screen(&covered())));
    assert_eq!(art_reserved(&app), 0);
}

#[test]
fn the_picture_of_the_track_before_is_not_drawn_under_the_next() {
    let mut app = with_art(covered(), area(80, 24));
    assert!(art_slot(area(80, 24), &app).is_some());
    app.host.cover.hash = Some("fedcba9876543210".into());
    assert_eq!(art_slot(area(80, 24), &app), None);
    assert_eq!(art_reserved(&app), 0);
    let rows = text(&screen(&app));
    assert!(
        rows.iter()
            .all(|row| !row.contains('▀') && !row.contains('▄'))
    );
}

#[test]
fn the_marquee_has_the_columns_the_art_took_off_the_row() {
    let bare = covered();
    let app = with_art(covered(), area(80, 24));
    assert_eq!(art_reserved(&bare), 0);
    // The art's eight columns and the two between it and the words.
    assert_eq!(art_reserved(&app), 10);
    assert_eq!(now_playing_room((80, 24)), 64);
    assert_eq!(now_playing_room((80, 24)) - art_reserved(&app), 54);
}

#[test]
fn art_with_nothing_to_show_changes_no_row_of_the_screen() {
    // A cover named but no picture yet: the screen is the one without any art.
    let bare = text(&screen(&hearing(Some(track("So What")))));
    assert_eq!(text(&screen(&covered())), bare);
}

/// The kitty protocol sends its picture once, in the slot's top-left cell. A frame a box covers
/// writes none of it (the box's Clear would wipe it unseen); the first frame without a box does.
#[test]
fn a_kitty_picture_is_uploaded_by_the_first_frame_no_box_covers() {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    let mut app = covered();
    let size = art_box(area(80, 24), &app).expect("a box");
    let protocol = build(&picker, gradient(), size).expect("a picture");
    app.host.cover.picture = Some(Picture {
        protocol,
        hash: "0123456789abcdef".into(),
        sent: Default::default(),
    });
    let slot = art_slot(area(80, 24), &app).expect("a slot");
    let cell = |app: &App| {
        screen(app)
            .cell((slot.x, slot.y))
            .expect("the cell")
            .symbol()
            .to_string()
    };
    app.overlays.help_open = true;
    assert!(!cell(&app).contains("\x1b_G"), "uploaded under a box");
    app.overlays.help_open = false;
    assert!(
        cell(&app).contains("\x1b_G"),
        "not uploaded once the box closed"
    );
}

/// A toast or a tooltip drawn over the picture's first cell would wipe the kitty upload the
/// same way a box does: the first upload waits until nothing covers it.
#[test]
fn a_kitty_picture_is_not_uploaded_under_a_toast_or_a_tooltip() {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    let build_app = || {
        let mut app = covered();
        let size = art_box(area(80, 24), &app).expect("a box");
        let protocol = build(&picker, gradient(), size).expect("a picture");
        app.host.cover.picture = Some(Picture {
            protocol,
            hash: "0123456789abcdef".into(),
            sent: Default::default(),
        });
        app
    };
    let mut app = build_app();
    let slot = art_slot(area(80, 24), &app).expect("a slot");
    let sent = |app: &App| {
        screen(app)
            .cell((slot.x, slot.y))
            .expect("the cell")
            .symbol()
            .contains("\x1b_G")
    };
    // A toast of three lines is five rows high at the bottom right: it holds the slot's corner.
    app.notify("a long warning that wraps onto three lines of the toast box ".repeat(4));
    assert!(!sent(&app), "uploaded under a toast");
    app.tick(std::time::Instant::now() + std::time::Duration::from_secs(3600));
    assert!(app.toast().is_none());
    assert!(sent(&app), "not uploaded once the toast was gone");

    // A tooltip shown beside the picture holds it as well.
    let mut app = build_app();
    app.pointer.tooltip = Some(crate::tui::boxes::tooltip::Tooltip {
        hit: crate::tui::mouse::Hit::WebLink,
        at: (slot.x, slot.y),
        since: std::time::Instant::now(),
        shown: true,
    });
    assert!(!sent(&app), "uploaded under a tooltip");
    app.pointer.tooltip = None;
    assert!(sent(&app), "not uploaded once the tooltip was gone");
}
