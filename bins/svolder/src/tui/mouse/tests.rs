// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use svolder_ipc::TuiConfig;
use svolder_profiles::Registry;

use super::*;
use crate::tui::keys;
use crate::tui::render;
use crate::tui::test_support::{
    app_showing, click, drawn, hear, mouse_at, plain_vo as raw_vo, profile, row_of, vo,
};
use crate::tui::theme::{Role, Theme};

/// An app on the built-in profile with the default `[tui]` settings and no device list.
fn app() -> App {
    App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    )
}

/// A rectangle `width` by `height` cells, at `(x, y)`.
fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A drag: the other half of a click this loop never acts on.
fn drag(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// One right click at `(x, y)`.
fn right_click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// One wheel step at `(x, y)`, up for `up`, else down.
fn wheel(x: u16, y: u16, up: bool) -> MouseEvent {
    MouseEvent {
        kind: if up {
            MouseEventKind::ScrollUp
        } else {
            MouseEventKind::ScrollDown
        },
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn a_left_click_inside_a_row_rect_focuses_it() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, 1));
    let app = app();

    assert_eq!(
        resolve(&hits, click(5, 2), &app),
        Some(MouseAction::Focus(Pane::Controls, 1))
    );
}

#[test]
fn a_click_on_a_toggle_flips_it() {
    let mut app = app();
    hear(&mut app, vec![vo("5", Some("mute"), json!(false), "off")]);
    let mute = row_of(&app, "mute");
    let mut hits = HitMap::default();
    hits.push(rect(0, 3, 40, 1), Hit::Row(Pane::Controls, mute));
    hits.push(rect(17, 3, 3, 1), Hit::ToggleOf(mute));

    assert_eq!(
        resolve(&hits, click(17, 3), &app),
        Some(MouseAction::Intent(Intent::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
        }))
    );
}

/// A click on a transport button asks for that button's press; the wheel over it asks for
/// nothing.
#[test]
fn a_click_on_a_transport_button_presses_it() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 12, 36, 1), Hit::Row(Pane::Controls, 7));
    hits.push(rect(23, 12, 4, 1), Hit::Transport(2));
    let app = app();
    assert_eq!(
        resolve(&hits, click(24, 12), &app),
        Some(MouseAction::Transport(2))
    );
    assert_eq!(resolve(&hits, wheel(24, 12, true), &app), None);
}

#[test]
fn a_click_on_select_next_sends_the_next_value() {
    let mut app = app();
    hear(&mut app, vec![vo("2", Some("input"), json!(1), "1")]);
    let input = row_of(&app, "input");
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, input));
    hits.push(rect(30, 2, 1, 1), Hit::SelectNext(input));

    assert_eq!(
        resolve(&hits, click(30, 2), &app),
        Some(MouseAction::Intent(Intent::Set {
            key: "input".into(),
            value: "2".into(),
            step: 1,
        }))
    );
}

#[test]
fn scroll_up_over_a_level_nudges_it_up_by_one_step() {
    let mut app = app();
    hear(&mut app, vec![vo("10", Some("volume"), json!(100), "100")]);
    let volume = row_of(&app, "volume");
    let mut hits = HitMap::default();
    hits.push(rect(0, 4, 40, 1), Hit::Row(Pane::Controls, volume));
    hits.push(rect(17, 4, 15, 1), Hit::Level(volume));

    assert_eq!(
        resolve(&hits, wheel(20, 4, true), &app),
        Some(MouseAction::Intent(Intent::Set {
            key: "volume".into(),
            value: "up".into(),
            step: 1,
        }))
    );
}

#[test]
fn scroll_down_over_the_loudness_bar_steps_it_down() {
    let mut app = app();
    // The loudness function ("12.52" on this profile) names no control of its own, so a
    // step is sent raw.
    hear(&mut app, vec![raw_vo("12.52", 3)]);
    let mut hits = HitMap::default();
    hits.push(rect(30, 0, 5, 1), Hit::Loudness);

    assert_eq!(
        resolve(&hits, wheel(32, 0, false), &app),
        Some(MouseAction::Intent(Intent::Raw {
            id: "12.52".into(),
            value_type: "u8".into(),
            value: "2".into(),
        }))
    );
}

/// A click on the Loudness half toggles the loudness mode and focuses it, while the
/// wheel over it steps the strength.
#[test]
fn a_click_anywhere_on_loudness_toggles_it() {
    let app = app();
    let mut hits = HitMap::default();
    hits.push(rect(20, 0, 20, 1), Hit::Loudness);

    assert_eq!(
        resolve(&hits, click(25, 0), &app),
        Some(MouseAction::ToggleTone(ToneRow::Loudness))
    );
}

/// A click on the Mode half toggles the flat mode and focuses it; a click on either of its
/// `‹ ›` arrows steps the select instead, like every select's.
#[test]
fn a_click_on_mode_toggles_it_and_its_arrows_step_it() {
    let app = app();
    let mut hits = HitMap::default();
    hits.push(rect(0, 0, 20, 1), Hit::Mode);
    hits.push(rect(5, 0, 1, 1), Hit::ModePrev);
    hits.push(rect(10, 0, 1, 1), Hit::ModeNext);

    assert_eq!(
        resolve(&hits, click(2, 0), &app),
        Some(MouseAction::ToggleTone(ToneRow::Mode)),
        "the Mode half off its arrows"
    );
    assert_eq!(
        resolve(&hits, click(5, 0), &app),
        Some(MouseAction::StepMode(-1)),
        "‹"
    );
    assert_eq!(
        resolve(&hits, click(10, 0), &app),
        Some(MouseAction::StepMode(1)),
        "›"
    );
}

