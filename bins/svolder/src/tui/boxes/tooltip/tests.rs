// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyModifiers, MouseButton};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use serde_json::json;
use svolder_ipc::{ColorDepth, TuiConfig};

use super::*;
use crate::tui::render;
use crate::tui::test_support::{app_showing, hear, mouse_at, theme_for, vo};
use crate::tui::theme::Theme;

/// One frame of `app` at 80×24 in `theme`: the cells, and where it put what.
fn drawn(app: &App, theme: &Theme) -> (Buffer, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|frame| {
            render::draw(frame, app, theme, &mut hits);
        })
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), hits)
}

/// The hits of the default frame of `app`.
fn hits_of(app: &App) -> HitMap {
    drawn(app, &Theme::new(false)).1
}

/// The lines of the tooltip to draw on a frame whose hits are `hits`: the one held, once shown
/// and while its target is still under its cell.
fn visible(app: &App, hits: &HitMap) -> Option<Vec<String>> {
    app.pointer.tooltip.filter(|tooltip| tooltip.shown)?;
    standing(app, hits)
}

/// A pointer move to `(x, y)`.
fn moved(x: u16, y: u16) -> MouseEvent {
    mouse_at(MouseEventKind::Moved, x, y)
}

/// The first cell of the rectangle the frame drew `hit` in.
fn cell_of(hits: &HitMap, hit: Hit) -> (u16, u16) {
    let area = hits
        .area_of(hit)
        .unwrap_or_else(|| panic!("{hit:?} is drawn"));
    (area.x, area.y)
}

/// An app on the device screen with the mouse on.
fn app() -> App {
    let mut app = app_showing(Screen::Device);
    app.mouse = true;
    app
}

/// What a tooltip shows on a rest of `after` on the cell `at` of `hits`, with the pointer
/// arriving at `t0`.
fn rest(app: &mut App, hits: &HitMap, at: (u16, u16), t0: Instant, after: Duration) -> bool {
    hover(app, hits, &moved(at.0, at.1), t0);
    app.tick(t0 + after);
    visible(app, hits).is_some()
}

#[test]
fn a_tooltip_shows_after_the_pointer_rests_seven_tenths_of_a_second() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    let t0 = Instant::now();

    assert!(!rest(&mut app, &hits, at, t0, Duration::ZERO));
    assert!(app.tooltip_pending());
    app.tick(t0 + Duration::from_millis(699));
    assert!(visible(&app, &hits).is_none(), "not before 0.7 s");
    app.tick(t0 + DELAY);
    assert_eq!(
        visible(&app, &hits),
        Some(vec!["EQ · the tone control · e".to_string()])
    );
    assert!(!app.tooltip_pending());
}

#[test]
fn moving_within_the_target_keeps_the_timer_and_the_tooltip() {
    let mut app = app();
    let hits = hits_of(&app);
    let area = hits.area_of(Hit::Tab(Screen::Tone)).expect("the EQ tab");
    let t0 = Instant::now();

    hover(&mut app, &hits, &moved(area.x, area.y), t0);
    hover(
        &mut app,
        &hits,
        &moved(area.x + 1, area.y),
        t0 + Duration::from_millis(400),
    );
    app.tick(t0 + DELAY);
    assert!(visible(&app, &hits).is_some(), "the timer ran from arrival");

    hover(
        &mut app,
        &hits,
        &moved(area.x, area.y),
        t0 + Duration::from_secs(2),
    );
    let held = app.pointer.tooltip.expect("it stays");
    assert!(held.shown);
    assert_eq!(
        held.at,
        (area.x + 1, area.y),
        "the box stays where it was put"
    );
}

#[test]
fn moving_straight_onto_another_target_shows_it_at_once() {
    let mut app = app();
    let hits = hits_of(&app);
    let t0 = Instant::now();
    assert!(rest(
        &mut app,
        &hits,
        cell_of(&hits, Hit::Tab(Screen::Tone)),
        t0,
        DELAY
    ));

    let other = cell_of(&hits, Hit::Tab(Screen::Telemetry));
    hover(&mut app, &hits, &moved(other.0, other.1), t0 + DELAY);
    assert_eq!(
        visible(&app, &hits),
        Some(vec!["Telemetry · every reading · t".to_string()])
    );
}

