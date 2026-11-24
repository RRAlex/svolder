// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tone screen draws, by part. The helpers here are the ones several parts use; the
//! tests are in the part files listed below.

use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use svolder_ipc::{ColorDepth, TuiConfig};
use svolder_profiles::EqSpec;
use svolder_profiles::tone::response::{F_MAX, F_MIN};
use svolder_proto::FunctionId;

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::app::{App, Intent, Screen, ToneRow};
use crate::tui::boxes::toast::ToastKind;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render;
use crate::tui::test_support::{
    assert_rows, assert_usage_md_shows, frame_with_theme, hear, plain_vo as vo, profile, text,
    theme_for,
};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

use crate::tui::screens::tone;

/// An app on the tone-control screen, in EQ mode with the loudness curve off and every
/// band flat: raw 64 is each band's own centre frequency, 98 is 0 dB and 64 is Q 4.30.
fn flat_app() -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut values = vec![vo("12.50", 2), vo("12.52", 0)];
    for band in 1..=5 {
        values.push(vo(&format!("12.{band}.2"), 64));
        values.push(vo(&format!("12.{band}.3"), 98));
        values.push(vo(&format!("12.{band}.1"), 64));
    }
    hear(&mut app, values);
    app.screen = Screen::Tone;
    app
}

/// [`flat_app`] with band 2 lifted to +6 dB, a curve the bands shape.
fn lifted_app() -> App {
    let mut app = flat_app();
    hear(&mut app, vec![vo("12.2.3", 127)]);
    app
}

/// Where on the header row the text `wanted` starts.
fn column_of(buffer: &Buffer, wanted: &str) -> u16 {
    let row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 0)).expect("a header cell").symbol())
        .collect();
    u16::try_from(
        row.find(wanted)
            .unwrap_or_else(|| panic!("no {wanted:?} in |{row}|")),
    )
    .expect("a column inside the frame")
}

/// One frame of `app` in the 80×24 terminal the mockup is drawn for.
fn screen(app: &App) -> Buffer {
    frame_with_theme(app, 80, 24, &Theme::new(false))
}

/// `app` with the tone control in `mode` (its raw value) and the volume at `volume`.
fn in_mode(mut app: App, mode: i64, volume: i64) -> App {
    hear(&mut app, vec![vo("12.50", mode), vo("10", volume)]);
    app
}

/// One frame of `app` in a terminal `width` by `height` drawn with `theme`, where it put its
/// pieces, and where it left the terminal's own cursor.
fn framed(
    app: &App,
    (width, height): (u16, u16),
    theme: &Theme,
) -> (Buffer, render::FrameAreas, (u16, u16)) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, theme, &mut hits))
        .expect("the frame draws");
    let cursor = terminal.get_cursor_position().expect("the cursor");
    (
        terminal.backend().buffer().clone(),
        areas,
        (cursor.x, cursor.y),
    )
}

/// Press each key of `keys` in turn: a character for each of `text`'s, bar the named keys.
fn keyed(app: &mut App, keys: &[KeyCode]) {
    for code in keys {
        app.key(KeyEvent::from(*code));
    }
}

/// Type `text` a character at a time.
fn type_text(app: &mut App, text: &str) {
    for letter in text.chars() {
        app.key(KeyEvent::from(KeyCode::Char(letter)));
    }
}

/// [`lifted_app`] with the imported preset `Phones` in the file and loaded with changes: the
/// bands are not what the preset fits and match no preset, so the header reads `Phones*`.
fn imported_app() -> App {
    let mut app = lifted_app();
    app.presets.saved = vec![crate::tui::test_support::presets::imported("Phones")];
    app.presets.loaded_preset = Some("Phones".into());
    app
}

/// [`imported_app`] with the bands on what the preset fits, so `Phones` is on the device.
fn fitted_app() -> App {
    let mut app = imported_app();
    let eq = app.profile.eq.clone().expect("a tone control");
    let imported = app.presets.saved[0]
        .source
        .clone()
        .expect("an imported preset");
    let fit = svolder_profiles::tone::preset::fit_for(&eq, &imported);
    let mut values = Vec::new();
    for (band, values_of) in fit.bands.iter().enumerate() {
        for (param, value) in [
            (svolder_profiles::EqParam::Frequency, values_of.f),
            (svolder_profiles::EqParam::Gain, values_of.g),
            (svolder_profiles::EqParam::Q, values_of.q),
        ] {
            let id = eq.band_id(band, param).expect("a band of the profile");
            let raw = eq.step(band, param, value).expect("a step");
            values.push(vo(&id.to_string(), i64::from(raw)));
        }
    }
    hear(&mut app, values);
    app
}

/// [`flat_app`] with the computer's controls there to flip to: three points, on.
fn computer_app() -> App {
    let mut app = flat_app();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        true,
        &[
            ("PK", 1000.0, 3.0, 1.4),
            ("LSC", 100.0, -4.0, 0.7),
            ("HSC", 8000.0, 5.0, 0.7),
        ],
    ));
    app
}

/// One frame of `app`, as rows of text.
fn rows_of(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame_with_theme(app, width, height, &Theme::new(false)))
}

/// Whether any row of `rows` holds `needle`.
fn holds(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

mod header;

mod layout;

mod response;

mod bands;

mod fit;

mod computer;

mod docs;

mod stages;
