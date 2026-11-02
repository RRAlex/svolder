// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::tui::app::{App, Screen};
use crate::tui::mouse::{self, HitMap, MouseAction};
use crate::tui::test_support::{app_showing, click, drawn, mouse_at, ms, press, text};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets::spinner;

use super::*;

fn screen(width: u16, height: u16) -> Rect {
    Rect::new(0, 0, width, height)
}

/// An app whose clock reads `start`.
fn app_at(start: Instant) -> App {
    let mut app = app_showing(Screen::Device);
    app.tick(start);
    app
}

#[test]
fn an_info_toast_is_gone_at_three_seconds() {
    let start = Instant::now();
    let mut app = app_at(start);
    app.notify("no player");
    app.tick(start + ms(2_999));
    assert!(app.toast().is_some());
    app.tick(start + ms(3_000));
    assert!(app.toast().is_none());
}

#[test]
fn a_failure_toast_is_still_there_at_three_seconds_and_gone_at_six() {
    let start = Instant::now();
    let mut app = app_at(start);
    app.warn("settings not saved");
    app.tick(start + ms(3_000));
    assert_eq!(
        app.toast().map(|toast| toast.kind),
        Some(ToastKind::Failure)
    );
    app.tick(start + ms(5_999));
    assert!(app.toast().is_some());
    app.tick(start + ms(6_000));
    assert!(app.toast().is_none());
}

#[test]
fn a_wait_toast_stays_while_it_is_said_again_and_goes_half_a_second_after() {
    let start = Instant::now();
    let mut app = app_at(start);
    for step in 0..20 {
        let now = start + ms(step * 400);
        app.tick(now);
        app.waiting("⠋ opening Kraken…");
        assert!(app.toast().is_some(), "step {step}");
    }
    let last = start + ms(19 * 400);
    app.tick(last + ms(499));
    assert!(app.toast().is_some());
    app.tick(last + ms(500));
    assert!(app.toast().is_none());
}

#[test]
fn end_wait_clears_a_wait_toast_and_leaves_any_other() {
    let start = Instant::now();
    let mut app = app_at(start);
    app.waiting("⠋ opening Kraken…");
    app.end_wait();
    assert!(app.toast().is_none());
    app.warn("link lost");
    app.end_wait();
    assert_eq!(
        app.toast().map(|toast| toast.kind),
        Some(ToastKind::Failure)
    );
    app.notify("no player");
    app.end_wait();
    assert_eq!(app.toast().map(|toast| toast.kind), Some(ToastKind::Info));
}

#[test]
fn a_newer_toast_of_any_kind_replaces_the_one_showing() {
    let mut app = app_at(Instant::now());
    app.warn("first");
    app.notify("second");
    assert_eq!(
        app.toast(),
        Some(&Toast {
            text: "second".into(),
            kind: ToastKind::Info,
            lasts: None,
        })
    );
    app.waiting("third");
    assert_eq!(app.toast().map(|toast| toast.text.as_str()), Some("third"));
    app.warn("fourth");
    assert_eq!(
        app.toast().map(|toast| toast.kind),
        Some(ToastKind::Failure)
    );
}

#[test]
fn a_failure_keeps_its_advice_and_is_a_failure_toast() {
    let mut app = app_at(Instant::now());
    app.notify_failure(&crate::recovery::Failure::of(
        crate::recovery::Kind::Dropped,
        "device did not come back",
    ));
    assert_eq!(
        app.toast().map(|toast| toast.kind),
        Some(ToastKind::Failure)
    );
    assert!(app.notices.advice.is_some());
}

#[test]
fn the_box_sits_in_the_bottom_right_corner_at_80_by_24() {
    let (rect, lines) = area(screen(80, 24), "no player");
    assert_eq!(lines, vec!["no player".to_string()]);
    assert_eq!(rect.height, 3);
    assert_eq!(rect.bottom(), 24, "the bottom border is on the last row");
    assert_eq!(rect.right() - 1, 77, "the right border is two cells in");
    assert_eq!(rect.width, 9 + 4);
}

#[test]
fn the_box_is_at_most_sixty_percent_of_the_width() {
    let long = "x".repeat(300);
    let (rect, _) = area(screen(80, 24), &long);
    assert_eq!(rect.width, 48);
    let (rect, _) = area(screen(200, 50), &long);
    assert_eq!(rect.width, 120);
    let words = "word ".repeat(30);
    let (rect, _) = area(screen(80, 24), &words);
    assert!(rect.width <= 48);
}

#[test]
fn a_long_text_wraps_at_spaces_and_the_third_line_ends_in_an_ellipsis() {
    let (rect, lines) = area(
        screen(80, 24),
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta",
    );
    assert_eq!(lines.len(), 3);
    assert_eq!(rect.height, 5);
    assert!(
        lines.iter().all(|line| Span::raw(line).width() <= 44),
        "{lines:?}"
    );
    assert!(lines[2].ends_with('…'), "{lines:?}");
    assert!(!lines[0].ends_with(' ') && lines[0].starts_with("alpha"));
    // Text that wraps onto three lines and fits is not cut.
    let (_, lines) = area(
        screen(80, 24),
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi",
    );
    assert_eq!(lines.len(), 2);
    assert!(!lines.iter().any(|line| line.ends_with('…')));
}