#[test]
fn moving_onto_another_target_while_pending_starts_the_wait_again() {
    let mut app = app();
    let hits = hits_of(&app);
    let t0 = Instant::now();
    hover(
        &mut app,
        &hits,
        &moved_to(&hits, Hit::Tab(Screen::Tone)),
        t0,
    );
    let later = t0 + Duration::from_millis(500);
    hover(
        &mut app,
        &hits,
        &moved_to(&hits, Hit::Tab(Screen::Telemetry)),
        later,
    );
    app.tick(t0 + DELAY);
    assert!(visible(&app, &hits).is_none());
    app.tick(later + DELAY);
    assert!(visible(&app, &hits).is_some());
}

/// A move onto the first cell of `hit`.
fn moved_to(hits: &HitMap, hit: Hit) -> MouseEvent {
    let (x, y) = cell_of(hits, hit);
    moved(x, y)
}

#[test]
fn moving_onto_nothing_hides_it() {
    let mut app = app();
    let hits = hits_of(&app);
    let t0 = Instant::now();
    assert!(rest(
        &mut app,
        &hits,
        cell_of(&hits, Hit::Tab(Screen::Tone)),
        t0,
        DELAY
    ));

    // The last cell of the first row is a blank of the header.
    hover(&mut app, &hits, &moved(0, 0), t0 + DELAY);
    assert!(app.pointer.tooltip.is_none());
}

#[test]
fn a_press_the_wheel_a_drag_and_a_key_hide_it() {
    let shown = |app: &mut App| {
        let hits = hits_of(app);
        let at = cell_of(&hits, Hit::Tab(Screen::Tone));
        assert!(rest(app, &hits, at, Instant::now(), DELAY));
        (hits, at)
    };
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ] {
        let mut app = app();
        let (hits, at) = shown(&mut app);
        hover(&mut app, &hits, &mouse_at(kind, at.0, at.1), Instant::now());
        assert!(app.pointer.tooltip.is_none(), "{kind:?} hides it");
    }
    let mut app = app();
    shown(&mut app);
    app.key(crossterm::event::KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    ));
    assert!(app.pointer.tooltip.is_none(), "a key hides it");
}

#[test]
fn a_resize_hides_it() {
    let mut app = app();
    app.resize(80, 24);
    let hits = hits_of(&app);
    assert!(rest(
        &mut app,
        &hits,
        cell_of(&hits, Hit::Tab(Screen::Tone)),
        Instant::now(),
        DELAY
    ));
    app.resize(80, 24);
    assert!(app.pointer.tooltip.is_some(), "the same size is no resize");
    app.resize(100, 30);
    assert!(app.pointer.tooltip.is_none());
}

#[test]
fn a_box_opening_hides_it_and_none_shows_while_one_is_open() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));

    app.overlays.help_open = true;
    assert!(visible(&app, &hits).is_none(), "not while the box is open");
    let stands = standing(&app, &hits).is_some();
    settle(&mut app, stands);
    assert!(app.pointer.tooltip.is_none(), "the next frame drops it");

    hover(&mut app, &hits, &moved(at.0, at.1), Instant::now());
    assert!(
        app.pointer.tooltip.is_none(),
        "none starts while it is open"
    );
}

#[test]
fn none_shows_with_mouse_capture_off() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));

    app.mouse = false;
    assert!(visible(&app, &hits).is_none());
    let stands = standing(&app, &hits).is_some();
    settle(&mut app, stands);
    assert!(app.pointer.tooltip.is_none());
    hover(&mut app, &hits, &moved(at.0, at.1), Instant::now());
    assert!(app.pointer.tooltip.is_none());
}