/// The wheel over a band's row moves its gain one of the device's own steps, as `←→` do:
/// up from -0.20 dB (raw 97) lands on 0 dB exactly, the step a round half decibel would
/// have skipped, and down from 0 dB on -0.20 dB. At either end of the gain's steps the
/// wheel asks for nothing.
#[test]
fn the_wheel_over_a_band_row_moves_its_gain_one_step() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 5, 40, 1), Hit::Band(1));
    for (raw, up, wanted) in [
        (97, true, Some(0.0)),
        (98, false, Some(-20.0 / 98.0)),
        (127, true, None),
        (0, false, None),
    ] {
        let mut app = app();
        // Band 2's own gain function, 98 being 0 dB per the profile's gain conversion.
        hear(&mut app, vec![raw_vo("12.2.3", raw)]);
        let action = resolve(&hits, wheel(2, 5, up), &app);
        let Some(wanted) = wanted else {
            assert!(action.is_none(), "raw {raw}: {action:?}");
            continue;
        };
        let Some(MouseAction::BandEdit(writes)) = action else {
            panic!("raw {raw}: expected a band edit");
        };
        let [Intent::EqSet { band, param, value }] = writes.as_slice() else {
            panic!("raw {raw}: expected one EqSet, got {writes:?}");
        };
        assert_eq!((*band, param.as_str()), (2, "g"), "raw {raw}");
        assert!(
            (value - wanted).abs() < 1e-9,
            "raw {raw}: expected {wanted} dB, got {value}"
        );
    }
}

/// The wheel over a band's row is a band edit like the key's, and is finished the same way:
/// on a tone control in Flat mode the writes carry the switch to EQ after the band's, and
/// the key line says so; in Loudness mode they do not. A band with no gain read yet asks
/// for nothing at all.
#[test]
fn the_wheel_over_a_band_in_flat_mode_switches_the_tone_control_to_eq() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 5, 40, 1), Hit::Band(1));
    for (mode, switched) in [(1, true), (3, false)] {
        let mut app = app();
        hear(&mut app, vec![raw_vo("12.2.3", 98), raw_vo("12.50", mode)]);
        let Some(MouseAction::BandEdit(writes)) = resolve(&hits, wheel(2, 5, true), &app) else {
            panic!("mode {mode}: expected a band edit");
        };
        let finished = app.band_edit(writes);
        assert!(
            matches!(finished.first(), Some(Intent::EqSet { band: 2, .. })),
            "mode {mode}: {finished:?}"
        );
        let eq = Intent::EqMode("eq".into());
        match switched {
            true => {
                assert_eq!(finished.len(), 2, "mode {mode}: {finished:?}");
                assert_eq!(finished.last(), Some(&eq), "mode {mode}");
                assert_eq!(
                    app.toast().map(|toast| toast.text.as_str()),
                    Some(svolder_profiles::tone::preset::SWITCHED_TO_EQ)
                );
            }
            false => {
                assert_eq!(finished.len(), 1, "mode {mode}: {finished:?}");
                assert_eq!(
                    app.toast().map(|toast| toast.text.clone()),
                    None,
                    "mode {mode}"
                );
            }
        }
    }
    assert_eq!(resolve(&hits, wheel(2, 5, true), &app()), None);
}

#[test]
fn a_click_on_a_band_row_selects_it() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 6, 40, 1), Hit::Band(2));
    let app = app();

    assert_eq!(
        resolve(&hits, click(3, 6), &app),
        Some(MouseAction::Band(2))
    );
}

#[test]
fn a_click_on_a_tab_header_shows_that_screen() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 0, 80, 1), Hit::Tab(Screen::Telemetry));
    let app = app();

    assert_eq!(
        resolve(&hits, click(40, 0), &app),
        Some(MouseAction::Screen(Screen::Telemetry))
    );
}

#[test]
fn a_click_on_a_pane_header_focuses_the_pane_without_moving_the_selection() {
    let mut app = app();
    app.selected = 3;
    let mut hits = HitMap::default();
    hits.push(rect(0, 1, 40, 1), Hit::PaneHeader(Pane::Status));

    assert_eq!(
        resolve(&hits, click(2, 1), &app),
        Some(MouseAction::Focus(Pane::Status, 3))
    );
}

#[test]
fn a_drag_and_a_right_click_ask_for_nothing() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, 1));
    let app = app();

    assert_eq!(resolve(&hits, drag(5, 2), &app), None);
    assert_eq!(resolve(&hits, right_click(5, 2), &app), None);
}

#[test]
fn a_click_outside_every_rect_asks_for_nothing() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, 1));
    let app = app();

    assert_eq!(resolve(&hits, click(5, 10), &app), None);
}

#[test]
fn a_wheel_step_off_every_level_band_or_loudness_hit_asks_for_nothing() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, 1));
    let app = app();

    assert_eq!(resolve(&hits, wheel(5, 2, true), &app), None);
}

/// The hit map one frame of `app` fills in a terminal `width` by `height`.
fn frame_hits(app: &App, width: u16, height: u16) -> HitMap {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    hits
}

/// While the overlay is up the mouse reaches it and nothing else: a click closes it and
/// the wheel scrolls it a line at a time, whatever the screen underneath drew there.
#[test]
fn a_click_closes_the_help_overlay_and_the_wheel_scrolls_it() {
    let mut app = app();
    app.overlays.help_open = true;
    let hits = frame_hits(&app, 80, 24);

    assert_eq!(
        resolve(&hits, click(2, 4), &app),
        Some(MouseAction::Key(KeyCode::Esc)),
        "a click under the overlay asked for something else"
    );
    assert_eq!(
        resolve(&hits, wheel(2, 4, true), &app),
        Some(MouseAction::Key(KeyCode::Up)),
        "the wheel up did not scroll the overlay"
    );
    assert_eq!(
        resolve(&hits, wheel(2, 4, false), &app),
        Some(MouseAction::Key(KeyCode::Down)),
        "the wheel down did not scroll the overlay"
    );
    assert_eq!(
        resolve(&hits, drag(2, 4), &app),
        None,
        "a drag over the overlay asked for something"
    );
}

