// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU rows of the Status pane: the row text for a needle at the ends and the middle of the
//! scale, the lamp, the rows the pane takes from a terminal of 100 by 45, and what the `Level
//! from` row adds.

use std::time::Duration;

use svolder_host::Levels;
use svolder_host::vu::Scale;

use super::super::vu::{TRACK, needle_cell, reading_text, vu_lines, zero_cell};
use super::art::covered;
use super::*;
use crate::tui::test_support::{frame, text};

/// The mockup's app on the host's side with a capture running.
fn capturing() -> App {
    let mut app = hearing(Some(track("So What")));
    app.host.capturing = true;
    app
}

/// A block of both channels at `rms_db` dBFS RMS with a sample peak of `peak`.
fn block(rms_db: f32, peak: f32) -> svolder_host::Block {
    let rms = 10f32.powf(rms_db / 20.0);
    svolder_host::Block {
        bands: [0.0; svolder_host::fft::BANDS],
        left: rms,
        right: rms,
        levels: Levels {
            rms: [rms; 2],
            peak: [peak; 2],
        },
    }
}

/// Land blocks like `block` for two seconds, 33 ms apart, on `app`'s clock, and leave the clock
/// at the last of them.
fn play(app: &mut App, rms_db: f32, peak: f32) {
    let origin = app.now;
    for step in 0..=60u32 {
        app.heard_block(
            &block(rms_db, peak),
            origin + Duration::from_millis(33 * u64::from(step)),
        );
    }
}

/// The rows of the Status pane's text in a frame `width` by `height`.
fn rows(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
}

/// The line of `rows` that holds `label`.
fn line_with<'a>(rows: &'a [String], label: &str) -> Option<&'a String> {
    rows.iter().find(|row| row.contains(label))
}

/// The lamp `line` ends its text with, whatever border follows it.
fn lamp_in(line: &str) -> char {
    line.chars()
        .rev()
        .find(|glyph| matches!(glyph, '●' | '○'))
        .expect("a lamp")
}

/// The cell of the track `line` shows the needle in.
fn needle_in(line: &str) -> usize {
    let after = line.split("VU ").nth(1).expect("a VU row");
    after
        .chars()
        .skip(2)
        .take(TRACK)
        .position(|glyph| glyph == '┃')
        .expect("a needle on the track")
}

#[test]
fn the_needle_takes_the_cell_of_its_deflection() {
    // -20 VU is the left stop, +3 VU the right end of the scale and the stop a little past it.
    assert_eq!(needle_cell(Scale::deflection(0.1)), 0);
    assert_eq!(needle_cell(Scale::deflection(1.0)), zero_cell());
    assert_eq!(zero_cell(), 27);
    assert_eq!(
        needle_cell(Scale::deflection(10f64.powf(3.0 / 20.0))),
        TRACK - 1
    );
    assert_eq!(needle_cell(Scale::STOP), TRACK - 1);
    assert_eq!(needle_cell(-3.0), 0);
    assert_eq!(needle_cell(f64::NAN.max(0.0)), 0);
}

#[test]
fn the_reading_is_one_decimal_with_its_sign_and_says_below_the_scale() {
    for (db, words) in [
        (-4.34, "-4.3"),
        (2.64, "+2.6"),
        (0.0, "+0.0"),
        (-0.04, "+0.0"),
        (-20.0, "-20.0"),
        (-20.04, "-20.0"),
        (-20.06, "< -20"),
        (f64::NEG_INFINITY, "< -20"),
        (f64::NAN, "< -20"),
        (3.2, "+3.2"),
    ] {
        assert_eq!(reading_text(db), words, "{db}");
    }
}

#[test]
fn a_level_at_the_reference_puts_the_needle_on_the_zero_mark_and_reads_zero() {
    let mut app = capturing();
    play(&mut app, -18.0, 0.5);
    let rows = rows(&app, 100, 45);
    for label in ["VU L", "VU R"] {
        let line = line_with(&rows, label).unwrap_or_else(|| panic!("{label}: {rows:#?}"));
        assert_eq!(needle_in(line), zero_cell(), "{line}");
        assert!(line.contains("┄   +0.0  ○"), "{line}");
    }
}

#[test]
fn the_track_is_dashes_below_zero_a_cross_at_zero_and_dots_above() {
    let mut app = capturing();
    play(&mut app, -40.0, 0.01);
    let rows = rows(&app, 100, 45);
    let line = line_with(&rows, "VU L").expect("the left row");
    let track: String = line
        .split("VU L ")
        .nth(1)
        .unwrap()
        .chars()
        .take(TRACK)
        .collect();
    let expected: String = (0..TRACK)
        .map(|cell| match cell {
            0 => '┃',
            _ if cell < zero_cell() => '─',
            _ if cell == zero_cell() => '┼',
            _ => '┄',
        })
        .collect();
    assert_eq!(track, expected);
    assert!(line.contains("< -20  ○"), "{line}");
}