#[test]
fn a_resize_lays_the_next_frame_out_for_the_new_size() {
    let mut app = app_at(Instant::now());
    app.notify("no player");
    for (width, height) in [(80, 24), (100, 30), (80, 24)] {
        let (buffer, hits) = drawn(&app, width, height);
        let (rect, _) = area(screen(width, height), "no player");
        assert_eq!(hits.area_of(Hit::Toast), Some(rect), "{width}x{height}");
        assert!(text(&buffer)[usize::from(height - 2)].contains("no player"));
        assert_eq!(rect.right() - 1, width - 3);
    }
}

#[test]
fn the_border_takes_the_kind_s_role_and_the_text_is_text() {
    let theme = Theme::new(false);
    let mut app = app_at(Instant::now());
    for (kind, role) in [
        (ToastKind::Info, Role::Cursor),
        (ToastKind::Failure, Role::Warn),
        (ToastKind::Wait, Role::Dim),
    ] {
        match kind {
            ToastKind::Info => app.notify("hello"),
            ToastKind::Failure => app.warn("hello"),
            ToastKind::Wait => app.waiting("hello"),
        }
        let (buffer, _) = drawn(&app, 80, 24);
        let (rect, _) = area(screen(80, 24), "hello");
        let corner = buffer.cell((rect.x, rect.y)).expect("a corner");
        let ground = theme.style(Role::Text);
        let border = ground.patch(theme.style(role));
        assert_eq!(corner.fg, border.fg.unwrap_or_default(), "{kind:?}");
        assert!(corner.modifier.contains(border.add_modifier), "{kind:?}");
        let letter = buffer.cell((rect.x + 2, rect.y + 1)).expect("a letter");
        assert_eq!(letter.symbol(), "h");
        assert_eq!(letter.fg, ground.fg.unwrap_or_default());
        assert!(!letter.modifier.contains(Modifier::DIM));
    }
}

#[test]
fn a_wait_toast_reads_the_spinner_glyph_and_the_words() {
    let mut app = app_at(Instant::now());
    app.waiting(spinner::before('⠋', "opening Kraken…"));
    let (buffer, _) = drawn(&app, 80, 24);
    let row = &text(&buffer)[22];
    assert!(row.contains("│ ⠋ opening Kraken… │"), "{row}");
}

#[test]
fn a_toast_over_the_help_overlay_is_not_dimmed() {
    let theme = Theme::new(false);
    let mut app = app_at(Instant::now());
    app.overlays.help_open = true;
    app.warn("settings not saved: disk full");
    let (buffer, _) = drawn(&app, 80, 24);
    let (rect, _) = area(screen(80, 24), "settings not saved: disk full");
    // The screen round the overlay is dimmed, so the toast's cells being full strength means
    // something.
    let outside = buffer.cell((0, 23)).expect("a cell outside the overlay");
    assert!(outside.modifier.contains(Modifier::DIM));
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            let cell = buffer.cell((x, y)).expect("a cell");
            assert!(
                !cell.modifier.contains(Modifier::DIM),
                "({x},{y}) is dimmed"
            );
        }
    }
    let ground = theme.style(Role::Text);
    let border = ground.patch(theme.style(Role::Warn));
    let corner = buffer.cell((rect.x, rect.y)).expect("the corner");
    assert_eq!(corner.fg, border.fg.unwrap_or_default());
    assert!(text(&buffer)[22].contains("settings not saved: disk full"));
}

#[test]
fn escape_goes_to_the_device_screen_and_the_toast_stays() {
    let mut app = app_at(Instant::now());
    app.screen = Screen::Telemetry;
    app.notify("210.1 is never written to");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Device);
    assert!(app.toast().is_some());
}

#[test]
fn a_click_on_the_toast_dismisses_it_and_reaches_nothing_under_it() {
    let mut app = app_at(Instant::now());
    app.notify("no player");
    let (_, hits) = drawn(&app, 80, 24);
    let rect = hits.area_of(Hit::Toast).expect("the toast is a target");
    let action = mouse::resolve(&hits, click(rect.x + 1, rect.y + 1), &app);
    assert_eq!(action, Some(MouseAction::DismissToast));
    // Over the help overlay it dismisses the toast and the overlay stays.
    app.overlays.help_open = true;
    let (_, hits) = drawn(&app, 80, 24);
    let action = mouse::resolve(&hits, click(rect.x + 1, rect.y + 1), &app);
    assert_eq!(action, Some(MouseAction::DismissToast));
    // A click elsewhere is not the toast's.
    let elsewhere = mouse::resolve(&hits, click(1, 1), &app);
    assert_eq!(elsewhere, Some(MouseAction::Key(KeyCode::Esc)));
}

