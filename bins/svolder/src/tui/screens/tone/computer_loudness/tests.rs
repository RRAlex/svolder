// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::tui::app::{App, Ld, Screen, ToneRow};
use crate::tui::mouse::{Hit, HitMap, MouseAction, resolve};
use crate::tui::render;
use crate::tui::test_support::{
    app_showing, computer_eq_with_crossfeed, computer_eq_with_loudness, frame, text,
};
use crate::tui::theme::Theme;

/// The worked example: strength 50 % of the reference 100 % at the volume 35 % plays depth 13.7
/// dB, which the view says as bass +7.3, mids −7.3 and treble +2.1.
/// The panel's readout, with the treble.
const READOUT_OPEN: &str = "at 35 %: bass +7.3 \u{b7} mids \u{2212}7.3 \u{b7} treble +2.1 dB";

fn app_on(screen: Screen, on: bool) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_loudness(
        on,
        (50.0, 100.0),
        Some(35.0),
        13.7,
    ));
    app.flip_side();
    if screen == Screen::Tone {
        app.show(Screen::Tone);
        app.tone.row = ToneRow::Mode;
    }
    app
}

/// [`app_on`] on the tone screen with the keys in the Loudness panel.
fn panel(on: bool) -> App {
    let mut app = app_on(Screen::Tone, on);
    app.focus_ld(Ld::On);
    app
}

fn rows(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
}

fn at(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?}:\n{}", rows.join("\n")))
}

#[test]
fn the_panel_has_the_on_strength_reference_and_now_rows_in_that_order() {
    for (width, height) in [(80, 24), (120, 40)] {
        let rows = rows(&panel(true), width, height);
        let top = at(&rows, "Loudness   \u{2039} on \u{203a}");
        assert!(
            rows[top].contains('\u{25b6}'),
            "the cursor marks the first row"
        );
        let strength = &rows[top + 1];
        assert!(
            strength.contains("Strength   \u{2039} 50 % \u{203a}"),
            "{strength}"
        );
        assert!(
            strength.contains("\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2591}"),
            "{strength}"
        );
        let reference = &rows[top + 2];
        assert!(
            reference.contains("Reference  \u{2039} 100 % \u{203a}")
                && reference.contains("the volume you mixed at"),
            "{reference}"
        );
        assert!(!reference.contains("ref 100"), "{reference}");
        let now = &rows[top + 3];
        assert!(now.contains("Now") && now.contains(READOUT_OPEN), "{now}");
        assert!(
            !rows.iter().any(|r| r.contains("depth")),
            "no jargon: {rows:#?}"
        );
        // the chain's order: the Response graph, the strip, then the panel
        assert!(at(&rows, "Response") < at(&rows, "EQ \u{25cf}"));
        assert!(at(&rows, "EQ \u{25cf}") < top);
    }
}

#[test]
fn off_the_row_says_off_and_the_now_row_says_what_off_does() {
    let rows = rows(&panel(false), 80, 24);
    let top = at(&rows, "Loudness   \u{2039} off \u{203a}");
    assert!(rows[top + 1].contains("50 %"), "{}", rows[top + 1]);
    assert!(
        rows[top + 3].contains("off: the sound plays as the EQ made it"),
        "{rows:#?}"
    );
}

#[test]
fn the_dac_hint_shows_on_the_tone_screen_and_the_pane_only_while_the_dacs_loudness_is_on() {
    let mut app = panel(true);
    let wide = rows(&app, 160, 40);
    assert!(
        !wide.iter().any(|r| r.contains("DAC's loudness")),
        "{wide:#?}"
    );
    // the device's own mode, as its reading says it
    crate::tui::test_support::hear_one(&mut app, "12.50", 3);
    assert!(app.dac_loudness_on());
    let wide = rows(&app, 160, 40);
    let dim = &wide[at(&wide, "bass +7.3")];
    assert!(dim.contains("the DAC's loudness is on too"), "{dim}");
    // a narrow tone screen keeps the numbers and drops the hint
    let narrow = rows(&app, 80, 24);
    assert!(narrow[at(&narrow, "bass +7.3")].contains("mids"));
    // the pane says it on a line of its own, at every width
    let mut pane = app_on(Screen::Device, true);
    crate::tui::test_support::hear_one(&mut pane, "12.50", 3);
    for (width, height) in [(80, 24), (120, 40), (200, 50)] {
        let rows = rows(&pane, width, height);
        // once, within the two lines under the row: after the numbers where the line is wide
        // enough for both, else on a line of its own
        let top = at(&rows, "Loudness  \u{2039}");
        let said: Vec<usize> = (0..rows.len())
            .filter(|&at| rows[at].contains("the DAC's loudness is on too"))
            .collect();
        assert_eq!(said.len(), 1, "{width}: {said:?}");
        assert!(
            (top + 1..=top + 2).contains(&said[0]),
            "{width}: {said:?} under {top}"
        );
        assert!(
            rows[top + 1].contains("bass +7.3"),
            "{width}: the numbers stay"
        );
    }
}