/// Help opened from the Settings box takes the mouse over the box: a click on a settings row
/// closes help as `Esc` does, and the wheel scrolls help, not the box.
#[test]
fn a_click_on_the_settings_box_under_help_closes_help() {
    let mut app = app();
    app.key(KeyEvent::from(KeyCode::Char('s')));
    let boxed = frame_hits(&app, 80, 24);
    let row = boxed
        .area_of(Hit::SettingRow(0))
        .expect("the Settings box drew its first row");
    assert!(
        matches!(
            resolve(&boxed, click(row.x, row.y), &app),
            Some(MouseAction::Setting(_))
        ),
        "the row takes no click with the box alone"
    );
    app.key(KeyEvent::from(KeyCode::Char('?')));
    assert!(
        app.overlays.help_open && app.overlays.settings.is_some(),
        "? did not open help over the box"
    );
    let hits = frame_hits(&app, 80, 24);

    assert_eq!(
        resolve(&hits, click(row.x, row.y), &app),
        Some(MouseAction::Key(KeyCode::Esc)),
        "the click reached the box under help"
    );
    assert_eq!(
        resolve(&hits, wheel(row.x, row.y, false), &app),
        Some(MouseAction::Key(KeyCode::Down)),
        "the wheel did not scroll help"
    );
}

/// The cell in the middle of the box on top of the frame `hits` records.
fn middle(hits: &HitMap) -> (u16, u16) {
    let top = hits.top_box().expect("a box is open");
    (top.x + top.width / 2, top.y + top.height / 2)
}

/// A click outside the box on top presses `Esc` and reaches nothing beneath it, and a click
/// inside it asks for nothing: the header's `esc` mark and the logo included.
fn assert_outside_closes(app: &App, name: &str) {
    let hits = frame_hits(app, 80, 24);
    let top = hits.top_box().unwrap_or_else(|| panic!("{name}: no box"));
    let (x, y) = middle(&hits);
    assert_eq!(resolve(&hits, click(x, y), app), None, "{name}: inside");
    let esc = hits
        .area_of(Hit::EscMark)
        .or_else(|| hits.area_of(Hit::HelpMark))
        .expect("the header draws its marks");
    assert!(
        top.y > esc.y,
        "{name}: the header is not outside the box {top:?}"
    );
    for (what, at) in [
        ("the header", (esc.x, esc.y)),
        ("the logo", (1, 0)),
        ("a corner", (0, 23)),
    ] {
        assert_eq!(
            resolve(&hits, click(at.0, at.1), app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{name}: {what}"
        );
    }
    assert_eq!(
        resolve(&hits, right_click(0, 23), app),
        None,
        "{name}: right"
    );
}

/// The device editor and the question a removal asks are answered from the keyboard, so
/// the rows under them take no clicks while one is open, and a click outside presses `Esc`.
#[test]
fn the_device_editor_and_the_removal_take_no_clicks() {
    let mut app = app_showing(Screen::Devices);
    let hits = frame_hits(&app, 80, 24);
    assert!(
        resolve(&hits, click(4, 3), &app).is_some(),
        "a row of the list is not clickable to begin with"
    );

    app.key(KeyEvent::from(KeyCode::Char('u')));
    assert!(app.devices.editor.is_some(), "the editor did not open");
    assert_outside_closes(&app, "editor");

    app.devices.editor = None;
    app.key(KeyEvent::from(KeyCode::Char('x')));
    assert!(app.devices.confirm.is_some(), "the removal did not ask");
    assert_outside_closes(&app, "removal");
}

/// The preset list is answered from the keyboard too: nothing under it takes a click or the
/// wheel, on the tone-control screen or on the device screen, and a click outside presses `Esc`.
#[test]
fn the_preset_list_takes_no_clicks() {
    for screen in [Screen::Tone, Screen::Device] {
        let mut app = app_showing(screen);
        let hits = frame_hits(&app, 80, 24);
        let (x, y) = word_cell(&hits, KeyCode::Char('p'));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Char('p'))),
            "{screen:?}: the word's p is not the key"
        );
        app.key(KeyEvent::from(KeyCode::Char('p')));
        assert!(
            app.presets.preset_list.is_some(),
            "{screen:?}: p opened no list"
        );
        let hits = frame_hits(&app, 80, 24);
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{screen:?}: outside"
        );
        let (mx, my) = middle(&hits);
        assert_eq!(resolve(&hits, click(mx, my), &app), None, "{screen:?}");
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            ..click(mx, my)
        };
        assert_eq!(resolve(&hits, wheel, &app), None, "{screen:?}");
        assert_outside_closes(&app, "preset list");
    }
}

/// The Loki Max's list is answered from the keyboard too: nothing in it takes a click or the
/// wheel, the question a save asks included, and a click outside presses `Esc`.
#[test]
fn the_loki_max_preset_list_takes_no_clicks() {
    let mut app = App::new(
        crate::tui::test_support::profile_named("lokimax"),
        "loki".into(),
        &TuiConfig::default(),
        vec![crate::backend::DeviceRow::for_test("loki")],
    );
    app.key(KeyEvent::from(KeyCode::Char('p')));
    assert!(app.presets.device_list.is_some(), "p opened no list");
    for asking in [false, true] {
        if asking {
            app.key(KeyEvent::from(KeyCode::Up));
            app.key(KeyEvent::from(KeyCode::Char('u')));
            assert!(
                matches!(
                    app.presets.device_list.as_ref().map(|list| &list.step),
                    Some(crate::tui::boxes::device_presets::Step::Asking(_))
                ),
                "u opened no save question"
            );
        }
        let hits = frame_hits(&app, 80, 24);
        let (mx, my) = middle(&hits);
        assert_eq!(resolve(&hits, click(mx, my), &app), None, "asking {asking}");
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            ..click(mx, my)
        };
        assert_eq!(resolve(&hits, wheel, &app), None, "asking {asking}");
        assert_outside_closes(&app, "Loki Max preset list");
    }
}