#[test]
fn a_tooltip_whose_target_moved_away_hides_on_the_next_frame() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));

    // The screen changes under the pointer: the same cell now draws something else.
    app.show(Screen::Telemetry);
    let next = hits_of(&app);
    assert!(
        visible(&app, &next).is_none(),
        "nothing floats over the new screen"
    );
    let (buffer, _) = drawn(&app, &Theme::new(false));
    assert!(
        !crate::tui::test_support::text(&buffer)
            .join("\n")
            .contains("the tone control"),
        "no tooltip is drawn"
    );
    let stands = standing(&app, &next).is_some();
    settle(&mut app, stands);
    assert!(app.pointer.tooltip.is_none());
}

#[test]
fn a_toast_over_the_target_hides_the_tooltip() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));

    // A toast whose box covers the cell takes the hit.
    let mut covered = HitMap::default();
    covered.push(Rect::new(0, 0, 80, 24), Hit::Toast);
    assert!(visible(&app, &covered).is_none());
    let stands = standing(&app, &covered).is_some();
    settle(&mut app, stands);
    assert!(app.pointer.tooltip.is_none());
}

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|line| (*line).to_string()).collect()
}

#[test]
fn the_box_goes_under_the_pointer() {
    let screen = Rect::new(0, 0, 80, 24);
    let rect = area(
        screen,
        (10, 5),
        &lines(&["Band 1", "double-click a value to edit it"]),
    );
    assert_eq!(rect, Rect::new(10, 6, 33, 2));
}

#[test]
fn the_box_flips_above_near_the_bottom_and_shifts_left_at_the_right_edge() {
    let screen = Rect::new(0, 0, 80, 24);
    let two = lines(&["Mode", "a click toggles · ‹ › steps it"]);
    let width = 2 + 30;

    // The last row: the box ends on the row above the pointer.
    assert_eq!(area(screen, (10, 23), &two), Rect::new(10, 21, width, 2));
    // The second to last row has room for one row below, not two.
    assert_eq!(area(screen, (10, 22), &two), Rect::new(10, 20, width, 2));
    assert_eq!(area(screen, (10, 21), &two), Rect::new(10, 22, width, 2));
    // The right edge: the box ends on the last column.
    assert_eq!(
        area(screen, (79, 5), &two),
        Rect::new(80 - width, 6, width, 2)
    );
    assert_eq!(area(screen, (50, 5), &two), Rect::new(48, 6, width, 2));
}

#[test]
fn the_box_stays_on_the_screen_at_every_edge_and_corner() {
    let screen = Rect::new(0, 0, 80, 24);
    let two = lines(&["Response curve", "drag · right-drag widens · wheel sets Q"]);
    for x in [0, 1, 40, 78, 79] {
        for y in [0, 1, 12, 22, 23] {
            let rect = area(screen, (x, y), &two);
            assert!(
                rect.right() <= 80 && rect.bottom() <= 24,
                "({x}, {y}) gives {rect:?}"
            );
            let on_pointer = x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom();
            assert!(!on_pointer, "({x}, {y}) is covered by {rect:?}");
        }
    }
}

/// The background and modifiers a tooltip's first cell has, drawn in `theme`.
fn look_in(theme: &Theme) -> (Color, ratatui::style::Modifier, Color) {
    let screen = Rect::new(0, 0, 80, 24);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let text = lines(&["connected"]);
    terminal
        .draw(|frame| {
            draw(frame, screen, (10, 5), &text, theme, None);
        })
        .expect("the frame draws");
    let cell = terminal
        .backend()
        .buffer()
        .cell((11, 6))
        .expect("a cell")
        .clone();
    let border = terminal
        .backend()
        .buffer()
        .cell((10, 6))
        .expect("a cell")
        .bg;
    assert_eq!(border, cell.bg, "no border: the box is one shade across");
    (cell.bg, cell.modifier, cell.fg)
}