#[test]
fn a_loud_level_pins_the_needle_at_the_right_end_and_a_full_scale_peak_lights_the_lamp() {
    let mut app = capturing();
    play(&mut app, -3.0, 1.0);
    let rows = rows(&app, 100, 45);
    for label in ["VU L", "VU R"] {
        let line = line_with(&rows, label).expect("a VU row");
        assert_eq!(needle_in(line), TRACK - 1, "{line}");
        assert_eq!(lamp_in(line), '●', "{line}");
    }
}

#[test]
fn the_lamp_goes_dark_1200_ms_after_the_last_over() {
    let mut app = capturing();
    let origin = app.now;
    app.heard_block(&block(-18.0, 1.0), origin);
    app.now = origin + Duration::from_millis(1200);
    assert_eq!(
        lamp_in(line_with(&rows(&app, 100, 45), "VU L").unwrap()),
        '●'
    );
    app.now = origin + Duration::from_millis(1201);
    assert_eq!(
        lamp_in(line_with(&rows(&app, 100, 45), "VU L").unwrap()),
        '○'
    );
}

#[test]
fn a_daemon_frame_with_a_255_peak_lights_the_lamp_on_its_channel_only() {
    let mut app = capturing();
    let frame = svolder_host::Frame {
        capture_ms: 0,
        left: [0; svolder_host::analysis::BANDS],
        right: [0; svolder_host::analysis::BANDS],
        levels: [200, 200, 255, 254],
        beat: false,
        bpm: None,
        phase: 0,
    };
    app.heard_frame(&frame, app.now);
    let rows = rows(&app, 100, 45);
    assert_eq!(lamp_in(line_with(&rows, "VU L").unwrap()), '●', "{rows:#?}");
    assert_eq!(lamp_in(line_with(&rows, "VU R").unwrap()), '○', "{rows:#?}");
}

#[test]
fn with_no_capture_the_rows_read_a_dash_a_dark_lamp_and_the_needle_at_rest() {
    let mut app = hearing(Some(track("So What")));
    play(&mut app, -18.0, 1.0);
    app.host.capture_stopped();
    let rows = rows(&app, 100, 45);
    let line = line_with(&rows, "VU L").expect("the left row");
    assert_eq!(needle_in(line), 0, "{line}");
    assert!(line.contains("—  ○"), "{line}");
}

#[test]
fn the_rows_are_there_from_100_by_45_and_nowhere_below() {
    let app = capturing();
    for (width, height, drawn) in [
        (100, 45, true),
        (160, 50, true),
        (99, 45, false),
        (100, 44, false),
        (80, 24, false),
    ] {
        let rows = rows(&app, width, height);
        assert_eq!(
            line_with(&rows, "VU L").is_some() && line_with(&rows, "VU R").is_some(),
            drawn,
            "{width}x{height}"
        );
    }
}

#[test]
fn the_rows_sit_under_now_playing_with_a_blank_row_between() {
    let app = capturing();
    let rows = rows(&app, 100, 45);
    let playing = rows
        .iter()
        .position(|row| row.contains("Now playing"))
        .expect("Now playing");
    let left = rows
        .iter()
        .position(|row| row.contains("VU L"))
        .expect("VU L");
    let right = rows
        .iter()
        .position(|row| row.contains("VU R"))
        .expect("VU R");
    assert_eq!((left, right), (playing + 2, playing + 3), "{rows:#?}");
}

#[test]
fn the_status_pane_takes_three_rows_for_the_meter_from_100_by_45() {
    let mut app = capturing();
    app.host.cover.hash = None;
    let height = |app: &App, width, rows| frames_of(Rect::new(0, 0, width, rows), app)[2].1.height;
    assert_eq!(height(&app, 100, 45), height(&app, 99, 45) + 3);
    assert_eq!(height(&app, 100, 45), height(&app, 100, 44) + 3);
    assert_eq!(height(&app, 160, 50), height(&app, 160, 44) + 3);
    // With the host's audio off there is nothing to meter, and no row.
    app.tui.host_audio = false;
    assert_eq!(height(&app, 100, 45), height(&app, 99, 45));
}

#[test]
fn the_cover_keeps_the_room_the_meter_takes_and_no_row_goes_missing() {
    let mut app = covered();
    let height = |app: &App, width| frames_of(Rect::new(0, 0, width, 45), app)[2].1.height;
    let with = rows(&app, 100, 45);
    let left = line_with(&with, "VU L").unwrap_or_else(|| panic!("{with:#?}"));
    assert!(line_with(&with, "VU R").is_some(), "{with:#?}");
    assert_eq!(lamp_in(left), '○', "{left}");
    assert!(left.contains("—  ○"), "{left}");
    // The cover only adds room: the pane is never shorter than the one with no cover.
    let mut plain = covered();
    plain.host.cover.hash = None;
    assert!(
        height(&app, 100) >= height(&plain, 100),
        "the cover took the meter's room"
    );
    assert_eq!(height(&plain, 100), height(&plain, 99) + 3);
    // With the host's audio off there are no rows either.
    app.tui.host_audio = false;
    let without = rows(&app, 100, 45);
    assert!(line_with(&without, "VU L").is_none(), "{without:#?}");
    assert!(line_with(&with, "Level from").is_some(), "{with:#?}");
}