/// The quit question takes no clicks either: not on the rows under it, and not on the tabs
/// of the strip above them, where a click presses `Esc`.
#[test]
fn the_quit_question_takes_no_clicks() {
    let mut app = app_showing(Screen::Device);
    let (buffer, hits) = drawn(&app, 80, 24);
    let (x, y) = strip_cell(&buffer, "Telemetry");
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Screen(Screen::Telemetry)),
        "the tab is not a click to begin with"
    );
    app.key(KeyEvent::from(KeyCode::Char('q')));
    assert!(app.overlays.quitting, "q did not ask");
    let hits = frame_hits(&app, 80, 24);
    for at in [(x, y), (4, 3), (60, 12)] {
        assert_eq!(
            resolve(&hits, click(at.0, at.1), &app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{at:?}"
        );
    }
    assert_outside_closes(&app, "quit");
}

/// A band's edit line takes the mouse like the other boxes: inside asks for nothing, outside
/// presses `Esc`.
#[test]
fn a_click_outside_a_bands_edit_line_cancels_it() {
    let mut app = tone_app();
    app.key(KeyEvent::from(KeyCode::Enter));
    assert!(app.tone.edit_line.is_some(), "Enter opened no edit line");
    let hits = frame_hits(&app, 80, 24);
    let (x, y) = middle(&hits);
    assert_eq!(resolve(&hits, click(x, y), &app), None);
    assert_eq!(
        resolve(&hits, click(0, 0), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

/// The first cell whose symbol is `symbol` on the row that carries `text`.
fn cell_on_row(buffer: &ratatui::buffer::Buffer, text: &str, symbol: &str) -> (u16, u16) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("a cell").symbol())
            .collect();
        if !row.contains(text) {
            continue;
        }
        if let Some(x) = (0..buffer.area.width).find(|x| {
            buffer
                .cell((*x, y))
                .is_some_and(|cell| cell.symbol() == symbol)
        }) {
            return (x, y);
        }
    }
    panic!("no {symbol:?} on a row carrying {text:?}");
}

/// At 126×62, 200×60 and 300×60 the rows, the volume bar and the band rows are clicked and
/// wheeled where the wide and tall layout drew them.
#[test]
fn the_mouse_follows_the_wide_and_tall_layout() {
    for (width, height) in [(126, 62), (200, 60), (300, 60)] {
        let mut app = app();
        hear(
            &mut app,
            vec![
                vo("5", Some("mute"), json!(false), "off"),
                vo("10", Some("volume"), json!(100), "100"),
            ],
        );
        let (buffer, hits) = drawn(&app, width, height);
        let (x, y) = cell_on_row(&buffer, "Mute", "M");
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Focus(Pane::Controls, row_of(&app, "mute"))),
            "{width}×{height}: a click on the Mute row"
        );
        let (x, y) = cell_on_row(&buffer, "Volume", "█");
        assert_eq!(
            resolve(&hits, wheel(x, y, true), &app),
            Some(MouseAction::Intent(Intent::Set {
                key: "volume".into(),
                value: "up".into(),
                step: 1,
            })),
            "{width}×{height}: the wheel on the volume bar"
        );
        let (x, y) = cell_on_row(&buffer, "┌ Controls", "C");
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Focus(Pane::Controls, app.selected)),
            "{width}×{height}: the Controls title"
        );

        hear(&mut app, vec![raw_vo("12.4.2", 64)]);
        app.show(Screen::Tone);
        let (buffer, hits) = drawn(&app, width, height);
        let (x, y) = cell_on_row(&buffer, "2515 Hz", "4");
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Band(3)),
            "{width}×{height}: a click on band 4"
        );
    }
}

/// An app on the tone-control screen in EQ mode, every band at its centre, 0 dB and Q 4.30.
fn tone_app() -> App {
    let mut app = app();
    let mut values = vec![raw_vo("12.50", 2), raw_vo("12.52", 0)];
    for band in 1..=5 {
        values.push(raw_vo(&format!("12.{band}.2"), 64));
        values.push(raw_vo(&format!("12.{band}.3"), 98));
        values.push(raw_vo(&format!("12.{band}.1"), 64));
    }
    hear(&mut app, values);
    app.show(Screen::Tone);
    app
}

/// The cell band `band`'s point is drawn in on the curve `hits` recorded.
fn point_of(app: &App, hits: &HitMap, band: usize) -> (u16, u16) {
    let curve = hits.area_of(Hit::Curve).expect("the frame drew a curve");
    crate::tui::screens::tone::points(app, curve)
        .into_iter()
        .find(|(drawn, _)| *drawn == band)
        .map(|(_, cell)| cell)
        .unwrap_or_else(|| panic!("band {band} has no point"))
}