#[test]
fn the_look_is_the_raised_shade_with_text_and_reverse_video_where_none_is_painted() {
    let truecolor = theme_for("amber", ColorDepth::TrueColor);
    let raised = truecolor
        .surface(Surface::Raised)
        .expect("a raised shade at truecolor");
    let (bg, modifier, fg) = look_in(&truecolor);
    assert_eq!(bg, raised);
    assert!(!modifier.contains(Modifier::REVERSED));
    assert_eq!(fg, truecolor.style(Role::Text).fg.unwrap_or(Color::Reset));

    for (what, theme) in [
        ("16 colours", theme_for("amber", ColorDepth::Ansi16)),
        (
            "monochrome",
            Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor),
        ),
        (
            "the theme background off",
            Theme::from_config(
                &TuiConfig {
                    theme_background: false,
                    ..TuiConfig::default()
                },
                false,
                ColorDepth::TrueColor,
            ),
        ),
    ] {
        let (bg, modifier, _) = look_in(&theme);
        assert!(modifier.contains(Modifier::REVERSED), "{what}");
        assert_eq!(bg, Color::Reset, "{what}");
    }
}

#[test]
fn the_tooltip_and_the_toast_are_both_reported_on_one_frame() {
    let mut app = app();
    app.notify("a toast over the corner");
    let hits = hits_of(&app);
    let mark = cell_of(&hits, Hit::MouseMark);
    app.pointer.tooltip = Some(Tooltip {
        hit: Hit::MouseMark,
        at: mark,
        since: Instant::now(),
        shown: true,
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, &app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    assert!(areas.toast.is_some());
    let rect = areas.tooltip.expect("the tooltip");
    assert_eq!(rect.y, mark.1 + 1);
}

#[test]
fn a_shown_tooltip_is_drawn_and_reported() {
    let mut app = app();
    let hits = hits_of(&app);
    let at = cell_of(&hits, Hit::Tab(Screen::Tone));
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, &app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    let rect = areas.tooltip.expect("the tooltip's box is reported");
    assert_eq!(rect.y, at.1 + 1);
    let row: String = (rect.x..rect.right())
        .map(|x| {
            terminal
                .backend()
                .buffer()
                .cell((x, rect.y))
                .expect("cell")
                .symbol()
                .to_string()
        })
        .collect();
    assert_eq!(row.trim(), "EQ · the tone control · e");
}

/// Every tooltip target, on the frame that draws it, with the lines it says.
#[test]
fn every_target_has_its_text_within_forty_cells() {
    let mut app = app_showing(Screen::Device);
    app.mouse = true;
    hear(
        &mut app,
        vec![
            vo("5", Some("mute"), json!(false), "off"),
            vo("1", Some("volume"), json!(40), "-20 dB"),
        ],
    );
    let check = |hit: Hit, app: &App, expected: &[&str]| {
        let got = text(hit, app).unwrap_or_else(|| panic!("{hit:?} has a tooltip"));
        assert_eq!(got, lines(expected), "{hit:?}");
        for line in &got {
            assert!(widgets::text::cells(line) <= MAX_WIDTH, "{line:?}");
        }
    };
    check(
        Hit::Tab(Screen::Device),
        &app,
        &["Controls · the device screen · Esc"],
    );
    check(Hit::Tab(Screen::Tone), &app, &["EQ · the tone control · e"]);
    check(
        Hit::Tab(Screen::Telemetry),
        &app,
        &["Telemetry · every reading · t"],
    );
    check(
        Hit::Tab(Screen::Devices),
        &app,
        &["Devices · add, change, switch · d"],
    );
    check(Hit::HelpMark, &app, &["Every key, by screen · ?"]);
    check(Hit::EscMark, &app, &["Up one level · Esc"]);
    check(Hit::Key(KeyCode::Char('m')), &app, &["mute · m"]);
    check(Hit::Key(KeyCode::Char('r')), &app, &["refresh · r"]);
    check(Hit::Mode, &app, &["Mode", "a click toggles · ‹ › steps it"]);
    check(
        Hit::Loudness,
        &app,
        &["Loudness", "a click toggles · the wheel sets it"],
    );
    check(
        Hit::Band(1),
        &app,
        &["Band 2", "double-click a value to edit it"],
    );
    check(
        Hit::Curve,
        &app,
        &["Response curve", "drag · right-drag widens · wheel sets Q"],
    );
    check(
        Hit::Transport(1),
        &app,
        &["Play", "the player on this computer"],
    );
    app.session.connected = false;
    check(Hit::Connection, &app, &["link lost · r reopens"]);
    check(
        Hit::MouseMark,
        &app,
        &[
            "Mouse capture · M toggles it",
            "off, the terminal selects text",
        ],
    );

    let volume = app
        .rows()
        .iter()
        .position(|row| row.control.key == "volume")
        .expect("a volume row");
    let mute = app
        .rows()
        .iter()
        .position(|row| row.control.key == "mute")
        .expect("a mute row");
    let label = |at: usize| app.rows()[at].control.label.clone();
    let (volume_label, mute_label) = (label(volume), label(mute));
    check(
        Hit::Level(volume),
        &app,
        &[&volume_label, "click or drag · glides up above 75%"],
    );
    check(
        Hit::LevelBar(volume),
        &app,
        &[&volume_label, "click or drag · glides up above 75%"],
    );
    check(
        Hit::ToggleOf(mute),
        &app,
        &[&mute_label, "Space or a click flips it"],
    );
    check(
        Hit::Row(Pane::Controls, mute),
        &app,
        &[&mute_label, "Space or a click flips it"],
    );

    // A connected link reads `connected`.
    app.session.connected = true;
    check(Hit::Connection, &app, &["connected"]);

    // The other screens' rows.
    let mut telemetry = app_showing(Screen::Telemetry);
    telemetry.mouse = true;
    let got = text(Hit::ListRow(0), &telemetry).expect("a telemetry row");
    assert_eq!(got[1], "as the device last reported it");
    assert!(widgets::text::cells(&got[0]) <= MAX_WIDTH);
    let devices = app_showing(Screen::Devices);
    assert_eq!(
        text(Hit::ListRow(0), &devices),
        Some(lines(&[
            "gungnir",
            "Enter switches · c changes · x removes"
        ]))
    );
}

#[test]
fn a_control_row_says_what_its_kind_does() {
    let app = app();
    let kinds: Vec<(ControlKind, &str)> = vec![
        (ControlKind::Toggle, "Space or a click flips it"),
        (ControlKind::Select, "←→ or ‹ › steps it"),
        (
            ControlKind::Level,
            "click or drag the bar · ←→ or the wheel",
        ),
    ];
    for (kind, how) in kinds {
        let row = app
            .rows()
            .iter()
            .position(|row| row.control.kind == kind && row.control.key != "volume")
            .unwrap_or_else(|| panic!("a {kind:?} row"));
        let said = text(Hit::Row(Pane::Controls, row), &app).expect("a tooltip");
        assert_eq!(said[1], how, "{kind:?}");
        assert_eq!(said[0], app.rows()[row].control.label);
    }
    let any = app.rows()[0].control.clone();
    for (kind, how) in [
        (ControlKind::Readout, "read from the device"),
        (ControlKind::Fault, "a shows the advice"),
    ] {
        let said = control(&Control {
            kind,
            ..any.clone()
        })
        .expect("a tooltip");
        assert_eq!(said[1], how);
    }
}

#[test]
fn a_fault_mark_names_the_fault() {
    let mut app = crate::tui::test_support::app_showing(Screen::Device);
    app.profile = crate::tui::test_support::with_overcurrent(app.profile.clone());
    assert_eq!(text(Hit::FaultMark, &app), None, "no fault standing");
    hear(
        &mut app,
        vec![vo("9", Some("fault_overcurrent"), json!(true), "FAULT")],
    );
    if let Some(said) = text(Hit::FaultMark, &app) {
        assert_eq!(said[1], "a shows the advice");
        assert!(widgets::text::cells(&said[0]) <= MAX_WIDTH);
    }
}

#[test]
fn menu_entries_settings_rows_and_toasts_have_none() {
    let app = app();
    for hit in [
        Hit::MenuEntry(0),
        Hit::MenuOutside,
        Hit::SettingRow(0),
        Hit::SettingPrev,
        Hit::SettingNext,
        Hit::SettingCategory(0),
        Hit::Toast,
        Hit::Footer(0),
        Hit::PaneHeader(Pane::Controls),
    ] {
        assert_eq!(text(hit, &app), None, "{hit:?}");
    }
}

#[test]
fn the_header_draws_hits_for_the_dot_the_mouse_mark_and_a_fault() {
    for screen in [
        Screen::Device,
        Screen::Telemetry,
        Screen::Devices,
        Screen::Tone,
    ] {
        let mut app = app_showing(screen);
        app.mouse = true;
        let hits = hits_of(&app);
        let mark = hits
            .area_of(Hit::MouseMark)
            .unwrap_or_else(|| panic!("{screen:?} draws the mouse mark"));
        assert_eq!(mark.width, "mouse:on".len() as u16);
        assert_eq!(mark.y, 0, "{screen:?}");
        if screen != Screen::Tone {
            let dot = hits.area_of(Hit::Connection).expect("the connection words");
            assert!(dot.width > 0);
        }
    }
}

/// The frame docs/usage/tui.md shows of a tooltip, as far down as the box: the device screen's top
/// rows with the Telemetry tab's tooltip under the pointer.
fn tooltip_example() -> Vec<String> {
    let mut app = app();
    app.session.connected = true;
    let hits = hits_of(&app);
    let area = hits
        .area_of(Hit::Tab(Screen::Telemetry))
        .expect("the Telemetry tab");
    let at = (area.x + 2, area.y);
    assert!(rest(&mut app, &hits, at, Instant::now(), DELAY));
    let (buffer, _) = drawn(&app, &Theme::new(false));
    crate::tui::test_support::text(&buffer)
        .into_iter()
        .take(5)
        .collect()
}

/// The mockup docs/usage/tui.md shows of a tooltip is the frame this build draws.
#[test]
fn the_docs_show_the_tooltip_this_build_draws() {
    crate::tui::test_support::assert_usage_md_shows(&tooltip_example(), "tooltip");
}

/// The action a left click at `(x, y)` of `hits` resolves to.
fn clicked(app: &App, hits: &HitMap, (x, y): (u16, u16)) -> Option<crate::tui::mouse::MouseAction> {
    crate::tui::mouse::resolve(
        hits,
        mouse_at(MouseEventKind::Down(MouseButton::Left), x, y),
        app,
    )
}

#[test]
fn the_computer_volume_bar_claims_no_click_and_says_the_real_step() {
    let mut app = app();
    app.host.linked = true;
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "Sink".into(),
            target: None,
        }),
        true,
    );
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        false,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    app.flip_side();
    let row = crate::tui::test_support::row_of(&app, "volume");
    let hits = hits_of(&app);
    let label = app.rows()[row].control.label.clone();
    let said = text(Hit::Level(row), &app).expect("the computer volume tooltip");
    assert_eq!(
        said,
        lines(&[&label, "←→ or the wheel steps it 1 %", "Enter mutes it"])
    );
    // A click on the row only focuses it; the wheel steps the volume by one point.
    let at = cell_of(&hits, Hit::Level(row));
    assert!(matches!(
        clicked(&app, &hits, at),
        Some(crate::tui::mouse::MouseAction::Focus(Pane::Controls, r)) if r == row
    ));
    assert_eq!(
        app.nudge_row(row, 1),
        vec![crate::tui::app::Intent::HostVolume(55)]
    );
}

