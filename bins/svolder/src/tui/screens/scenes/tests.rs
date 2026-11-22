// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Scenes box and the Status pane's `Scene` line as drawn, at the two sizes the mockups use.

use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use crate::tui::app::{App, Screen};
use crate::tui::mouse::{self, Hit, HitMap, MouseAction};
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::{
    app_showing, mockup_scenes, offering_scenes, press, scenes_view, text, true_colour, type_text,
};
use crate::tui::theme::Role;

/// An app on the device screen whose daemon keeps the mockups' scenes.
fn scenic() -> App {
    offering_scenes(app_showing(Screen::Device), mockup_scenes())
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

/// The rows of the box the frame drew, border to border, trimmed of the cells outside it.
fn box_rows(app: &App, width: u16, height: u16) -> Vec<String> {
    let (buffer, areas, _) = drawn(app, width, height);
    let area = areas.modal.expect("the Scenes box is a modal box");
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

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

const TOP: &str = "↵ recall ─ n new ─ u update ─ c rename ─ x delete ─┐";

#[test]
fn the_box_lists_every_scene_with_its_lamp_and_parts_at_80_by_24_and_120_by_40() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Down);
    for (width, height) in [(80, 24), (120, 40)] {
        let rows = box_rows(&app, width, height);
        let at = format!("{width}x{height}");
        assert!(rows[0].starts_with("┌ Scenes "), "{at}: {:?}", rows[0]);
        assert!(
            rows[0].ends_with(TOP),
            "{at}: the hints end with ─ and the corner stays"
        );
        assert!(
            rows[1].trim_matches(['│', ' ']).is_empty(),
            "{at}: a blank row opens the box"
        );
        assert!(
            rows[2].contains("●") && rows[2].contains("Late night  computer · Desk DAC"),
            "{at}: {:?}",
            rows[2]
        );
        assert!(
            rows[3].contains("▶ ○ Desk  ") && rows[3].contains("computer · Desk DAC"),
            "{at}: the cursor is on Desk: {:?}",
            rows[3]
        );
        assert!(
            rows[4].contains("○ Movie")
                && rows[4].contains("computer · Desk DAC · Hall (not connected)"),
            "{at}: {:?}",
            rows[4]
        );
        let last = &rows[rows.len() - 2];
        assert!(
            last.trim_matches(['│', ' ']).is_empty(),
            "{at}: the last row is free: {last:?}"
        );
        assert!(rows[rows.len() - 1].starts_with('└'), "{at}");
        assert!(
            rows.iter()
                .all(|row| row.chars().count() == rows[0].chars().count()),
            "{at}"
        );
    }
}

#[test]
fn only_the_scene_that_is_on_has_a_lit_lamp() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    let rows = box_rows(&app, 80, 24);
    let lamps = rows.iter().filter(|row| row.contains('●')).count();
    assert_eq!(lamps, 1, "{rows:#?}");
    assert!(rows.iter().filter(|row| row.contains('○')).count() == 2);
}

#[test]
fn while_the_daemon_has_not_settled_which_scene_is_on_no_lamp_is_lit() {
    let mut view = mockup_scenes();
    // A view that is not settled lights nothing, whatever `on` holds.
    view.known = false;
    let mut app = offering_scenes(app_showing(Screen::Device), view);
    press(&mut app, KeyCode::Char('z'));
    let rows = box_rows(&app, 80, 24);
    assert!(rows.iter().all(|row| !row.contains('●')), "{rows:#?}");
    // And the Status line reads `reading…`, never `none`.
    let (buffer, _, _) = drawn(&scenic_with(false, Some("Late night")), 80, 24);
    let screen = text(&buffer).join("\n");
    assert!(screen.contains("Scene  reading…"), "{screen}");
    assert!(!screen.contains("Scene  none"));
}

fn scenic_with(known: bool, on: Option<&str>) -> App {
    let mut view = mockup_scenes();
    view.known = known;
    view.on = on.map(str::to_string);
    offering_scenes(app_showing(Screen::Device), view)
}

#[test]
fn the_delete_ask_takes_the_boxes_last_row() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('x'));
    for (width, height) in [(80, 24), (120, 40)] {
        let rows = box_rows(&app, width, height);
        let last = &rows[rows.len() - 2];
        assert!(
            last.contains("Delete Movie? It is removed for every device. [y/N]"),
            "{width}x{height}: {last:?}"
        );
        assert!(rows[0].ends_with(TOP), "the hints stay on the border");
    }
}

#[test]
fn the_name_prompt_takes_the_boxes_last_row_with_its_keys_and_the_terminal_cursor() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Char('n'));
    for (width, height) in [(80, 24), (120, 40)] {
        let rows = box_rows(&app, width, height);
        let last = &rows[rows.len() - 2];
        assert!(last.contains("Save current as: Late night 2"), "{last:?}");
        assert!(last.contains("↵ save · esc cancel"), "{last:?}");
    }
    // The terminal's cursor stands right after the typed name.
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &true_colour(), &mut hits);
        })
        .expect("the frame draws");
    let cursor = terminal.get_cursor_position().expect("a cursor position");
    let screen = text(terminal.backend().buffer());
    let before: String = screen[usize::from(cursor.y)]
        .chars()
        .take(usize::from(cursor.x))
        .collect();
    assert!(
        before.ends_with("Save current as: Late night 2"),
        "{before:?}"
    );
}