/// On the curve the tone screen draws, a left or a right press on a band's point grabs it; a
/// left press away from every point grabs nothing, where a right one grabs the band nearest
/// in log frequency; and at 80×24 and 126×62 alike.
#[test]
fn a_press_on_the_curve_grabs_the_band_whose_point_it_lands_on() {
    for (width, height) in [(80, 24), (126, 62)] {
        let app = tone_app();
        let hits = frame_hits(&app, width, height);
        let curve = hits.area_of(Hit::Curve).expect("a curve");
        let (x, y) = point_of(&app, &hits, 3);
        let size = format!("{width}×{height}");
        for (button, gesture) in [
            (MouseButton::Left, Gesture::Move),
            (MouseButton::Right, Gesture::Width),
        ] {
            assert_eq!(
                resolve(&hits, mouse_at(MouseEventKind::Down(button), x, y), &app),
                Some(MouseAction::Grab {
                    band: 3,
                    gesture,
                    at: (x, y)
                }),
                "{size}: {button:?}"
            );
        }
        let (x, top) = (x, curve.y);
        assert_eq!(resolve(&hits, click(x, top), &app), None, "{size}");
        assert_eq!(
            resolve(&hits, right_click(x, top), &app),
            Some(MouseAction::Grab {
                band: 3,
                gesture: Gesture::Width,
                at: (x, top)
            }),
            "{size}"
        );
    }
}

/// The wheel over the curve steps the Q of the band under the pointer one of the device's own
/// steps — up narrows the bell and down widens it — and off every point the band whose
/// centre is nearest in log frequency, as a band edit like the wheel over a band's row. At
/// the top of the Q's steps the wheel up asks for nothing.
#[test]
fn the_wheel_over_the_curve_steps_the_q_of_the_band_under_it() {
    let eq = profile().eq.expect("a tone control");
    let q = |band: usize, raw: u8| {
        MouseAction::BandEdit(vec![Intent::EqSet {
            band,
            param: "q".into(),
            value: eq.q(raw),
        }])
    };
    let mut app = tone_app();
    let hits = frame_hits(&app, 80, 24);
    let (x, y) = point_of(&app, &hits, 2);
    assert_eq!(resolve(&hits, wheel(x, y, true), &app), Some(q(3, 65)));
    assert_eq!(resolve(&hits, wheel(x, y - 3, false), &app), Some(q(3, 63)));
    let (last, _) = point_of(&app, &hits, 4);
    assert_eq!(
        resolve(&hits, wheel(last + 3, y - 3, true), &app),
        Some(q(5, 65))
    );
    hear(&mut app, vec![raw_vo("12.3.1", 127)]);
    assert_eq!(resolve(&hits, wheel(x, y, true), &app), None);
}

/// Each of a band row's three values is a target of its own, over the text the row draws for
/// it, unit and all; a single click there focuses the band as anywhere on the row does, and
/// the wheel there steps its gain as it does anywhere on the row.
#[test]
fn a_band_rows_values_are_targets_over_their_text() {
    for (width, height) in [(80, 24), (126, 62)] {
        let app = tone_app();
        let (buffer, hits) = drawn(&app, width, height);
        for (field, wanted) in ["  160 Hz", "  +0.0 dB", "4.30"].into_iter().enumerate() {
            let area = hits
                .area_of(Hit::BandField(1, field))
                .unwrap_or_else(|| panic!("{width}×{height}: no target for field {field}"));
            let text: String = (area.x..area.right())
                .map(|x| buffer.cell((x, area.y)).expect("a cell").symbol())
                .collect();
            assert_eq!(text, wanted, "{width}×{height}: field {field}");
            assert_eq!(
                resolve(&hits, click(area.x, area.y), &app),
                Some(MouseAction::Band(1)),
                "{width}×{height}: field {field}"
            );
            assert!(
                matches!(
                    resolve(&hits, wheel(area.x, area.y, true), &app),
                    Some(MouseAction::BandEdit(_))
                ),
                "{width}×{height}: the wheel over field {field}"
            );
        }
    }
}

/// Two left presses on the same cell of a band row's frequency, gain or Q within
/// [`DOUBLE_CLICK`] of each other are a double-click, which opens the edit line on that
/// value's field; the first of them, a third one right after, a second press on another
/// cell, and two presses 500 ms apart each only focus the band.
#[test]
fn a_double_click_on_a_band_value_opens_its_field_and_a_single_click_does_not() {
    let mut app = tone_app();
    let hits = frame_hits(&app, 80, 24);
    let press = |app: &mut App, at: (u16, u16), when: Instant| {
        note_press(app, &click(at.0, at.1), when);
        resolve(&hits, click(at.0, at.1), app)
    };
    let start = Instant::now();
    let ms = Duration::from_millis;
    for field in 0..3 {
        let area = frame_hits(&app, 80, 24)
            .area_of(Hit::BandField(1, field))
            .expect("a value's target");
        let (at, beside) = ((area.x + 1, area.y), (area.x + 2, area.y));
        let t = start + ms(10_000 * field as u64);
        let band = Some(MouseAction::Band(1));
        assert_eq!(press(&mut app, at, t), band, "field {field}: one press");
        assert_eq!(
            press(&mut app, at, t + ms(400)),
            Some(MouseAction::EditBand { band: 1, field }),
            "field {field}: two presses"
        );
        assert_eq!(
            press(&mut app, at, t + ms(450)),
            band,
            "field {field}: a third"
        );
        assert_eq!(
            press(&mut app, beside, t + ms(500)),
            band,
            "field {field}: beside"
        );
        assert_eq!(
            press(&mut app, beside, t + ms(1_000)),
            band,
            "field {field}: 500 ms"
        );
    }
}

/// While a drag is held, a press of the other button asks for nothing, on the curve or off
/// it, and leaves the drag as it is.
#[test]
fn a_press_of_the_other_button_during_a_drag_is_ignored() {
    let mut app = tone_app();
    let hits = frame_hits(&app, 80, 24);
    let (x, y) = point_of(&app, &hits, 3);
    let row = hits.area_of(Hit::Band(4)).expect("band 5's row");
    curve_drag::start(&mut app, 1, Gesture::Move, (20, 6));
    assert_eq!(resolve(&hits, right_click(x, y), &app), None);
    curve_drag::start(&mut app, 1, Gesture::Width, (20, 6));
    assert_eq!(resolve(&hits, click(x, y), &app), None);
    assert_eq!(resolve(&hits, click(row.x + 2, row.y), &app), None);
    assert_eq!(
        app.pointer.curve_drag.as_ref().map(|drag| drag.gesture),
        Some(Gesture::Width)
    );
}

