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
use crate::tui::screens::device::art::{GAP, MIN_COLS, MIN_WORDS};
use crate::tui::screens::device::rows_of;

/// A picture with a gradient, so every cell of it differs from the next.
fn gradient() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(96, 96, |x, y| {
        image::Rgb([(x * 2) as u8, (y * 2) as u8, 128])
    }))
}

/// The mockup's app with a cover to show: the terminal's cells are 8 by 16 pixels.
pub(super) fn covered() -> App {
    let mut app = hearing(Some(track("So What")));
    app.host.cover.hash = Some("0123456789abcdef".into());
    app.host.cover.font = Some((8, 16));
    app
}

/// `app` with the art for its box in a frame of `area`, drawn as half-blocks.
pub(super) fn with_art(mut app: App, area: Rect) -> App {
    let size = art_box(area, &app).expect("a box for the art");
    let protocol = build(&Picker::halfblocks(), gradient(), size).expect("a picture");
    app.host.cover.picture = Some(Picture {
        protocol,
        hash: "0123456789abcdef".into(),
        sent: Default::default(),
    });
    app
}

/// `app` with its cover decoded to `width` by `height` pixels.
fn shaped(mut app: App, width: u32, height: u32) -> App {
    app.host.cover.extent = Some(("0123456789abcdef".into(), width, height));
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

/// The heights of the panes (Controls, Tone, Status) in a frame `area`, and the cover's box.
fn heights(app: &App, area: Rect) -> ([u16; 3], Option<Size>) {
    let frames = frames_of(area, app);
    let tone = if frames.len() == 3 {
        frames[1].1.height
    } else {
        0
    };
    let status = frames[frames.len() - 1].1.height;
    let heights = [frames[0].1.height, tone, status];
    (heights, art_box(area, app))
}

/// The lines of `rows` (a frame as text) inside `pane`, blank ones left out.
fn lines_in(rows: &[String], pane: Rect) -> Vec<String> {
    rows.iter()
        .skip(usize::from(pane.y))
        .take(usize::from(pane.height))
        .map(|row| {
            let inside: String = row
                .chars()
                .skip(usize::from(pane.x) + 1)
                .take(usize::from(pane.width).saturating_sub(2))
                .collect();
            inside.trim().to_string()
        })
        .filter(|line| !line.is_empty() && !line.starts_with('─') && !line.starts_with('┌'))
        .collect()
}

/// `app` with a cover named and the terminal's cells 8 by 16 pixels.
fn with_cover(mut app: App) -> App {
    app.host.cover.hash = Some("0123456789abcdef".into());
    app.host.cover.font = Some((8, 16));
    app
}

/// A Kraken on a host, playing, capturing, with the Host group and its Transport and Spectrum
/// rows: the busiest Controls pane of the built-in profiles.
fn kraken_playing() -> App {
    let mut app = app_on(crate::tui::test_support::profile_named("kraken"));
    app.host.linked = true;
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "Kraken Analog Stereo".into(),
            target: Some("alsa_output.usb-Kraken-00.analog-stereo".into()),
        }),
        true,
    );
    app.host.found_bus(Some(track("So What")), app.now);
    app.host.capturing = true;
    app
}

/// The Gungnir on the computer's side: a sink with a target (the path row), capture on, and the
/// computer's EQ with crossfeed (the Tone pane's Crossfeed row), playing a track.
fn computer_side_playing() -> App {
    let mut app = super::host_group::daemon_hosting(super::host_group::gungnir_route(|_| {}));
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_crossfeed(
        true, "bauer", None,
    ));
    app.flip_side();
    assert!(app.on_computer());
    app
}