#[test]
fn the_level_from_row_says_where_zero_vu_is_while_the_rows_are_drawn() {
    let mut app = capturing();
    play(&mut app, -18.0, 0.5);
    let on = rows(&app, 100, 45);
    assert!(
        line_with(&on, "Level from")
            .unwrap()
            .contains("(0 VU = -18 dBFS)"),
        "{on:#?}"
    );
    let off = rows(&app, 99, 45);
    assert!(
        !line_with(&off, "Level from").unwrap().contains("VU"),
        "{off:#?}"
    );
    app.tui.vu_reference = -17;
    assert!(
        line_with(&rows(&app, 100, 45), "Level from")
            .unwrap()
            .contains("(0 VU = -17 dBFS)")
    );
}

#[test]
fn a_new_reference_moves_the_reading_by_the_same_decibels() {
    let mut app = capturing();
    app.tui.vu_reference = -24;
    play(&mut app, -30.0, 0.5);
    let rows = rows(&app, 100, 45);
    assert!(
        line_with(&rows, "VU L").unwrap().contains("-6.0  ○"),
        "{rows:#?}"
    );
}

#[test]
fn a_monochrome_theme_keeps_the_glyphs_and_tells_the_lamp_by_its_shape() {
    use ratatui::text::Line;
    use svolder_ipc::{ColorDepth, TuiConfig};
    let theme =
        crate::tui::theme::Theme::from_config(&TuiConfig::default(), true, ColorDepth::Ansi16);
    let mut app = capturing();
    play(&mut app, -12.0, 1.0);
    let flat = |line: &Line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    let lit = flat(&vu_lines(&app, &theme)[1]);
    assert!(
        lit.contains('┃') && lit.contains('┼') && lit.contains('┄') && lit.contains('─'),
        "{lit}"
    );
    assert!(lit.ends_with('●'), "{lit}");
    app.now += Duration::from_millis(2000);
    assert!(flat(&vu_lines(&app, &theme)[1]).ends_with('○'));
    assert_eq!(vu_lines(&app, &theme).len(), 3);
    assert!(vu_lines(&app, &theme)[0].spans.is_empty());
}

#[test]
fn the_capture_runs_for_the_rows_alone_from_100_by_45_and_not_below() {
    let mut app = capturing();
    app.tui.title_meter = false;
    app.breathing = false;
    app.effects = false;
    app.host.sink = Some(svolder_host::SinkState {
        volume: 0.5,
        muted: false,
        name: "USB DAC".into(),
        target: Some("alsa_output.usb".into()),
    });
    app.size = (99, 45);
    assert_eq!(app.capture_target(), None);
    assert!(!app.vu_moving());
    app.size = (100, 45);
    assert_eq!(app.capture_target(), Some("alsa_output.usb"));
    assert!(!app.vu_moving(), "a meter at rest is still");
    play(&mut app, -18.0, 0.5);
    assert!(app.vu_moving());
    app.host.capturing = false;
    assert!(!app.vu_moving(), "no capture, no redraw pace");
    app.host.capturing = true;
    assert!(app.vu_moving());
    app.screen = Screen::Tone;
    assert_eq!(app.capture_target(), None, "the rows are not on the screen");
    // Under any box over the device screen the capture is not held for them, as the Spectrum's is
    // not: the real Settings box, the help overlay.
    app.screen = Screen::Device;
    assert_eq!(app.capture_target(), Some("alsa_output.usb"));
    app.overlays.settings = Some(crate::tui::app::SettingsBox::default());
    assert_eq!(app.capture_target(), None, "under the Settings box");
    assert!(!app.vu_moving());
    app.overlays.settings = None;
    app.overlays.help_open = true;
    assert_eq!(app.capture_target(), None, "under the help overlay");
    app.overlays.help_open = false;
    assert_eq!(app.capture_target(), Some("alsa_output.usb"));
}

#[test]
fn the_two_rows_are_one_target_that_says_what_the_meter_is() {
    use crate::tui::mouse::Hit;
    let mut app = capturing();
    app.tui.vu_reference = -20;
    let (buffer, hits) = crate::tui::test_support::drawn(&app, 100, 45);
    let rows = text(&buffer);
    let top = rows
        .iter()
        .position(|row| row.contains("VU L"))
        .expect("VU L") as u16;
    for (x, y, want) in [
        (10, top, Some(Hit::Vu)),
        (10, top + 1, Some(Hit::Vu)),
        (10, top - 1, None),
        (10, top + 2, None),
    ] {
        assert_eq!(
            hits.hit_at(x, y).copied().filter(|hit| *hit == Hit::Vu),
            want,
            "{x},{y}"
        );
    }
    // Below 100 by 45 there are no rows and no target.
    let (_, hits) = crate::tui::test_support::drawn(&app, 99, 45);
    assert!(hits.area_of(Hit::Vu).is_none());
}