/// A press a box swallows — the click that closes the help overlay — or one a drag of the
/// other button ignores arms no double-click: the press on the same cell right after it is
/// a single click.
#[test]
fn a_swallowed_press_arms_no_double_click() {
    let mut app = tone_app();
    let hits = frame_hits(&app, 80, 24);
    let area = hits.area_of(Hit::BandField(1, 1)).expect("band 2's gain");
    let at = click(area.x + 1, area.y);
    let start = Instant::now();
    let ms = Duration::from_millis;

    app.overlays.help_open = true;
    note_press(&mut app, &at, start);
    app.overlays.help_open = false;
    note_press(&mut app, &at, start + ms(100));
    assert_eq!(
        resolve(&hits, at, &app),
        Some(MouseAction::Band(1)),
        "after the overlay"
    );

    let mut app = tone_app();
    curve_drag::start(&mut app, 3, Gesture::Width, (40, 6));
    note_press(&mut app, &at, start);
    app.pointer.curve_drag = None;
    note_press(&mut app, &at, start + ms(100));
    assert_eq!(
        resolve(&hits, at, &app),
        Some(MouseAction::Band(1)),
        "after a drag"
    );
}

/// Dragging with the button a drag is held by drags the point, wherever the pointer is —
/// past the curve too — against the cells the curve was drawn in, and letting go of that
/// button lets go of the point; the other button, or either with no drag held, asks for
/// nothing. Loudness mode draws no curve to press on.
#[test]
fn the_drags_own_button_drags_and_lets_go() {
    let mut app = tone_app();
    let hits = frame_hits(&app, 80, 24);
    let curve = hits.area_of(Hit::Curve).expect("a curve");
    let up = |button| mouse_at(MouseEventKind::Up(button), 10, 10);
    let moved = |button| mouse_at(MouseEventKind::Drag(button), 3, 20);
    assert_eq!(resolve(&hits, up(MouseButton::Left), &app), None);
    assert_eq!(resolve(&hits, moved(MouseButton::Left), &app), None);
    curve_drag::start(&mut app, 1, Gesture::Move, (20, 6));
    assert_eq!(
        resolve(&hits, moved(MouseButton::Left), &app),
        Some(MouseAction::DragTo { at: (3, 20), curve })
    );
    assert_eq!(resolve(&hits, moved(MouseButton::Right), &app), None);
    assert_eq!(
        resolve(&hits, up(MouseButton::Left), &app),
        Some(MouseAction::Release {
            at: (10, 10),
            curve: Some(curve)
        })
    );
    assert_eq!(resolve(&hits, up(MouseButton::Right), &app), None);

    hear(&mut app, vec![raw_vo("12.50", 3)]);
    let hits = frame_hits(&app, 80, 24);
    assert_eq!(hits.area_of(Hit::Curve), None, "Loudness mode drew a curve");
}

#[test]
fn overlapping_rects_let_the_later_inner_hit_win() {
    let mut app = app();
    hear(&mut app, vec![vo("5", Some("mute"), json!(true), "on")]);
    let mute = row_of(&app, "mute");
    let mut hits = HitMap::default();
    hits.push(rect(0, 3, 40, 1), Hit::Row(Pane::Controls, mute));
    hits.push(rect(17, 3, 3, 1), Hit::ToggleOf(mute));

    // Inside the toggle box: the later, narrower hit wins over the row it sits inside.
    assert_eq!(
        resolve(&hits, click(18, 3), &app),
        Some(MouseAction::Intent(Intent::Set {
            key: "mute".into(),
            value: "off".into(),
            step: 1,
        }))
    );
    // Elsewhere on the same row: the row hit still answers.
    assert_eq!(
        resolve(&hits, click(2, 3), &app),
        Some(MouseAction::Focus(Pane::Controls, mute))
    );
}

/// The cell of the strip under the header where `word` starts, at 80×24.
fn strip_cell(buffer: &ratatui::buffer::Buffer, word: &str) -> (u16, u16) {
    let row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 1)).expect("a cell").symbol())
        .collect();
    let byte = row.find(word).expect("the word is on the strip");
    (
        u16::try_from(row[..byte].chars().count()).expect("a column"),
        1,
    )
}

/// A click on a tab's name shows that screen; on the current tab it does nothing.
#[test]
fn a_click_on_a_tab_of_the_strip_shows_that_screen() {
    let mut app = tone_app();
    app.show(Screen::Device);
    let (buffer, hits) = drawn(&app, 80, 24);
    for (word, screen) in [
        ("EQ", Screen::Tone),
        ("Telemetry", Screen::Telemetry),
        ("Devices", Screen::Devices),
    ] {
        let (x, y) = strip_cell(&buffer, word);
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Screen(screen)),
            "{word}"
        );
    }
    app.show(Screen::Telemetry);
    let (buffer, hits) = drawn(&app, 80, 24);
    let (x, y) = strip_cell(&buffer, "Controls");
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Screen(Screen::Device))
    );
    let (x, y) = strip_cell(&buffer, "Telemetry");
    assert_eq!(resolve(&hits, click(x, y), &app), None, "the current tab");
}

/// A click on `? help` opens help, and on `esc …` acts as `Esc`.
#[test]
fn a_click_on_the_marks_of_the_strip_is_the_key() {
    let app = tone_app();
    let (buffer, hits) = drawn(&app, 80, 24);
    let (x, y) = strip_cell(&buffer, "? help");
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Key(KeyCode::Char('?')))
    );
    let (x, y) = strip_cell(&buffer, "esc");
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