#[test]
fn a_rename_prompt_holds_the_old_name_and_says_rename() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Char('c'));
    type_text(&mut app, "!");
    let rows = box_rows(&app, 80, 24);
    let last = &rows[rows.len() - 2];
    assert!(
        last.contains("Rename Late night to: Late night!"),
        "{last:?}"
    );
    assert!(last.contains("↵ rename · esc cancel"), "{last:?}");
}

#[test]
fn the_daemons_question_about_a_recall_is_the_last_row() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    app.scenes_done(crate::tui::boxes::scenes::SceneDone::Asked {
        op: svolder_ipc::socket::protocol::SceneOp::Recall {
            name: "Late night".into(),
            confirm: false,
        },
        message: "Late night leaves Warm on both: it plays twice.".into(),
    });
    let rows = box_rows(&app, 80, 24);
    let last = &rows[rows.len() - 2];
    assert!(last.contains("it plays twice. [y/N]"), "{last:?}");
}

#[test]
fn the_both_sides_question_is_wrapped_so_all_of_it_shows_before_y() {
    let question = "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway?";
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = scenic();
        press(&mut app, KeyCode::Char('z'));
        app.scenes_done(crate::tui::boxes::scenes::SceneDone::Asked {
            op: svolder_ipc::socket::protocol::SceneOp::Recall {
                name: "Late night".into(),
                confirm: false,
            },
            message: question.into(),
        });
        let rows = box_rows(&app, width, height);
        let inner: Vec<String> = rows[1..rows.len() - 1]
            .iter()
            .map(|row| row.trim_matches('│').trim().to_string())
            .collect();
        // The words of the question, in order, over the rows after the scenes, ending in [y/N].
        let joined = inner.join(" ");
        let squeezed: String = joined.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            squeezed.ends_with(&format!("{question} [y/N]")),
            "{width}x{height}: {rows:#?}"
        );
        assert!(!squeezed.contains('…'), "{width}x{height}: cut: {rows:#?}");
        assert!(
            rows.iter()
                .all(|row| row.chars().count() == rows[0].chars().count())
        );
    }
}

#[test]
fn the_rename_prompt_keeps_room_for_the_name_and_its_keys_with_a_long_old_name() {
    let long = "L".repeat(40);
    let view = scenes_view(&[(long.as_str(), &[("computer", true)])], None, true);
    let mut app = offering_scenes(app_showing(Screen::Device), view);
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Char('c'));
    let rows = box_rows(&app, 80, 24);
    let last = &rows[rows.len() - 2];
    assert!(last.contains("↵ rename · esc cancel"), "{last:?}");
    // The typed name (the old one, 40 letters) has at least 20 cells visible.
    let letters = last.chars().filter(|c| *c == 'L').count();
    assert!(letters >= 20, "{letters} of the name show: {last:?}");
}

