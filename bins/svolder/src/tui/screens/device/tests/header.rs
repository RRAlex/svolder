// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen's header: the wordmark, the fault glyph, the link dot and the device name.

use super::*;

/// At 100 columns the header carries the small mark on row 1, the device line beside it on row 2
/// and the tab strip on row 3. The test reads the mark's rows from `wordmark::lines` instead of a
/// golden so that redrawing the art does not break it.
#[test]
fn a_hundred_column_screen_carries_the_small_wordmark_in_its_header() {
    let theme = Theme::new(false);
    let rows = text(&frame_with_theme(&mockup_app(), 100, 24, &theme));
    let mark = wordmark::lines(wordmark::Size::Small, &theme);
    let top: String = mark[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        rows[0].contains(top.trim_end()),
        "the mark's first row is not on row 1: |{}|",
        rows[0]
    );
    assert!(
        rows[1].contains("Gungnir 2 · fw 3") && rows[1].ends_with("mouse:on 14:02"),
        "the device name does not sit beside the mark on row 2: |{}|",
        rows[1]
    );
    assert!(
        !rows[1].contains("svolder"),
        "the mark says svolder's name, so the word itself does not: |{}|",
        rows[1]
    );
    assert_eq!(
        rows[2],
        format!(
            "─┤ Controls ├── EQ ── Telemetry ── Devices {} ? help ─ esc menu ─",
            "─".repeat(100 - 63)
        ),
        "the tab strip does not sit on row 3"
    );
}

#[test]
fn the_header_fault_glyph_carries_the_fault_role() {
    let mut app = faulty_mockup_app();
    hear(
        &mut app,
        vec![vo(
            "230.1",
            Some("fault_overcurrent"),
            json!(true),
            "active",
        )],
    );
    let buffer = screen(&app);
    let glyph = buffer
        .cell((50, 0))
        .expect("the fault glyph cell in the header");
    assert_eq!(glyph.symbol(), "⚡");
    assert_eq!(
        glyph.fg,
        Color::Red,
        "a fault is not drawn in the fault role"
    );
}

/// A link that went down hollows the header's dot, turns a spinner in front of the word —
/// the header's own wait, gone the moment the link is back — and dims every row to its
/// last-known value.
#[test]
fn a_link_that_went_down_hollows_the_header_dot_and_dims_the_rows() {
    let mut app = mockup_app();
    app.apply(Update::Connection { connected: false }, Instant::now());
    let buffer = screen(&app);
    assert_eq!(
        text(&buffer)[0],
        " svolder  Gungnir 2 · fw 3 · 0102…08  ⠋ ○ disconnected            mouse:on 14:02",
    );
    // The first letter of the label of the focused row, then of a row that is not.
    for (y, letter, row) in [(3, "I", "Input"), (4, "P", "Phase invert")] {
        let cell = buffer
            .cell((4, y))
            .unwrap_or_else(|| panic!("the {row} row's label"));
        assert_eq!(cell.symbol(), letter, "the {row} row moved");
        assert!(
            cell.modifier.contains(Modifier::DIM),
            "the {row} row shows a last-known value and is not dim: {:?}",
            cell.modifier
        );
    }

    app.apply(Update::Connection { connected: true }, Instant::now());
    let rows = text(&screen(&app));
    assert!(
        rows[0].contains("● connected") && !rows[0].contains('⠋'),
        "the spinner outlived the link it was turning for: |{}|",
        rows[0]
    );
}

/// A link that is down with nothing left trying to bring it back — the feed itself has
/// closed, as it does once a direct session spends its one connect deadline — hollows the
/// header's dot exactly as any other dropped link does, but turns no spinner in front of
/// it: there is no wait under way for one to mark.
#[test]
fn a_closed_feed_hollows_the_header_dot_with_no_spinner() {
    let mut app = mockup_app();
    app.apply(Update::Connection { connected: false }, Instant::now());
    app.session.feed_open = false;
    let rows = text(&screen(&app));
    assert!(
        rows[0].contains("○ disconnected") && !rows[0].contains('⠋'),
        "a closed feed still turned the spinner: |{}|",
        rows[0]
    );
}