/// A click on the `EQ` tab of a device with no tone control does what `e` does there.
#[test]
fn a_click_on_eq_with_no_tone_control_asks_for_nothing() {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("lyr5")
        .expect("lyr5 is built in")
        .clone();
    let app = App::new(profile, "lyr5".into(), &TuiConfig::default(), Vec::new());
    let (buffer, hits) = drawn(&app, 80, 24);
    let (x, y) = strip_cell(&buffer, "EQ");
    assert_eq!(resolve(&hits, click(x, y), &app), None);
}

/// The cell of the first column of the word whose key is `code` on the last frame.
fn word_cell(hits: &HitMap, code: KeyCode) -> (u16, u16) {
    let area = hits
        .area_of(Hit::Key(code))
        .unwrap_or_else(|| panic!("the frame drew no word for {code:?}"));
    (area.x, area.y)
}

/// A click on a word of a border is its key.
#[test]
fn a_click_on_a_word_presses_its_key() {
    let app = app_showing(Screen::Device);
    let hits = frame_hits(&app, 80, 24);
    for code in ['m', 'f', 'p'] {
        let (x, y) = word_cell(&hits, KeyCode::Char(code));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Char(code))),
            "{code}"
        );
    }
    let app = app_showing(Screen::Devices);
    let hits = frame_hits(&app, 80, 24);
    for code in ['n', 'u', 'x', 'r'] {
        let (x, y) = word_cell(&hits, KeyCode::Char(code));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Char(code))),
            "{code}"
        );
    }
}

/// A word that is faint on this device is still its key: the click does what the key does.
#[test]
fn a_click_on_a_faint_word_is_the_key_all_the_same() {
    let mut app = app_showing(Screen::Device);
    app.profile
        .controls
        .retain(|control| control.center.is_none());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let theme = Theme::new(false);
    terminal
        .draw(|frame| {
            render::draw(frame, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let centre = keys::hints_for(Screen::Device)
        .into_iter()
        .find(|hint| hint.code == KeyCode::Char('0'))
        .expect("the device screen's 0");
    assert!(
        !keys::available(centre, &app),
        "a device with no balance has nothing to centre"
    );
    let (x, y) = word_cell(&hits, KeyCode::Char('0'));
    let cell = terminal.backend().buffer().cell((x, y)).expect("a cell");
    assert_eq!(
        Some(cell.fg),
        theme.style(Role::Faint).fg,
        "the word is drawn faint"
    );
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Key(KeyCode::Char('0')))
    );
}

/// While a box is open a click on a word does nothing: not the word's key, and not the
/// screen's under the box.
#[test]
fn a_click_on_a_word_does_nothing_while_a_box_is_open() {
    let opens: [(&str, &[KeyCode]); 4] = [
        ("settings", &[KeyCode::Esc, KeyCode::Char('s')]),
        ("preset list", &[KeyCode::Char('p')]),
        ("quit question", &[KeyCode::Char('q')]),
        ("help", &[KeyCode::Char('?')]),
    ];
    for (name, keys_to_press) in opens {
        let mut app = app_showing(Screen::Device);
        let (x, y) = word_cell(&frame_hits(&app, 80, 24), KeyCode::Char('m'));
        for code in keys_to_press {
            app.key(KeyEvent::from(*code));
        }
        assert!(app.modal_open(), "{name} did not open");
        let hits = frame_hits(&app, 80, 24);
        let action = resolve(&hits, click(x, y), &app);
        assert_ne!(
            action,
            Some(MouseAction::Key(KeyCode::Char('m'))),
            "{name}: the click pressed the word's key"
        );
        // The help overlay closes on any click, and the Settings box reads
        // the cell as its own; the other boxes ask for nothing.
        match name {
            "help" => assert_eq!(action, Some(MouseAction::Key(KeyCode::Esc))),
            "settings" => assert!(matches!(action, None | Some(MouseAction::Setting(_)))),
            _ => assert_eq!(action, Some(MouseAction::Key(KeyCode::Esc)), "{name}"),
        }
    }
    // The menu is a box too: a click outside it closes it, as Esc does, and presses nothing.
    let mut app = app_showing(Screen::Device);
    let (x, y) = word_cell(&frame_hits(&app, 80, 24), KeyCode::Char('m'));
    app.key(KeyEvent::from(KeyCode::Esc));
    assert!(app.overlays.menu.is_some(), "Esc did not open the menu");
    let hits = frame_hits(&app, 80, 24);
    assert_ne!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::Key(KeyCode::Char('m')))
    );
    // The device editor and the removal question are answered from the keyboard.
    for opener in ['u', 'x'] {
        let mut app = app_showing(Screen::Devices);
        let (x, y) = word_cell(&frame_hits(&app, 80, 24), KeyCode::Char('r'));
        app.key(KeyEvent::from(KeyCode::Char(opener)));
        assert!(app.modal_open(), "{opener} opened nothing");
        let hits = frame_hits(&app, 80, 24);
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{opener}"
        );
    }
}

/// The rest of a border keeps its hits: the Tone control title focuses its pane and the Devices
/// top border leads to the device screen.
#[test]
fn the_words_leave_the_rest_of_the_border_its_hits() {
    let app = app_showing(Screen::Device);
    let hits = frame_hits(&app, 80, 24);
    // The Tone pane starts at column 38; its title is the first cell after the corner.
    assert_eq!(
        resolve(&hits, click(40, 2), &app),
        Some(MouseAction::Focus(Pane::Tone, app.selected))
    );
    let app = app_showing(Screen::Devices);
    let hits = frame_hits(&app, 80, 24);
    let (word, _) = word_cell(&hits, KeyCode::Char('n'));
    assert_eq!(
        resolve(&hits, click(word.saturating_sub(5), 2), &app),
        Some(MouseAction::Screen(Screen::Device))
    );
}