#[test]
fn a_request_in_flight_is_said_in_the_last_row() {
    let mut app = scenic();
    press(&mut app, KeyCode::Char('z'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    let rows = box_rows(&app, 80, 24);
    assert!(
        rows[rows.len() - 2].contains("Recalling Desk…"),
        "{rows:#?}"
    );
}

#[test]
fn a_box_with_no_scene_says_how_to_save_one() {
    let mut app = offering_scenes(app_showing(Screen::Device), scenes_view(&[], None, true));
    press(&mut app, KeyCode::Char('z'));
    let rows = box_rows(&app, 80, 24);
    assert!(
        rows.iter()
            .any(|row| row.contains("no scenes yet: n saves")),
        "{rows:#?}"
    );
}

#[test]
fn many_scenes_scroll_with_the_cursor_and_say_which_way_the_rest_are() {
    let names: Vec<String> = (1..=30).map(|n| format!("Scene {n}")).collect();
    let rows: Vec<(&str, &[(&str, bool)])> = names
        .iter()
        .map(|name| (name.as_str(), &[("computer", true)][..]))
        .collect();
    let mut app = offering_scenes(app_showing(Screen::Device), scenes_view(&rows, None, true));
    press(&mut app, KeyCode::Char('z'));
    let first = box_rows(&app, 80, 24);
    assert!(
        first.last().is_some_and(|row| row.contains("more ↓")),
        "{first:#?}"
    );
    for _ in 0..29 {
        press(&mut app, KeyCode::Down);
    }
    let last = box_rows(&app, 80, 24);
    assert!(
        last[0].contains("more ↑")
            || last[0].ends_with(TOP)
            || last.iter().any(|row| row.contains("more ↑")),
        "{last:#?}"
    );
    assert!(last.iter().any(|row| row.contains("Scene 30")), "{last:#?}");
}

#[test]
fn a_click_focuses_a_row_and_a_click_on_the_focused_row_recalls() {
    let mut app = scenic();
    app.mouse = true;
    press(&mut app, KeyCode::Char('z'));
    let (_, _, hits) = drawn(&app, 80, 24);
    let (x, y) = coord_of(&hits, 1);
    assert_eq!(hits.hit_at(x, y), Some(&Hit::SceneRow(1)));
    assert_eq!(
        mouse::resolve(&hits, click(x, y), &app),
        Some(MouseAction::ScenePick(1))
    );
}

/// A cell of the row the hit map has for scene row `index`.
fn coord_of(hits: &HitMap, index: usize) -> (u16, u16) {
    for y in 0..24 {
        for x in 0..80 {
            if hits.hit_at(x, y) == Some(&Hit::SceneRow(index)) {
                return (x + 2, y);
            }
        }
    }
    panic!("no row {index}");
}

#[test]
fn a_click_outside_the_box_closes_it() {
    let mut app = scenic();
    app.mouse = true;
    press(&mut app, KeyCode::Char('z'));
    let (_, _, hits) = drawn(&app, 80, 24);
    assert_eq!(
        mouse::resolve(&hits, click(0, 0), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

#[test]
fn the_status_pane_names_the_scene_that_is_on_on_its_first_line() {
    let theme = true_colour();
    for (width, height) in [(80, 24), (120, 40)] {
        let (buffer, _, _) = drawn(&scenic(), width, height);
        let screen = text(&buffer);
        let status = screen
            .iter()
            .position(|row| row.contains("Status"))
            .expect("the Status pane");
        // The first line of the pane's text: the tall tier keeps a blank row under the border.
        let first = (status + 1..screen.len())
            .find(|at| !screen[*at].trim_matches(['│', ' ']).is_empty())
            .expect("a line in the pane");
        assert!(
            screen[first].contains("Scene  Late night"),
            "{width}x{height}: {:?}",
            screen[first]
        );
        // The label is the dim role.
        let x = screen[first]
            .chars()
            .position(|c| c == 'S')
            .expect("the label");
        assert_eq!(
            buffer[(u16::try_from(x).unwrap(), u16::try_from(first).unwrap())].fg,
            theme.style(Role::Dim).fg.expect("a dim colour")
        );
    }
}

#[test]
fn the_scene_line_reads_none_when_the_daemon_has_settled_on_no_scene() {
    let app = scenic_with(true, None);
    let (buffer, _, _) = drawn(&app, 80, 24);
    assert!(text(&buffer).join("\n").contains("Scene  none"));
}

#[test]
fn there_is_no_scene_line_while_scenes_are_not_offered() {
    let app = app_showing(Screen::Device);
    let (buffer, _, _) = drawn(&app, 80, 24);
    assert!(!text(&buffer).join("\n").contains("Scene  "));
}

#[test]
fn the_wrapped_question_shows_whole_over_many_scenes_at_80_by_24() {
    let question = "Late night leaves Warm on the Desk DAC and on the computer: it plays twice. Recall it anyway?";
    let names: Vec<String> = (1..=30).map(|n| format!("Scene {n}")).collect();
    let rows: Vec<(&str, &[(&str, bool)])> = names
        .iter()
        .map(|name| (name.as_str(), &[("computer", true)][..]))
        .collect();
    let mut app = offering_scenes(app_showing(Screen::Device), scenes_view(&rows, None, true));
    press(&mut app, KeyCode::Char('z'));
    app.scenes_done(crate::tui::boxes::scenes::SceneDone::Asked {
        op: svolder_ipc::socket::protocol::SceneOp::Recall {
            name: "Scene 1".into(),
            confirm: false,
        },
        message: question.into(),
    });
    let rows = box_rows(&app, 80, 24);
    let inner = rows[1..rows.len() - 1]
        .iter()
        .map(|row| row.trim_matches('│').trim().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let squeezed = inner.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        squeezed.ends_with(&format!("{question} [y/N]")),
        "{rows:#?}"
    );
    // The body under the header at 80x24 is 22 rows: the box takes no more.
    assert!(rows.len() <= 22, "the box fits the body: {}", rows.len());
}

#[test]
fn with_no_scene_saved_the_status_pane_has_no_scene_line_and_keeps_its_rows() {
    let plain = {
        let app = app_showing(Screen::Device);
        let (buffer, _, _) = drawn(&app, 80, 24);
        text(&buffer)
    };
    let none = offering_scenes(app_showing(Screen::Device), scenes_view(&[], None, true));
    let (buffer, _, _) = drawn(&none, 80, 24);
    let rows = text(&buffer);
    assert!(!rows.join("\n").contains("Scene  "), "{rows:#?}");
    assert_eq!(rows, plain, "the screen is the one a daemon-less app has");
    // The line comes with the first scene, and `reading…` while that scene's state is unsettled.
    let (buffer, _, _) = drawn(&scenic(), 80, 24);
    assert!(text(&buffer).join("\n").contains("Scene  Late night"));
    let unsettled = {
        let mut view = mockup_scenes();
        view.known = false;
        view.on = None;
        offering_scenes(app_showing(Screen::Device), view)
    };
    let (buffer, _, _) = drawn(&unsettled, 80, 24);
    assert!(text(&buffer).join("\n").contains("Scene  reading…"));
}