/// The Status pane is as tall as its rows, with one blank row at its end in the tall tier only,
/// whatever the cover and the terminal: the cover is as tall as the rows, the pane is the same
/// with a cover, with none and with a failed one, and the rows it does not need are the Controls
/// and Tone panes'.
#[test]
fn the_status_pane_is_its_rows_and_in_the_tall_tier_a_blank_row() {
    for (width, height) in [
        (80, 24),
        (119, 39),
        (100, 44),
        (100, 45),
        (160, 50),
        (200, 60),
        (160, 70),
    ] {
        let area = area(width, height);
        let with_cover = covered();
        let mut bare = covered();
        bare.host.cover.hash = None;
        let mut failed = covered();
        failed.host.cover.failed = failed.host.cover.hash.clone();
        let (with, with_box) = heights(&with_cover, area);
        assert_eq!(
            heights(&bare, area).0,
            with,
            "{area:?}: a cover moves a pane"
        );
        assert_eq!(heights(&failed, area).0, with, "{area:?}: a failed cover");
        let tier = crate::tui::layout_tier::Tier::of(area);
        let status = super::super::rows_of(area, &with_cover).1[2];
        let rows = text(&frame(&with_cover, width, height));
        let inside = |y: u16| rows[usize::from(y)].trim_matches(['│', ' ']).to_string();
        let last = status.y + status.height - 1;
        let blank = tier.end_blank();
        assert_eq!(blank, u16::from(height >= 40), "{area:?}");
        if blank == 1 {
            assert!(
                inside(last - 1).is_empty(),
                "{area:?}: no blank row at the end"
            );
        }
        assert!(
            !inside(last - 1 - blank).is_empty(),
            "{area:?}: the last row is a row"
        );
        let lines = status.height - 2 - tier.top() - blank;
        assert_eq!(
            with_box.map(|size| size.height),
            Some(lines),
            "{area:?}: the cover is the rows"
        );
        let body = height - super::super::header_height(width);
        assert_eq!(with[0] + tier.row_gap() + with[2], body, "{area:?}");
    }
    assert_eq!(
        heights(&covered(), area(200, 60)),
        (
            [44, 44, 12],
            Some(Size {
                width: 16,
                height: 8
            })
        )
    );
    assert_eq!(
        heights(&covered(), area(100, 45)),
        (
            [29, 29, 12],
            Some(Size {
                width: 16,
                height: 8
            })
        )
    );
    assert_eq!(
        heights(&covered(), area(100, 44)),
        (
            [31, 31, 9],
            Some(Size {
                width: 10,
                height: 5
            })
        )
    );
}

/// The rows the Status pane no longer takes for a cover stay with the panes above: every
/// non-blank line they draw without a cover they draw with one, and the Kraken's Host group,
/// the computer side's path and the Crossfeed row all show on a big terminal.
#[test]
fn a_cover_costs_the_panes_above_no_row() {
    for (width, height) in [(100, 45), (160, 50), (160, 70)] {
        let area = area(width, height);
        for make in [kraken_playing, computer_side_playing, host_mockup_app] {
            let bare = make();
            let app = with_cover(make());
            let base = frames_of(area, &bare);
            let covered = frames_of(area, &app);
            assert_eq!(base, covered, "{area:?}: a cover moved a pane");
            let before = text(&frame(&bare, area.width, area.height));
            let after = text(&frame(&app, area.width, area.height));
            for ((_, was), (_, now)) in base.iter().zip(&covered).take(base.len() - 1) {
                assert_eq!(lines_in(&before, *was), lines_in(&after, *now), "{area:?}");
            }
        }
    }
    let kraken = with_cover(kraken_playing());
    let rows = text(&frame(&kraken, 160, 50));
    let controls = lines_in(&rows, frames_of(area(160, 50), &kraken)[0].1);
    for word in ["Host", "Transport", "Spectrum"] {
        assert!(
            controls.iter().any(|line| line.contains(word)),
            "{word}: {controls:#?}"
        );
    }
    let computer = with_cover(computer_side_playing());
    let rows = text(&frame(&computer, 160, 50));
    let frames = frames_of(area(160, 50), &computer);
    let controls = lines_in(&rows, frames[0].1);
    assert!(
        controls.iter().any(|line| line.starts_with("Path")),
        "{controls:#?}"
    );
    let tone = lines_in(&rows, frames[1].1);
    assert!(
        tone.iter().any(|line| line.contains("Crossfeed")),
        "{tone:#?}"
    );
}

#[test]
fn a_cover_that_failed_keeps_the_usual_layout() {
    let mut app = with_cover(kraken_playing());
    let with = heights(&app, area(160, 50)).0;
    app.host.cover.failed = app.host.cover.hash.clone();
    let failed = heights(&app, area(160, 50));
    assert_eq!(failed.1, None, "no cover box for a failed cover");
    app.host.cover.hash = None;
    assert_eq!(failed.0, heights(&app, area(160, 50)).0);
    assert_eq!(with, failed.0, "{with:?} {failed:?}");
    app.host.cover.hash = Some("fedcba9876543210".into());
    assert_eq!(heights(&app, area(160, 50)).0, with);
}

