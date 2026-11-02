// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use crate::tui::app::{App, Intent, Screen};
use crate::tui::mouse::{self, Hit, HitMap, MouseAction};
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::{app_showing, press, text, true_colour};
use crate::tui::theme::Role;

use super::*;

/// An app on the device screen with the menu open.
fn open() -> App {
    let mut app = app_showing(Screen::Device);
    press(&mut app, KeyCode::Esc);
    app
}

/// One frame of `app` at `width` by `height`.
fn drawn(app: &App, width: u16, height: u16) -> (Buffer, FrameAreas, HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, &true_colour(), &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas, hits)
}

/// The 11 rows of the box at 80×24, from its top border to its bottom one.
fn box_rows(app: &App) -> Vec<String> {
    let (buffer, areas, _) = drawn(app, 80, 24);
    let area = areas.modal.expect("the menu is a modal box");
    text(&buffer)[usize::from(area.top())..usize::from(area.bottom())]
        .iter()
        .map(|row| {
            row.chars()
                .skip(usize::from(area.x))
                .take(usize::from(area.width))
                .collect()
        })
        .collect()
}

fn left_click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// `Esc` on the device screen with nothing open opens the menu on Settings, and `Esc` in the
/// menu closes it.
#[test]
fn esc_on_the_device_screen_opens_the_menu_and_esc_closes_it() {
    let mut app = app_showing(Screen::Device);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(app.overlays.menu, Some(0));
    assert!(app.modal_open());
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.overlays.menu, None);
    assert_eq!(app.screen, Screen::Device);
}

/// `Esc` on the other screens goes to the device screen and opens no menu.
#[test]
fn esc_on_another_screen_goes_to_the_device_screen() {
    for screen in [Screen::Tone, Screen::Telemetry, Screen::Devices] {
        let mut app = app_showing(screen);
        press(&mut app, KeyCode::Esc);
        assert_eq!(
            (app.screen, app.overlays.menu),
            (Screen::Device, None),
            "{screen:?}"
        );
    }
}

/// With a box open, `Esc` closes that box and nothing else: no menu opens behind it.
#[test]
fn esc_with_a_box_open_closes_that_box_first() {
    let mut app = app_showing(Screen::Device);
    press(&mut app, KeyCode::Char('?'));
    press(&mut app, KeyCode::Esc);
    assert_eq!((app.overlays.help_open, app.overlays.menu), (false, None));

    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Esc);
    assert_eq!((app.overlays.settings, app.overlays.menu), (None, None));

    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Esc);
    assert_eq!((app.overlays.quitting, app.overlays.menu), (false, None));

    let mut app = app_showing(Screen::Tone);
    press(&mut app, KeyCode::Char('p'));
    assert!(app.presets.preset_list.is_some());
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        (app.presets.preset_list.is_some(), app.screen),
        (false, Screen::Tone)
    );
}

/// In the demo the row under the box holds the command that joins its daemon, centred; with no
/// demo the row is empty.
#[test]
fn the_demo_menu_shows_the_join_command_under_the_box() {
    let mut app = open();
    let (buffer, areas, _) = drawn(&app, 80, 24);
    let below = usize::from(areas.modal.expect("a modal box").bottom());
    assert!(!text(&buffer)[below].contains("Join with"));

    app.session.join_hint = Some("svolder tui --join /tmp/sd-abcdef/sv.sock".into());
    let (buffer, _, _) = drawn(&app, 80, 24);
    assert!(
        text(&buffer)[below].contains("Join with: svolder tui --join /tmp/sd-abcdef/sv.sock"),
        "{:?}",
        text(&buffer)[below]
    );
}

/// The join command takes only its own width under the box: the rest of that row keeps the
/// dimmed screen, its borders and its greys.
#[test]
fn the_join_command_leaves_the_rest_of_its_row_dimmed() {
    let mut app = open();
    let (plain, areas, _) = drawn(&app, 80, 24);
    let below = areas.modal.expect("a modal box").bottom();
    let command = "svolder tui --join /tmp/sd-abcdef/sv.sock";
    app.session.join_hint = Some(command.into());
    let (hinted, _, _) = drawn(&app, 80, 24);
    let width = u16::try_from(format!(" Join with: {command} ").len()).expect("a short label");
    let left = (80 - width) / 2;
    for x in (0..left).chain(left + width..80) {
        assert_eq!(hinted[(x, below)], plain[(x, below)], "column {x}");
    }
}

/// The box is 36×11 and centred at 80×24, with the menu's rows.
#[test]
fn the_box_is_36_by_11_and_centred_at_80_by_24() {
    let app = open();
    let (_, areas, _) = drawn(&app, 80, 24);
    let area = areas.modal.expect("a modal box");
    assert_eq!((area.x, area.y, area.width, area.height), (22, 6, 36, 11));
    let rows = box_rows(&app);
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    assert_eq!(
        rows,
        [
            "┌──────────────────────────────────┐".to_string(),
            "│                                  │".to_string(),
            "│  █▀▀▀ █  █ █▀▀█ █ ▄▄▄█ ████ █▀▀  │".to_string(),
            "│  ▄▄▄█  ██  █▄▄█ █ █▄▄█ █▄▄▄ █    │".to_string(),
            format!("│{version:>33} │"),
            "│                                  │".to_string(),
            "│             Settings             │".to_string(),
            "│               Help               │".to_string(),
            "│               Quit               │".to_string(),
            "│                                  │".to_string(),
            "└──────────────────────────────────┘".to_string(),
        ]
    );
}

