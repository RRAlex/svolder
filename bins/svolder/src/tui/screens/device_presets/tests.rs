// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use svolder_ipc::TuiConfig;

use crate::backend::DeviceRow;
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::presets::hear_id;
use crate::tui::test_support::{assert_rows, profile_named, text};

use super::*;

/// One frame of `app` in a terminal `width` by `height` drawn with `theme`, and where it put its
/// pieces.
fn frame(app: &App, (width, height): (u16, u16), theme: &Theme) -> (Buffer, FrameAreas) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, theme, &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas)
}

/// The box's own rows, as text.
fn boxed_rows(buffer: &Buffer, area: Rect) -> Vec<String> {
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect()
        })
        .collect()
}

/// Press one key.
fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::from(code));
}

/// A Loki Max on its device screen whose bands read Bass Boost, with the list open on it.
fn listing() -> App {
    let mut app = App::new(
        profile_named("lokimax"),
        "Loki Max".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("Loki Max")],
    );
    for (band, raw) in [28, 21, 17, 15, 15, 15].into_iter().enumerate() {
        let id = app
            .profile
            .control_by_key(&format!("band{}", band + 1))
            .expect("a band")
            .id;
        hear_id(&mut app, id, raw);
    }
    press(&mut app, KeyCode::Char('p'));
    app
}

/// The list over the device screen on an 80×24 terminal: the slots, the `Built in` header and the
/// built-ins, the one the bands read marked `●`, and the keys named on the box with the save key
/// dimmed and its reason while a built-in is focused. usage/tui.md shows this very frame.
#[test]
fn the_loki_max_preset_list_draws_at_80_by_24() {
    let (buffer, _) = frame(&listing(), (80, 24), &Theme::new(false));
    assert_rows(
        &buffer,
        &[
            " svolder  Loki Max  ● connected                                        mouse:on ",
            "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─",
            "┌ Controls ───────────────────────────────────────── mute ─ 0 centre ─ refresh ┐",
            "│ ▶ Input  —                                                                   │",
            "│   Mode   —                                                                   │",
            "│                                                                              │",
            "│   20 Hz  █████28/30███┌ Presets · Loki Max ─────────┐                        │",
            "│   120 Hz █████21/30░░░│     Preset 1                │                        │",
            "│   550 Hz █████17/30░░░│     Preset 2                │                        │",
            "│   2 kHz  █████15/30░░░│     Preset 3                │                        │",
            "│   6 kHz  █████15/30░░░│     Built in                │                        │",
            "│   20 kHz █████15/30░░░│     Flat                    │                        │",
            "│                       │ ▶ ● Bass Boost              │                        │",
            "│                       │     Treble Boost            │                        │",
            "│                       │     Smiley                  │                        │",
            "│                       │     Loudness                │                        │",
            "│                       │                             │                        │",
            "│                       │ Enter loads · Esc closes    │                        │",
            "│                       │ u saves · device slots only │                        │",
            "└───────────────────────└─────────────────────────────┘────────────────────────┘",
            "┌ Status ────────────────────────────────────────────────────────────── advice ┐",
            "│                                                                              │",
            "│ Raw: none                                                                    │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
    crate::tui::test_support::assert_usage_md_shows(&text(&buffer), "Loki Max preset list");
}

/// With a slot focused the save key is named like every other key of the box, and the box keeps
/// its size while the focus moves.
#[test]
fn the_save_key_is_live_on_a_slot_and_the_box_keeps_its_size() {
    let mut app = listing();
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let built_in = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    assert!(
        built_in[12].contains("u saves · device slots only"),
        "{built_in:#?}"
    );
    for _ in 0..2 {
        press(&mut app, KeyCode::Up);
    }
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let area = areas.modal.expect("the list is a modal box");
    let slot = boxed_rows(&buffer, area);
    assert_eq!(slot.len(), built_in.len());
    assert_eq!(slot[3], "│ ▶   Preset 3                │");
    assert_eq!(slot[12], "│ u saves                     │");
}

/// `u` on a slot puts the question in a box of its own, with the keys that answer it.
#[test]
fn the_save_question_is_a_box_of_its_own() {
    let mut app = listing();
    for _ in 0..2 {
        press(&mut app, KeyCode::Up);
    }
    press(&mut app, KeyCode::Char('u'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the question is a modal box"));
    assert_eq!(
        rows,
        [
            "┌ Save ─────────────────────────────┐",
            "│ Save the bands to preset 3? [y/N] │",
            "│                                   │",
            "│ y saves · Enter/n/Esc cancels     │",
            "└───────────────────────────────────┘",
        ]
    );
}
