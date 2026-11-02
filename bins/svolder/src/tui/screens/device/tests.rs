// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the device screen draws, by topic. The helpers here are the ones several topics use; the
//! tests are in the topic files listed below.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::text::{Line, Span};
use serde_json::json;
use svolder_ipc::{ColorDepth, TuiConfig, ValueObject};
use svolder_profiles::{Control, ControlKind, Profile, Registry};
use svolder_proto::ValueType;

use crate::backend::Update;
use crate::recovery::{Failure, Kind};
use crate::tui::app::{App, Pane, Screen, ToneRow};
use crate::tui::decoration::effects::Effects;
use crate::tui::decoration::wordmark::{self, Size};
use crate::tui::host;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render;
use crate::tui::test_support::{
    AEGIR_SCALE, animated, assert_rows, frame, frame_with_theme, hear, meter_reading,
    profile_named, text, theme_for, tyr_following_its_floor, vo, with_log_scale, with_overcurrent,
    with_range_on, without_scale,
};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;
use crate::tui::widgets::rows::BALANCE_MARK;

use super::{
    CONTROLS_WIDTH, cell_window, cells, cut_cells, fault_line, frames_of, header_height, logo_of,
    now_playing_room, path_row, row_style, tone_lines,
};

// the header row
mod header;
// the Controls pane's rows and hits
mod controls;
// volume, balance and loudness bars
mod bars;
// the Tone pane
mod tone_pane;
// the Status pane
mod status;
// the Host group
mod host_group;
// meter rows
mod meters;
// tiers, sizes, panes and borders
mod layout;
// effects on the device screen
mod effects;
// the tab strip
mod tab_strip;
// the connection strip and the level history
mod graphs;

/// The column every control row's value starts in on the mockup screen: the pane border,
/// the row cursor and the label column of its longest label.
const VALUE_COLUMN: u16 = 17;

/// One reading of a control that converts its raw value to human units.
fn converted(id: &str, key: &str, raw: i64, display: &str, value: f64, unit: &str) -> ValueObject {
    ValueObject {
        value: Some(value),
        unit: Some(unit.into()),
        ..vo(id, Some(key), json!(raw), display)
    }
}

/// An app on `profile`, with the default `[tui]` settings and no device list.
fn app_on(profile: Profile) -> App {
    App::new(profile, "gungnir".into(), &TuiConfig::default(), Vec::new())
}

/// The device-screen mockup's app: a Gungnir 2 on input 3, phase off, the
/// NOS filter on, unmuted, at -6 dB, centred, with five EQ bands set and three raw
/// readings no control names.
fn mockup_app() -> App {
    mockup_on(profile_named("gungnir2-v2"), false)
}

/// The mockup's app on a profile that has an over-current fault, which reads clear.
fn faulty_mockup_app() -> App {
    mockup_on(with_overcurrent(profile_named("gungnir2-v2")), true)
}

/// The mockup's readings on `profile`, with the fault reading when `fault` is set.
fn mockup_on(profile: Profile, fault: bool) -> App {
    let mut app = app_on(profile);
    hear(
        &mut app,
        vec![
            vo("2", Some("input"), json!(3), "3 (Optical)"),
            vo("3", Some("phase"), json!(false), "off"),
            vo("4", Some("nos"), json!(true), "on"),
            vo("5", Some("mute"), json!(false), "off"),
            vo("6", Some("sample_rate"), json!(2), "2 (48 kHz)"),
            converted("10", "volume", 109, "109 (-6.0 dB)", -6.0, "dB"),
            vo("11", Some("balance"), json!(64), "64"),
            vo("210.2.1", None, json!(3), "3"),
            vo("9.1", None, json!(0), "0"),
            vo("9.2", None, json!(1), "1"),
            vo("9", None, json!(64), "64"),
            vo("12.50", None, json!(2), "2"),
            vo("12.52", None, json!(0), "0"),
            // Band frequency, gain and Q raws: 64 is each band's own centre frequency,
            // 98 is 0 dB and 64 is Q 4.3.
            vo("12.1.2", None, json!(64), "64"),
            vo("12.1.3", None, json!(98), "98"),
            vo("12.1.1", None, json!(64), "64"),
            vo("12.2.2", None, json!(64), "64"),
            vo("12.2.3", None, json!(86), "86"),
            vo("12.2.1", None, json!(27), "27"),
            vo("12.3.2", None, json!(64), "64"),
            vo("12.3.3", None, json!(98), "98"),
            vo("12.3.1", None, json!(64), "64"),
            vo("12.4.2", None, json!(64), "64"),
            vo("12.4.3", None, json!(105), "105"),
            vo("12.4.1", None, json!(8), "8"),
            vo("12.5.2", None, json!(64), "64"),
            vo("12.5.3", None, json!(98), "98"),
            vo("12.5.1", None, json!(64), "64"),
        ],
    );
    if fault {
        hear(
            &mut app,
            vec![vo(
                "230.1",
                Some("fault_overcurrent"),
                json!(false),
                "clear",
            )],
        );
    }
    app.session.unit = Some("0102…08".into());
    app.clock = "14:02".into();
    app
}