/// The box is the same size at every tier, centred over the screen.
#[test]
fn the_box_keeps_its_size_in_the_wide_and_tall_tiers() {
    let app = open();
    for (width, height) in [(120, 24), (80, 40), (140, 50)] {
        let (_, areas, _) = drawn(&app, width, height);
        let area = areas.modal.expect("a modal box");
        assert_eq!(
            (area.x, area.y, area.width, area.height),
            ((width - 36) / 2, (height - 11) / 2, 36, 11),
            "{width}x{height}"
        );
    }
}

/// The version row reads `v` and the crate's version, dim, with the daemon's joining it dim
/// where it is the same and in the warn role where it differs. The longest of them fits.
#[test]
fn the_version_row_names_both_versions_and_warns_where_they_differ() {
    let own = env!("CARGO_PKG_VERSION");
    let theme = true_colour();
    let mut app = open();
    let line = version_line(&app, &theme);
    let words: Vec<String> = line.spans.iter().map(|s| s.content.to_string()).collect();
    assert_eq!(words, [format!("v{own}"), " ".to_string()]);
    assert_eq!(line.spans[0].style, theme.style(Role::Dim));

    app.session.daemon_version = Some(own.to_string());
    let line = version_line(&app, &theme);
    assert_eq!(line.spans[1].content, format!(" · daemon v{own}"));
    assert_eq!(line.spans[1].style, theme.style(Role::Dim));

    // Another build of the same version is no difference.
    app.session.daemon_version = Some(format!("{own}+build.5"));
    let line = version_line(&app, &theme);
    assert_eq!(line.spans[1].style, theme.style(Role::Dim));

    app.session.daemon_version = Some("0.0.1".to_string());
    let line = version_line(&app, &theme);
    assert_eq!(line.spans[1].content, " · daemon v0.0.1");
    assert_eq!(line.spans[1].style, theme.style(Role::Warn));

    app.session.daemon_version = Some("0.9.0".to_string());
    let rows = box_rows(&app);
    let both = format!("v{own} · daemon v0.9.0 ");
    assert!(rows[4].ends_with(&format!("{both}│")), "{}", rows[4]);
    assert!(rows[4].starts_with("│ "));
}

/// An entry's first letter is bold in the key-hint role and the rest is text; the entry under
/// the cursor is selected whole.
#[test]
fn entries_have_a_bold_first_letter_and_the_cursor_selects_one_whole() {
    let theme = true_colour();
    let mut app = open();
    app.overlays.menu = Some(1);
    let (buffer, areas, _) = drawn(&app, 80, 24);
    let area = areas.modal.expect("a modal box");
    let key = theme.style(Role::KeyHint);
    let plain = theme.style(Role::Text);
    let selected = theme.style(Role::Selected);
    let row = |at: u16| area.y + 6 + at;
    let cell = |x: u16, y: u16| buffer.cell((x, y)).expect("a cell").clone();

    // Settings starts at column 14 of the box's 36.
    let first = cell(area.x + 14, row(0));
    assert_eq!(first.symbol(), "S");
    assert_eq!(first.fg, key.fg.expect("a colour"));
    assert!(first.modifier.contains(Modifier::BOLD));
    let rest = cell(area.x + 15, row(0));
    assert_eq!(
        (rest.fg, rest.modifier.contains(Modifier::BOLD)),
        (plain.fg.expect("a colour"), false)
    );

    // Help is under the cursor: every letter selected, the first included.
    for (at, letter) in "Help".chars().enumerate() {
        let c = cell(area.x + 16 + at as u16, row(1));
        assert_eq!(c.symbol(), letter.to_string());
        assert_eq!(c.fg, selected.fg.expect("a colour"), "{letter}");
        assert!(
            c.modifier.contains(Modifier::REVERSED | Modifier::BOLD),
            "{letter}"
        );
    }
    let quit = cell(area.x + 16, row(2));
    assert_eq!(quit.fg, key.fg.expect("a colour"));
}

/// `↑↓` move the cursor and stop at the ends.
#[test]
fn the_arrows_move_the_cursor_and_stop_at_the_ends() {
    let mut app = open();
    press(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.menu, Some(0));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.overlays.menu, Some(2));
    press(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.menu, Some(1));
}