#[test]
fn a_box_that_meets_the_toast_goes_to_the_pointers_other_side_or_is_not_drawn() {
    let screen = Rect::new(0, 0, 80, 24);
    let two = lines(&["Mode", "a click toggles · ‹ › steps it"]);

    // No toast: the placement is the one `area` gives.
    assert_eq!(
        place(screen, (10, 5), &two, None),
        Some(area(screen, (10, 5), &two))
    );
    // A toast the box does not meet changes nothing.
    let far = Rect::new(40, 18, 30, 5);
    assert_eq!(
        place(screen, (10, 5), &two, Some(far)),
        Some(area(screen, (10, 5), &two))
    );
    // Below meets the toast: above the pointer instead.
    let toast = Rect::new(0, 6, 80, 3);
    assert_eq!(
        place(screen, (10, 5), &two, Some(toast)),
        Some(Rect::new(10, 3, 32, 2))
    );
    // The last rows put the box above; a toast there sends it below only if there is room, and
    // there is none.
    let low = Rect::new(0, 19, 80, 5);
    assert_eq!(place(screen, (10, 23), &two, Some(low)), None);
    // Both sides meet the toast: none.
    let tall = Rect::new(0, 3, 80, 5);
    assert_eq!(place(screen, (10, 5), &two, Some(tall)), None);
    // Above meets it, below does not (a pointer near the top, a toast over the rows above).
    assert_eq!(
        place(screen, (10, 1), &two, Some(Rect::new(0, 0, 80, 1))),
        Some(area(screen, (10, 1), &two))
    );
}