#[test]
fn nothing_is_drawn_where_the_view_has_no_loudness() {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_crossfeed(false, "bauer", None));
    app.flip_side();
    app.show(Screen::Tone);
    for (width, height) in [(80, 24), (120, 40)] {
        let rows = rows(&app, width, height);
        assert!(!rows.iter().any(|r| r.contains("Loudness")), "{rows:#?}");
    }
    app.show(Screen::Device);
    let rows = rows(&app, 120, 40);
    assert!(!rows.iter().any(|r| r.contains("Loudness")), "{rows:#?}");
}

#[test]
fn the_device_pane_has_the_row_over_the_crossfeed_row_with_the_picked_dim_line() {
    for (width, height, dim) in [
        (80, 24, "at 35 %: bass +7.3 \u{b7} mids \u{2212}7.3 dB"),
        (120, 40, "at 35 %: bass +7.3 \u{b7} mids \u{2212}7.3 dB"),
    ] {
        let rows = rows(&app_on(Screen::Device, true), width, height);
        let line = &rows[at(&rows, "Loudness  \u{2039}")];
        assert!(line.contains("Loudness  \u{2039} on \u{203a} "), "{line}");
        assert!(line.contains("50 %"), "{line}");
        // indented under the cursor only, and nothing of treble
        let under = &rows[at(&rows, "Loudness  \u{2039}") + 1];
        let col = |row: &str, needle: &str| row.find(needle).map(|b| row[..b].chars().count());
        assert!(under.contains(dim), "{width}: {under}");
        assert_eq!(
            col(under, &dim[..4]),
            col(line, "Loudness"),
            "{width}: the text starts under the label, indented by the cursor only"
        );
        assert!(!under.contains("treble"), "{under}");
        assert!(at(&rows, "Loudness  \u{2039}") < at(&rows, "Crossfeed"));
        assert!(
            at(&rows, "LSC") < at(&rows, "Loudness  \u{2039}"),
            "after the points"
        );
    }
}

fn click(at: (u16, u16), kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: at.0,
        row: at.1,
        modifiers: KeyModifiers::NONE,
    }
}

fn drawn(app: &App, width: u16, height: u16) -> (HitMap, Vec<String>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    (hits, text(terminal.backend().buffer()))
}

/// The cell of `needle` on the row that has `row_has`, as a column and a row.
fn cell(rows: &[String], row_has: &str, needle: &str) -> (u16, u16) {
    let y = at(rows, row_has);
    let byte = rows[y].find(needle).expect("the needle");
    (
        u16::try_from(rows[y][..byte].chars().count()).unwrap(),
        u16::try_from(y).unwrap(),
    )
}