/// One frame of `app` in the 80×24 terminal the mockup is drawn for.
fn screen(app: &App) -> Buffer {
    frame(app, 80, 24)
}

/// The mockup's app on a host whose default sink is the Gungnir, at 54 %, with mpv playing
/// So What and the sink's monitor being captured: the Host group under the device's rows,
/// and the `Level from` and `Now playing` rows.
fn host_mockup_app() -> App {
    let mut app = mockup_app();
    app.host.linked = true;
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "Gungnir 2 Analog Stereo".into(),
            target: Some("alsa_output.usb-Schiit_Audio_Gungnir_2-00.analog-stereo".into()),
        }),
        true,
    );
    app.host.found_bus(Some(track("So What")), app.now);
    app.host.capturing = true;
    app
}

/// The column of the first cell on row `y` whose symbol is `symbol`.
fn first_cell(buffer: &Buffer, y: u16, symbol: &str) -> u16 {
    (0..buffer.area.width)
        .find(|&x| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == symbol)
        })
        .unwrap_or_else(|| panic!("no {symbol:?} cell on row {y}"))
}

/// One line of a full-width pane, padded out to its border.
fn pane_row(text: &str) -> String {
    format!("│{text:<78}│")
}

/// The scale the cases below give an amp's meters.
const METER_SCALE: (i64, i64) = (0, 10_000);

/// An app on `vidar2` whose left meter names a scale to be read against.
fn amp_with_scale() -> App {
    let mut app = app_on(with_range_on(
        profile_named("vidar2"),
        "meter_1",
        METER_SCALE,
    ));
    animated(&mut app);
    app
}

/// One reading of the left meter.
fn meter(raw: i64) -> ValueObject {
    ValueObject {
        key: Some("meter_1".into()),
        ..meter_reading("20.1", raw)
    }
}

/// An app on `vidar2` with `meter_1` given a linear scale, reporting `raw` on it.
fn app_with_meter_reading(raw: i64) -> App {
    let mut app = amp_with_scale();
    hear(&mut app, vec![meter(raw)]);
    app
}

/// The characters of the row naming `label`, left to right, for locating a column by its
/// glyph rather than its byte offset.
fn row_chars(buffer: &Buffer, label: &str) -> Vec<char> {
    text(buffer)
        .into_iter()
        .find(|row| row.contains(label))
        .unwrap_or_else(|| panic!("a row naming {label:?}"))
        .chars()
        .collect()
}

/// The cells of the row naming `label`, left to right.
fn row_cells(buffer: &Buffer, label: &str) -> Vec<ratatui::buffer::Cell> {
    let y = text(buffer)
        .iter()
        .position(|row| row.contains(label))
        .unwrap_or_else(|| panic!("a row naming {label:?}")) as u16;
    (0..buffer.area.width)
        .map(|x| {
            buffer
                .cell((x, y))
                .expect("a cell inside the buffer")
                .clone()
        })
        .collect()
}

/// Where the `▶` row cursor is drawn, found by its symbol rather than a fixed cell.
fn cursor_cell(buffer: &Buffer) -> (u16, u16) {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if buffer
                .cell((x, y))
                .expect("a cell inside the buffer")
                .symbol()
                == "▶"
            {
                return (x, y);
            }
        }
    }
    panic!("no ▶ cursor drawn");
}

/// The mockup app with the curve boosting: band 2 lifted to +6 dB, band 4 back at 0 dB
/// and band 2's Q at the centre, the volume at `volume` converted through the attenuation
/// table as the device converts it, in the tone-control mode `mode`.
fn boosting(volume: i64, mode: i64) -> App {
    let mut app = mockup_app();
    let table = svolder_profiles::db_tables::table("dac-attenuation-db").expect("the table");
    let db = table[usize::try_from(volume).expect("a step")];
    hear(
        &mut app,
        vec![
            vo("12.2.3", None, json!(127), "127"),
            vo("12.2.1", None, json!(64), "64"),
            vo("12.4.3", None, json!(98), "98"),
            vo("12.50", None, json!(mode), &mode.to_string()),
            converted("10", "volume", volume, &volume.to_string(), db, "dB"),
        ],
    );
    app.settle();
    app
}

/// What a player plays: `title` by Miles Davis, in mpv.
fn track(title: &str) -> svolder_host::NowPlaying {
    svolder_host::NowPlaying {
        genre: Vec::new(),
        title: title.into(),
        artist: "Miles Davis".into(),
        identity: "mpv".into(),
        status: svolder_host::Status::Playing,
        ..Default::default()
    }
}

/// The mockup's app linked to a host whose session bus answered, following `playing`.
fn hearing(playing: Option<svolder_host::NowPlaying>) -> App {
    let mut app = mockup_app();
    app.host.linked = true;
    app.host.found_bus(playing, app.now);
    app
}

/// The mockup's app linked to a host whose sink reads `volume` (1.0 = 100 %), muted or
/// not, named `name`, with the session bus answered and nothing playing on it.
fn sinking(volume: f32, muted: bool, name: &str) -> App {
    let mut app = hearing(None);
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume,
            muted,
            name: name.into(),
            target: None,
        }),
        true,
    );
    app
}