/// A click on the Tone control pane's title moves the keys to the pane like the other two
/// titles do, at either size, and earns the light that runs round it; the tone-control screen is
/// reached by its tab and `e`.
#[test]
fn a_click_on_the_tone_panes_title_focuses_the_pane_and_earns_the_light() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = app_showing(Screen::Device);
        let hits = frame_hits(&app, width, height);
        let title = hits
            .area_of(Hit::PaneHeader(Pane::Tone))
            .unwrap_or_else(|| panic!("no title hit at {width}x{height}"));
        let Some(MouseAction::Focus(pane, row)) = resolve(&hits, click(title.x, title.y), &app)
        else {
            panic!("a click on the Tone pane's title asked for nothing at {width}x{height}");
        };
        app.frame_flags.pane_changed = false;
        app.focus(pane, row);
        assert_eq!(app.pane, Pane::Tone, "{width}x{height}");
        assert_eq!(app.screen, Screen::Device, "{width}x{height}");
        assert!(
            app.frame_flags.pane_changed,
            "{width}x{height}: no light round the pane"
        );
    }
}

/// The device screen with a tone control read and the keys on the Controls pane.
fn device_tone_app() -> App {
    let mut app = tone_app();
    app.show(Screen::Device);
    app.focus(Pane::Controls, 0);
    app.frame_flags.pane_changed = false;
    app
}

/// A click on a band's row of the Tone control pane, or on its Gain cell, moves the keys to the
/// pane, selects the band (and, on the cell, takes the double-click no further than the
/// selection, the edit line not being drawn there) and earns the light.
#[test]
fn a_click_on_a_tone_pane_band_row_or_gain_cell_selects_it_and_earns_the_light() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = device_tone_app();
        let hits = frame_hits(&app, width, height);
        let row = hits
            .area_of(Hit::Band(2))
            .unwrap_or_else(|| panic!("{width}x{height}: no target on band 3's row"));
        let gain = hits
            .area_of(Hit::BandField(3, 1))
            .unwrap_or_else(|| panic!("{width}x{height}: no target on band 4's gain"));
        assert_eq!(
            resolve(&hits, click(row.x, row.y), &app),
            Some(MouseAction::Band(2)),
            "{width}x{height}"
        );
        app.focus_band(2);
        assert_eq!(
            (app.pane, app.tone.band),
            (Pane::Tone, 2),
            "{width}x{height}"
        );
        assert!(app.frame_flags.pane_changed, "{width}x{height}: no light");
        app.frame_flags.pane_changed = false;
        app.pointer.double_click = true;
        assert_eq!(
            resolve(&hits, click(gain.x, gain.y), &app),
            Some(MouseAction::Band(3)),
            "{width}x{height}: a double-click opens no edit line here"
        );
        app.focus_band(3);
        assert_eq!(app.tone.band, 3, "{width}x{height}");
        assert!(
            !app.frame_flags.pane_changed,
            "{width}x{height}: the keys were already there"
        );
    }
}

/// The wheel over a Gain cell of the Tone control pane nudges that band's gain.
#[test]
fn the_wheel_over_a_tone_pane_gain_cell_nudges_that_bands_gain() {
    for (width, height) in [(80, 24), (120, 40)] {
        let app = device_tone_app();
        let hits = frame_hits(&app, width, height);
        let gain = hits
            .area_of(Hit::BandField(3, 1))
            .unwrap_or_else(|| panic!("{width}x{height}: no target on band 4's gain"));
        let Some(MouseAction::BandEdit(writes)) = resolve(&hits, wheel(gain.x, gain.y, true), &app)
        else {
            panic!("{width}x{height}: the wheel asked for nothing");
        };
        assert!(
            matches!(writes.first(), Some(Intent::EqSet { band: 4, param, .. }) if param == "g"),
            "{width}x{height}: {writes:?}"
        );
    }
}

/// The three values of a Tone control pane band row are targets over the text drawn for them.
#[test]
fn a_tone_pane_rows_values_are_targets_over_their_text() {
    let app = device_tone_app();
    let (buffer, hits) = drawn(&app, 120, 40);
    for (field, wanted) in ["  160 Hz", "  +0.0 dB", "4.3"].into_iter().enumerate() {
        let area = hits
            .area_of(Hit::BandField(1, field))
            .unwrap_or_else(|| panic!("no target for field {field}"));
        let text: String = (area.x..area.right())
            .map(|x| buffer.cell((x, area.y)).expect("a cell").symbol())
            .collect();
        assert_eq!(text.trim_start(), wanted.trim_start(), "field {field}");
    }
}

#[test]
fn a_move_asks_for_nothing_wherever_it_lands() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 2, 40, 1), Hit::Row(Pane::Controls, 1));
    hits.push(rect(0, 3, 40, 1), Hit::Tab(Screen::Tone));
    let app = app();
    let moved = |x, y| MouseEvent {
        kind: MouseEventKind::Moved,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };

    assert_eq!(resolve(&hits, moved(5, 2), &app), None);
    assert_eq!(resolve(&hits, moved(5, 3), &app), None);
    assert_eq!(resolve(&hits, moved(50, 9), &app), None);
}

#[test]
fn the_headers_connection_mouse_and_fault_marks_do_nothing_when_clicked() {
    let mut hits = HitMap::default();
    hits.push(rect(0, 0, 10, 1), Hit::Connection);
    hits.push(rect(10, 0, 10, 1), Hit::MouseMark);
    hits.push(rect(20, 0, 10, 1), Hit::FaultMark);
    let app = app();

    for x in [3, 13, 23] {
        assert_eq!(resolve(&hits, click(x, 0), &app), None);
        assert_eq!(resolve(&hits, wheel(x, 0, true), &app), None);
        assert_eq!(resolve(&hits, right_click(x, 0), &app), None);
    }
}