#[test]
fn the_status_pane_does_not_change_with_no_cover_to_show() {
    let mut app = covered();
    app.host.cover.hash = None;
    let plain = heights(&app, area(100, 45)).0;
    // The pane's usual eight rows, the VU meter's three, and the blank row at its end.
    assert_eq!(plain[2], 8 + 3 + 1);
    let mut app = covered();
    app.host.cover.font = None;
    assert_eq!(heights(&app, area(160, 50)).0[2], 8 + 3 + 1);
}

/// A toast that meets the picture anywhere holds the kitty upload, not only one that covers its
/// first cell: a one-line toast at the right edge of 80×24 overlaps the slot's last rows and
/// columns but not its corner, and clears cells the picture is sent in.
#[test]
fn a_kitty_picture_is_held_by_a_toast_that_misses_its_corner() {
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
    app.notify("ab");
    let (_, [_, body]) = crate::tui::layout_tier::split(area(80, 24));
    let toast = crate::tui::boxes::toast::area(body, "ab").0;
    assert!(toast.intersects(slot), "{toast:?} {slot:?}");
    assert!(
        !toast.contains(ratatui::layout::Position::new(slot.x, slot.y)),
        "the corner is clear"
    );
    let sent = |app: &App| {
        screen(app)
            .cell((slot.x, slot.y))
            .expect("the cell")
            .symbol()
            .contains("\x1b_G")
    };
    assert!(!sent(&app), "uploaded under a toast that meets the slot");
    app.tick(std::time::Instant::now() + std::time::Duration::from_secs(3600));
    assert!(app.toast().is_none());
    assert!(sent(&app), "not uploaded once the toast was gone");
}

/// The box is as tall as the pane's rows whatever the cover, as wide as the cover's own shape
/// makes it at that height in the terminal's cells, and never leaves the words under 64 columns.
#[test]
fn the_box_takes_the_covers_own_shape_and_the_words_keep_their_columns() {
    // (width, height) in pixels: a 16:9 thumbnail, a square cover, a 2:3 portrait.
    for (w, h) in [(980u32, 551u32), (600, 600), (400, 600)] {
        for (cols, rows) in [(200, 60), (100, 45), (120, 40)] {
            let a = area(cols, rows);
            let app = shaped(covered(), w, h);
            let square = art_box(a, &covered()).expect("a square box");
            let size = art_box(a, &app).unwrap_or_else(|| panic!("a box at {cols}x{rows} {w}x{h}"));
            assert_eq!(size.height, square.height, "{cols}x{rows} {w}x{h}: height");
            let (tier, [_, _, status]) = rows_of(a, &app);
            let inner = tier.block().inner(status);
            let trail = u16::try_from(tier.trail()).unwrap();
            let room = inner.width - trail - GAP - MIN_WORDS;
            // Cells are 8 by 16 pixels.
            let wanted = (u32::from(size.height) * 16 * w).div_ceil(8 * h);
            assert_eq!(
                u32::from(size.width),
                wanted.min(u32::from(room)),
                "{cols}x{rows} {w}x{h}: width"
            );
            assert!(inner.width - trail - GAP - size.width >= MIN_WORDS);
            if w == h {
                assert_eq!(size, square, "a square cover keeps today's box");
            }
        }
    }
    // 200x60 is roomy: the wide box is really wider than the square one.
    let wide = art_box(area(200, 60), &shaped(covered(), 980, 551)).unwrap();
    let square = art_box(area(200, 60), &covered()).unwrap();
    assert!(wide.width > square.width, "{wide:?} {square:?}");
    let tall = art_box(area(200, 60), &shaped(covered(), 400, 600)).unwrap();
    assert!(tall.width < square.width, "{tall:?} {square:?}");
}

/// A cover so tall that its shape would be narrower than the smallest box keeps the smallest box
/// and is cropped, rather than vanishing where a square cover shows.
#[test]
fn a_very_tall_cover_keeps_the_smallest_box() {
    let square = art_box(area(80, 24), &covered()).expect("a square box at 80x24");
    let tall = art_box(area(80, 24), &shaped(covered(), 100, 600))
        .expect("a 1:6 cover still shows at 80x24");
    assert_eq!(tall.height, square.height);
    assert_eq!(tall.width, MIN_COLS, "{tall:?}");
}