fn mouse_press(button: MouseButton, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(button),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// An app on the device screen with a toast showing, and the frame's hits.
fn toasted() -> (App, HitMap, Rect) {
    let mut app = app_at(Instant::now());
    app.notify("no player");
    let (_, hits) = drawn(&app, 80, 24);
    let rect = hits.area_of(Hit::Toast).expect("the toast is a target");
    (app, hits, rect)
}

#[test]
fn a_right_press_and_the_wheel_over_the_toast_do_nothing() {
    let (app, hits, rect) = toasted();
    let (x, y) = (rect.x + 1, rect.y + 1);
    assert_eq!(
        mouse::resolve(&hits, mouse_press(MouseButton::Right, x, y), &app),
        None
    );
    assert_eq!(
        mouse::resolve(&hits, mouse_press(MouseButton::Middle, x, y), &app),
        None
    );
    for kind in [MouseEventKind::ScrollUp, MouseEventKind::ScrollDown] {
        assert_eq!(mouse::resolve(&hits, mouse_at(kind, x, y), &app), None);
    }
}

#[test]
fn a_drag_and_its_release_pass_through_a_toast() {
    let mut app = app_showing(Screen::Tone);
    app.notify("no player");
    crate::tui::app::curve_drag::start(
        &mut app,
        0,
        crate::tui::app::curve_drag::Gesture::Move,
        (5, 5),
    );
    let curve = Rect::new(0, 2, 80, 20);
    let mut with = HitMap::default();
    let mut without = HitMap::default();
    for hits in [&mut with, &mut without] {
        hits.push(curve, Hit::Curve);
    }
    let (x, y) = (74, 22);
    with.push(Rect::new(70, 21, 8, 3), Hit::Toast);
    for kind in [
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let through = mouse::resolve(&with, mouse_at(kind, x, y), &app);
        assert!(through.is_some(), "{kind:?}");
        assert_eq!(
            through,
            mouse::resolve(&without, mouse_at(kind, x, y), &app)
        );
    }
}

#[test]
fn the_dismissing_click_makes_no_double_click_with_the_next_press() {
    let (mut app, hits, rect) = toasted();
    let at = (rect.x + 1, rect.y + 1);
    let now = Instant::now();
    let first = mouse_press(MouseButton::Left, at.0, at.1);
    mouse::note_press(&mut app, &first, now);
    assert_eq!(
        mouse::resolve(&hits, first, &app),
        Some(MouseAction::DismissToast)
    );
    app.dismiss_toast();
    mouse::note_press(&mut app, &first, now + ms(100));
    assert!(!app.pointer.double_click);
}

#[test]
fn a_click_on_the_toast_dismisses_it_over_the_other_boxes() {
    for open in ["quit", "presets"] {
        let mut app = app_at(Instant::now());
        match open {
            "quit" => app.overlays.quitting = true,
            _ => {
                app = app_showing(Screen::Tone);
                press(&mut app, KeyCode::Char('p'));
                assert!(app.presets.preset_list.is_some());
            }
        }
        app.warn("settings not saved: disk full");
        let (_, hits) = drawn(&app, 80, 24);
        let rect = hits.area_of(Hit::Toast).expect("the toast is a target");
        assert_eq!(
            mouse::resolve(&hits, click(rect.x + 1, rect.y + 1), &app),
            Some(MouseAction::DismissToast),
            "{open}"
        );
        assert_eq!(
            mouse::resolve(&hits, click(1, 1), &app),
            Some(MouseAction::Key(KeyCode::Esc)),
            "{open}"
        );
    }
}

/// In the wide tier the toast's right edge is the body's, which is capped and centred, not the
/// terminal's; below it the body is the whole width and nothing moves.
#[test]
fn the_toast_keeps_to_the_body_in_the_wide_tier() {
    for (width, height) in [(80, 24), (100, 30), (120, 40), (200, 50), (300, 50)] {
        let mut app = app_at(Instant::now());
        app.notify("no player");
        let (_, hits) = drawn(&app, width, height);
        let rect = hits.area_of(Hit::Toast).expect("the toast is drawn");
        let (_, [_, body]) = crate::tui::layout_tier::split(screen(width, height));
        assert_eq!(
            rect.right() + RIGHT_MARGIN,
            body.right(),
            "{width}x{height}"
        );
        assert_eq!(rect.bottom(), height, "{width}x{height}");
    }
    let (_, [_, body]) = crate::tui::layout_tier::split(screen(300, 50));
    assert!(body.right() < 300, "the body is capped at 256 columns");
}

/// The note for a daemon reading another file wraps onto three lines at 80 columns with
/// nothing cut, the daemon's path before this session's.
#[test]
fn the_config_mismatch_note_fits_whole_at_80_columns_daemon_path_first() {
    let text = "svolderd reads /tmp/elsewhere/config/svolder/config.toml, not ~/.config/svolder/config.toml";
    let (_, lines) = area(screen(80, 24), text);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines.join(" "), text);
    assert!(lines[1].starts_with("/tmp/elsewhere/config/svolder/config.toml,"));
}