/// A device name too long for the header's text row at 100 columns, where the small wordmark
/// takes 33 of them, is cut short with `…` so the connection words, the fault marks, the mouse
/// indicator and the clock stay whole to the row's right edge. A name that fits is drawn as it
/// is, at 80, 100 and 120 columns.
#[test]
fn a_long_device_name_is_elided_so_the_header_keeps_its_tail() {
    const LONG: &str = "Forkbeard Gungnir 2 Limited Anniversary Edition Prototype";
    for width in [80, 100, 120] {
        let mut app = faulty_mockup_app();
        app.clock = "14:02".into();
        app.session.product = LONG.into();
        // Over-current is active, so a fault mark follows the connection words.
        hear(
            &mut app,
            vec![vo(
                "230.1",
                Some("fault_overcurrent"),
                json!(true),
                "active",
            )],
        );
        let rows = text(&frame(&app, width, 30));
        let row = rows
            .iter()
            .find(|row| row.contains("mouse:"))
            .unwrap_or_else(|| panic!("the header row in {rows:#?}"));
        let row = row.trim_end();
        assert!(row.ends_with("mouse:on 14:02"), "{width}: |{row}|");
        assert!(row.contains("…"), "{width}: the name is elided: |{row}|");
        assert!(
            row.contains("connected") && row.contains("over-current mouse:on"),
            "{width}: |{row}|"
        );
    }
}

/// A device name of wide glyphs, which take two cells each, is measured in cells: at 100
/// columns it is elided and the connection words, `mouse:on` and the clock still read whole.
#[test]
fn a_wide_glyph_name_is_elided_by_cells() {
    let mut app = mockup_app();
    app.clock = "14:02".into();
    app.session.product = "フォークビアード・ガングニール・ツー・限定版".into();
    let rows = text(&frame(&app, 100, 30));
    let row = rows
        .iter()
        .find(|row| row.contains("mouse:"))
        .expect("the header row");
    let row = row.trim_end();
    assert!(row.ends_with("mouse:on 14:02"), "|{row}|");
    assert!(row.contains('…') && row.contains("connected"), "|{row}|");
}

/// The wide tier's header on every screen but the tone one names the computer EQ while it is on,
/// as `Host EQ NAME`; the 80×24 header does not, and neither does an off one.
#[test]
fn the_wide_device_header_names_the_computer_eq_while_it_is_on() {
    use svolder_host::eq::{HostEqState, Manager, Route};
    let state = |on| HostEqState {
        on,
        name: "Phones".into(),
        origin: None,
        target: "alsa_output.test".into(),
        route: Route::Default,
        manager: Manager::Process,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
    };
    let header = |app: &App, width, height| {
        text(&frame_with_theme(app, width, height, &Theme::new(false)))
            .into_iter()
            .find(|row| row.contains("connected"))
            .expect("the header row")
    };
    let mut app = mockup_app();
    app.presets.host_eq = Some(state(true));
    assert!(
        header(&app, 126, 30).contains("Host EQ Phones"),
        "{}",
        header(&app, 126, 30)
    );
    assert!(!header(&app, 80, 24).contains("Host EQ"));
    app.presets.host_eq = Some(state(false));
    assert!(!header(&app, 126, 30).contains("Host EQ"));
}

/// The header's own logo, where the exit lands: the small mark at 100 columns and wider, and
/// the bold word below.
#[test]
fn the_header_reports_its_logo_where_it_draws_it() {
    use crate::tui::render::Logo;
    let app = app_on(profile_named("gungnir2-v2"));
    for (w, h) in [(80, 24), (120, 40)] {
        let buf = crate::tui::test_support::frame(&app, w, h);
        let (_, [header, _]) = crate::tui::layout_tier::split(Rect::new(0, 0, w, h));
        match logo_of(header) {
            Some(Logo::Word(rect)) => {
                assert_eq!(w, 80);
                let word: String = (rect.left()..rect.right())
                    .map(|x| buf.cell((x, rect.y)).unwrap().symbol().to_string())
                    .collect();
                assert_eq!(word, "svolder");
            }
            Some(Logo::Mark(rect)) => {
                assert_eq!(w, 120);
                assert_eq!(rect, Rect::new(1, 0, wordmark::width(Size::Small), 2));
            }
            None => panic!("every screen's header carries a logo"),
        }
    }
}