/// `Enter` and the entry's letter take the same entry: Settings opens the box, Help the
/// overlay, Quit the quit question — or quits at once with `confirm_quit` off.
#[test]
fn enter_and_the_letters_take_the_entries() {
    for (keys, check) in [
        (vec![KeyCode::Enter], 0),
        (vec![KeyCode::Char('s')], 0),
        (vec![KeyCode::Down, KeyCode::Enter], 1),
        (vec![KeyCode::Char('h')], 1),
        (vec![KeyCode::Char('?')], 1),
        (vec![KeyCode::Down, KeyCode::Down, KeyCode::Enter], 2),
        (vec![KeyCode::Char('q')], 2),
    ] {
        let mut app = open();
        for code in &keys {
            assert!(press(&mut app, *code).is_empty());
        }
        assert_eq!(app.overlays.menu, None, "{keys:?}");
        match check {
            0 => assert!(app.overlays.settings.is_some(), "{keys:?}"),
            1 => assert!(app.overlays.help_open, "{keys:?}"),
            _ => assert!(app.overlays.quitting, "{keys:?}"),
        }
    }
    for keys in [
        vec![KeyCode::Char('q')],
        vec![KeyCode::Down, KeyCode::Down, KeyCode::Enter],
    ] {
        let mut app = open();
        app.tui.confirm_quit = false;
        let mut last = Vec::new();
        for code in keys {
            last = press(&mut app, code);
        }
        assert_eq!(last, [Intent::Quit]);
    }
}

/// `Esc` closes the menu and Ctrl-C quits from it.
#[test]
fn esc_closes_the_menu_and_ctrl_c_quits() {
    let mut app = open();
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(app.overlays.menu, None);
    let mut app = open();
    let quit = app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(quit, [Intent::Quit]);
}

/// A key the menu does not take never reaches the screen under it: `m` does not mute, `r`
/// does not refresh, and the screen keys and the toggles do nothing.
#[test]
fn a_key_the_menu_does_not_take_never_reaches_the_screen() {
    for code in "mretdfMEHRFVa+0".chars() {
        let mut app = open();
        let before = (
            app.screen,
            app.selected,
            app.tui.clone(),
            app.notices.show_advice,
        );
        let intents = press(&mut app, KeyCode::Char(code));
        assert!(intents.is_empty(), "{code}: {intents:?}");
        assert_eq!(app.overlays.menu, Some(0), "{code}");
        assert_eq!(
            (
                app.screen,
                app.selected,
                app.tui.clone(),
                app.notices.show_advice
            ),
            before,
            "{code}"
        );
    }
}

/// A click on an entry takes it, and a click anywhere outside the box closes the menu.
#[test]
fn a_click_on_an_entry_takes_it_and_a_click_outside_closes_the_menu() {
    let app = open();
    let (_, areas, hits) = drawn(&app, 80, 24);
    let area = areas.modal.expect("a modal box");
    for (at, key) in [(0, 's'), (1, 'h'), (2, 'q')] {
        let (x, y) = (area.x + 18, area.y + 6 + at);
        assert_eq!(hits.hit_at(x, y), Some(&Hit::MenuEntry(usize::from(at))));
        assert_eq!(
            mouse::resolve(&hits, left_click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Char(key)))
        );
    }
    for (x, y) in [
        (0, 0),
        (79, 23),
        (area.x - 1, area.y + 3),
        (area.right(), area.y + 3),
        (40, area.bottom()),
    ] {
        assert_eq!(
            mouse::resolve(&hits, left_click(x, y), &app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "({x}, {y})"
        );
    }
    // A click on the box's own blank or mark cells does nothing.
    for (x, y) in [
        (area.x + 5, area.y + 2),
        (area.x + 5, area.y + 4),
        (area.x, area.y),
    ] {
        assert_eq!(
            mouse::resolve(&hits, left_click(x, y), &app),
            None,
            "({x}, {y})"
        );
    }
}

/// A resize below 80×24 draws the too-small screen with the menu still open, and the way back
/// draws the menu again, laid out for the size it has.
#[test]
fn a_resize_below_the_floor_and_back_keeps_the_menu_open() {
    let mut app = open();
    let before = box_rows(&app);
    app.resize(60, 20);
    let (buffer, areas, _) = drawn(&app, 60, 20);
    assert_eq!(areas.modal, None);
    assert!(text(&buffer).iter().any(|row| row.contains("80")));
    assert_eq!(app.overlays.menu, Some(0));
    app.resize(80, 24);
    assert_eq!(box_rows(&app), before);
    app.resize(120, 40);
    let (_, areas, _) = drawn(&app, 120, 40);
    let area = areas.modal.expect("the menu is still open");
    assert_eq!((area.x, area.y), (42, 14));
}

/// docs/usage/tui.md shows the menu at 80×24, byte for byte what this build draws.
#[test]
fn usage_md_shows_the_menu_this_build_draws() {
    let doc = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage/tui.md");
    let doc = std::fs::read_to_string(&doc).expect("docs/usage/tui.md reads");
    let lines: Vec<&str> = doc.lines().collect();
    let top = lines
        .iter()
        .position(|line| *line == "┌──────────────────────────────────┐")
        .expect("usage/tui.md shows the menu");
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let wanted = box_rows(&open());
    assert_eq!(wanted[4], format!("│{version:>33} │"));
    assert_eq!(lines[top..top + 11], wanted[..]);
}