/// A cover of another track's size does not shape this track's box.
#[test]
fn the_shape_of_another_cover_is_not_used() {
    let mut app = shaped(covered(), 980, 551);
    app.host.cover.hash = Some("fedcba9876543210".into());
    assert_eq!(
        art_box(area(200, 60), &app),
        art_box(area(200, 60), &covered())
    );
}

/// Where the words' columns cap the box, the picture is cut from the centre to the box and is
/// as tall as the rows: the leftmost and rightmost cells show the picture's pixels as far in
/// from each edge.
#[test]
fn a_wide_cover_in_a_capped_box_is_cropped_from_the_centre() {
    let mut app = shaped(covered(), 320, 180);
    let a = area(80, 24);
    let size = art_box(a, &app).expect("a box");
    let uncapped = (u32::from(size.height) * 16 * 320).div_ceil(8 * 180);
    assert!(u32::from(size.width) < uncapped, "the words cap the box");
    let image = DynamicImage::ImageRgb8(RgbImage::from_fn(320, 180, |x, _| {
        image::Rgb([(x * 255 / 319) as u8, 0, 0])
    }));
    let protocol = build(&Picker::halfblocks(), image, size).expect("a picture");
    assert_eq!(protocol.size(), size, "the picture fills its box");
    app.host.cover.picture = Some(Picture {
        protocol,
        hash: "0123456789abcdef".into(),
        sent: Default::default(),
    });
    let slot = art_slot(a, &app).expect("a slot");
    assert_eq!(slot.height, size.height);
    let buffer = screen(&app);
    let red = |x: u16| match buffer.cell((x, slot.y)).expect("a cell").fg {
        ratatui::style::Color::Rgb(r, ..) => i32::from(r),
        other => panic!("not a picture cell: {other:?}"),
    };
    let (left, right) = (red(slot.x), red(slot.right() - 1));
    assert!(left > 10, "left {left}: not cut from the centre");
    assert!(right < 245, "right {right}: not cut from the centre");
    assert!(
        (left + right - 255).abs() < 20,
        "{left} {right}: off centre"
    );
}

/// The wide box of a 16:9 cover is held by a toast or a tooltip over any part of it, as the
/// square one is: the kitty upload waits until nothing covers the slot.
#[test]
fn a_kitty_picture_in_a_wide_slot_is_held_by_a_toast_and_a_tooltip() {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    let a = area(200, 60);
    let build_app = || {
        let mut app = shaped(covered(), 980, 551);
        let size = art_box(a, &app).expect("a box");
        let image = DynamicImage::ImageRgb8(RgbImage::new(980, 551));
        let protocol = build(&picker, image, size).expect("a picture");
        app.host.cover.picture = Some(Picture {
            protocol,
            hash: "0123456789abcdef".into(),
            sent: Default::default(),
        });
        app
    };
    let mut app = build_app();
    let slot = art_slot(a, &app).expect("a slot");
    let square = art_box(a, &covered()).expect("a box");
    assert!(slot.width > square.width, "the slot is wide: {slot:?}");
    let sent = |app: &App| {
        frame(app, a.width, a.height)
            .cell((slot.x, slot.y))
            .expect("the cell")
            .symbol()
            .contains("\x1b_G")
    };
    app.notify("ab");
    let (_, [_, body]) = crate::tui::layout_tier::split(a);
    let toast = crate::tui::boxes::toast::area(body, "ab").0;
    assert!(toast.intersects(slot), "{toast:?} {slot:?}");
    assert!(
        !sent(&app),
        "uploaded under a toast that meets the wide slot"
    );
    app.tick(std::time::Instant::now() + std::time::Duration::from_secs(3600));
    assert!(sent(&app), "not uploaded once the toast was gone");

    let mut app = build_app();
    app.pointer.tooltip = Some(crate::tui::boxes::tooltip::Tooltip {
        hit: crate::tui::mouse::Hit::WebLink,
        at: (slot.right() - 1, slot.y),
        since: std::time::Instant::now(),
        shown: true,
    });
    assert!(
        !sent(&app),
        "uploaded under a tooltip at the wide slot's far edge"
    );
}