#[test]
fn a_drawn_tooltip_never_meets_the_toast_and_goes_above_where_it_must() {
    let screen = Rect::new(0, 0, 80, 24);
    let mut flipped = 0;
    for screen_shown in [Screen::Device, Screen::Tone, Screen::Telemetry] {
        let mut app = app_showing(screen_shown);
        app.mouse = true;
        app.notify("a toast over the corner of the screen, which wraps onto lines");
        let hits = hits_of(&app);
        let toast = hits.area_of(Hit::Toast).expect("the toast is drawn");
        for y in 0..24 {
            for x in 0..80 {
                let Some(hit) = hits.hit_at(x, y).copied() else {
                    continue;
                };
                let Some(said) = text(hit, &app) else {
                    continue;
                };
                let wanted = area(screen, (x, y), &said);
                app.pointer.tooltip = Some(Tooltip {
                    hit,
                    at: (x, y),
                    since: Instant::now(),
                    shown: true,
                });
                let mut terminal =
                    Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
                let mut drawn = HitMap::default();
                let mut areas = render::FrameAreas::default();
                terminal
                    .draw(|frame| {
                        areas = render::draw(frame, &app, &Theme::new(false), &mut drawn);
                    })
                    .expect("the frame draws");
                if let Some(rect) = areas.tooltip {
                    assert!(!rect.intersects(toast), "({x}, {y}) meets the toast");
                    if wanted.intersects(toast) {
                        assert!(rect.bottom() <= y, "({x}, {y}) goes above");
                        flipped += 1;
                    }
                }
            }
        }
    }
    assert!(flipped > 0, "some tooltip had to go to the other side");
}

/// The Loudness bar's tooltip says what a click on the bar does, and the word's says what a
/// click on it does.
#[test]
fn the_loudness_tooltips_match_their_clicks() {
    use crate::backend::DeviceRow;
    use crate::tui::app::glide::Bar;
    use crate::tui::mouse::MouseAction;
    use crate::tui::test_support::{hear_one, profile};

    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    hear_one(&mut app, "12.52", 10);
    app.mouse = true;
    let hits = hits_of(&app);
    assert_eq!(
        text(Hit::LoudnessBar, &app),
        Some(lines(&[
            "Loudness strength",
            "click or drag the bar · the wheel too"
        ]))
    );
    assert_eq!(
        text(Hit::Loudness, &app),
        Some(lines(&["Loudness", "a click toggles · the wheel sets it"]))
    );
    let bar = cell_of(&hits, Hit::LoudnessBar);
    assert!(matches!(
        clicked(&app, &hits, bar),
        Some(MouseAction::BarPress {
            bar: Bar::Loudness,
            ..
        })
    ));
    let word = cell_of(&hits, Hit::Loudness);
    assert!(matches!(
        clicked(&app, &hits, word),
        Some(MouseAction::ToggleTone(_))
    ));
}