#[test]
fn the_parts_of_the_device_panes_row_are_hits() {
    let app = app_on(Screen::Device, true);
    let (hits, rows) = drawn(&app, 120, 40);
    let word = cell(&rows, "Loudness", "on");
    assert_eq!(hits.hit_at(word.0, word.1), Some(&Hit::LdOn));
    assert!(matches!(
        resolve(
            &hits,
            click(word, MouseEventKind::Down(MouseButton::Left)),
            &app
        ),
        Some(MouseAction::LdOn)
    ));
    let label = cell(&rows, "Loudness", "Loudness");
    assert_eq!(hits.hit_at(label.0, label.1), Some(&Hit::Ld(Ld::Strength)));
    // the bar: a press carries the column and the cells the bar was drawn across
    let bar = cell(&rows, "Loudness", "\u{2588}");
    assert_eq!(hits.hit_at(bar.0, bar.1), Some(&Hit::LdBar));
    match resolve(
        &hits,
        click(bar, MouseEventKind::Down(MouseButton::Left)),
        &app,
    ) {
        Some(MouseAction::BarPress { bar: b, at, area }) => {
            assert_eq!(b, crate::tui::app::glide::Bar::Ld);
            assert_eq!((at, area.width), (bar.0, 10));
            assert_eq!(area.x, bar.0);
        }
        other => panic!("{other:?}"),
    }
    // the wheel over the bar steps the strength
    match resolve(&hits, click(bar, MouseEventKind::ScrollUp), &app) {
        Some(MouseAction::Ld { row, step }) => assert_eq!((row, step), (Ld::Strength, 1)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_panels_arrows_and_bar_are_hits() {
    let app = panel(true);
    let (hits, rows) = drawn(&app, 120, 40);
    let on = cell(&rows, "Loudness   \u{2039}", "\u{203a}");
    assert_eq!(hits.hit_at(on.0, on.1), Some(&Hit::LdStep(Ld::On, 1)));
    let left = cell(&rows, "Reference", "\u{2039}");
    assert_eq!(
        hits.hit_at(left.0, left.1),
        Some(&Hit::LdStep(Ld::Reference, -1))
    );
    let right = cell(&rows, "Reference", "\u{203a}");
    assert!(matches!(
        resolve(
            &hits,
            click(right, MouseEventKind::Down(MouseButton::Left)),
            &app
        ),
        Some(MouseAction::Ld {
            row: Ld::Reference,
            step: 1
        })
    ));
    let bar = cell(&rows, "Strength", "\u{2588}");
    assert_eq!(hits.hit_at(bar.0, bar.1), Some(&Hit::LdBar));
    // a press on it carries the column and the cells the bar was drawn across
    match resolve(
        &hits,
        click(bar, MouseEventKind::Down(MouseButton::Left)),
        &app,
    ) {
        Some(MouseAction::BarPress { bar: b, at, area }) => {
            assert_eq!(b, crate::tui::app::glide::Bar::Ld);
            assert_eq!((at, area.width), (bar.0, 10));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_view_that_is_unavailable_says_why_and_draws_the_rows_as_live() {
    let mut app = app_on(Screen::Tone, true);
    let theme = Theme::new(false);
    let live = |app: &App| {
        let view = app.loudness_view().expect("loudness").clone();
        super::strength_row(app, &view, &theme, 80).line
    };
    let before = live(&app);
    let view = app.host.computer.eq.as_mut().expect("a view");
    view.loudness.as_mut().expect("loudness").unavailable = Some("no sink".to_string());
    assert_eq!(live(&app), before, "the rows are not dimmed");
    app.focus_ld(Ld::On);
    let rows = rows(&app, 80, 24);
    assert!(
        rows[at(&rows, "Loudness   \u{2039}") + 3].contains("unavailable: no sink"),
        "{rows:#?}"
    );
}

#[test]
fn a_click_on_the_panes_dim_lines_focuses_the_strength_row() {
    let mut pane = app_on(Screen::Device, true);
    crate::tui::test_support::hear_one(&mut pane, "12.50", 3);
    let (hits, rows) = drawn(&pane, 120, 40);
    for needle in ["bass +7.3", "the DAC's loudness is on too"] {
        let at = cell(&rows, needle, needle);
        assert_eq!(
            hits.hit_at(at.0, at.1),
            Some(&Hit::Ld(Ld::Strength)),
            "{needle}"
        );
    }
}

#[test]
fn the_panel_never_takes_a_row_from_the_graph() {
    // the graph keeps its rows whether the keys are in the panel or on the header
    let graph = |app: &App| {
        super::super::curve_of(ratatui::layout::Rect::new(0, 0, 80, 24), app)
            .expect("a curve")
            .height
    };
    let header = app_on(Screen::Tone, true);
    let mut open = app_on(Screen::Tone, true);
    open.focus_ld(Ld::Reference);
    assert_eq!(graph(&header), graph(&open));
    assert_eq!(graph(&open), 4, "four data rows at 80x24");
}
