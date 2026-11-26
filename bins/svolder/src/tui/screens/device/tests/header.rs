// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen's header: the wordmark, the fault glyph, the link dot and the device name.

use super::*;
use crate::tui::test_support::hear_one;

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
        rows[1].contains("Gungnir 2 · fw 3") && rows[1].ends_with("mouse:on 14:02 "),
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
        .cell((49, 0))
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
        " svolder  Gungnir 2 · fw 3 · 0102…08  ⠋ ○ disconnected · stale   mouse:on 14:02 ",
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

/// The clock is followed by one blank cell at every width, so the corner never touches the
/// window's right edge, and the mouse indicator keeps one blank before the clock.
#[test]
fn the_clock_keeps_one_blank_cell_before_the_right_edge() {
    for width in [80, 100, 120, 200] {
        let mut app = mockup_app();
        app.clock = "14:02".into();
        let rows = text(&frame(&app, width, 30));
        let row = rows
            .iter()
            .find(|row| row.contains("mouse:"))
            .unwrap_or_else(|| panic!("the header row in {rows:#?}"));
        assert!(row.ends_with("mouse:on 14:02 "), "{width}: |{row}|");
        assert_eq!(row.chars().count(), usize::from(width), "{width}: |{row}|");
    }
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
    let state = |on| {
        let mut eq = crate::tui::test_support::computer_eq(on, &[("PK", 1000.0, 2.0, 1.0)]);
        eq.preset = Some("Phones".into());
        Some(eq)
    };
    let header = |app: &App, width, height| {
        text(&frame_with_theme(app, width, height, &Theme::new(false)))
            .into_iter()
            .find(|row| row.contains("connected"))
            .expect("the header row")
    };
    let mut app = mockup_app();
    app.host.computer.eq = state(true);
    assert!(
        header(&app, 126, 30).contains("Host EQ Phones"),
        "{}",
        header(&app, 126, 30)
    );
    assert!(!header(&app, 80, 24).contains("Host EQ"));
    app.host.computer.eq = state(false);
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

/// The row of `buffer` that carries the link words, the mouse mark and the clock.
fn device_row(buffer: &Buffer) -> String {
    text(buffer)
        .into_iter()
        .find(|row| row.contains("mouse:"))
        .expect("a header row")
}

/// While the link is down the header says the values on screen are from before it dropped, in
/// the longest words the row has room for, at once and in the warning role; the first status
/// after the link is back clears it, and the line keeps its width, so nothing around it moves.
#[test]
fn a_link_down_marks_the_readings_as_from_before_the_drop_until_a_fresh_one() {
    let theme = Theme::new(false);
    let mut app = mockup_app();
    let live = device_row(&frame_with_theme(&app, 140, 24, &theme));
    assert!(!live.contains("readings"), "{live}");
    app.apply(Update::Connection { connected: false }, Instant::now());
    let down = device_row(&frame_with_theme(&app, 140, 24, &theme));
    assert!(
        down.contains("○ disconnected · readings from before the link dropped"),
        "{down}"
    );
    assert!(down.ends_with("mouse:on 14:02 "), "{down}");
    assert_eq!(down.chars().count(), live.chars().count());
    // A row narrower than the long words has the shorter ones, and the narrowest the shortest.
    let narrow = device_row(&frame_with_theme(&app, 110, 24, &theme));
    assert!(
        narrow.contains("○ disconnected · readings stale"),
        "{narrow}"
    );
    let buffer = frame_with_theme(&app, 140, 24, &theme);
    let y = u16::try_from(
        text(&buffer)
            .iter()
            .position(|row| row.contains("mouse:"))
            .unwrap(),
    )
    .unwrap();
    let at = down.find("readings").expect("the words");
    let cell = buffer.cell((u16::try_from(down[..at].chars().count()).unwrap(), y));
    assert_eq!(
        cell.map(|cell| cell.fg),
        Some(theme.style(Role::Warn).fg.unwrap_or_default())
    );
    // The link back is not a fresh reading: the words stay until the device says something.
    app.apply(Update::Connection { connected: true }, Instant::now());
    let back = device_row(&frame_with_theme(&app, 140, 24, &theme));
    assert!(
        back.contains("● connected · readings from before"),
        "{back}"
    );
    hear_one(&mut app, "10", 100);
    let fresh = device_row(&frame_with_theme(&app, 140, 24, &theme));
    assert!(!fresh.contains("readings"), "{fresh}");
}

/// A device never heard has no readings to be old: the waiting panel's header says only that it
/// is disconnected.
#[test]
fn a_device_never_heard_has_no_old_readings() {
    let mut app = mockup_app();
    app.readings.values.clear();
    app.apply(Update::Connection { connected: false }, Instant::now());
    let top = device_row(&frame_with_theme(&app, 140, 24, &Theme::new(false)));
    assert!(top.contains("○ disconnected"), "{top}");
    assert!(!top.contains("readings") && !top.contains("stale"), "{top}");
}

/// The wall clock the signal cases hold, in unix milliseconds.
const SIGNAL_WALL: u64 = 1_800_000_000_000;

/// `mockup_app` with the facts of a device whose signal is `dbm` heard `secs` ago, the link
/// `connected` or not.
fn signal_app(dbm: Option<i16>, secs: u64, connected: bool) -> App {
    crate::tui::widgets::graph::hold_wall_ms(SIGNAL_WALL);
    let mut app = mockup_app();
    app.session.connected = connected;
    app.session.facts = Some(crate::backend::DeviceRow {
        name: "gungnir".into(),
        signal: dbm.map(|dbm| svolder_ipc::Signal {
            dbm,
            source: svolder_ipc::SignalSource::Advert,
            heard_at_ms: SIGNAL_WALL - secs * 1000,
        }),
        ..crate::backend::DeviceRow::default()
    });
    app
}

/// The header's text row of `app` at `width` columns.
fn header_row(app: &App, width: u16) -> String {
    let rows = text(&frame(app, width, 24));
    match width >= 100 {
        true => rows[1].clone(),
        false => rows[0].clone(),
    }
}

/// Four bars and the reading follow the connection word, by the rule the GNOME panel draws
/// (svolder-ipc `Signal::bars`): 4 bars from -55 dBm, 3 from -67, 2 from -80, 1 below, the unlit
/// ones dim.
#[test]
fn the_header_carries_four_bars_and_the_reading_after_the_connection_word() {
    for (dbm, lit) in [
        (-50, 4),
        (-55, 4),
        (-56, 3),
        (-67, 3),
        (-68, 2),
        (-80, 2),
        (-81, 1),
    ] {
        let at = signal_app(Some(dbm), 7, true);
        let buffer = frame(&at, 120, 24);
        let row = text(&buffer)[1].clone();
        let start = row.chars().position(|c| c == '▂').unwrap() as u16;
        let lit_now = (0..4u16)
            .filter(|n| {
                !buffer
                    .cell((start + n, 1))
                    .unwrap()
                    .modifier
                    .contains(Modifier::DIM)
            })
            .count();
        assert_eq!(lit_now, lit, "{dbm} dBm");
    }
    let app = signal_app(Some(-50), 7, true);
    assert_eq!(
        header_row(&app, 80),
        " svolder  Gungnir 2 · fw 3 · 0102…08  ● connected  ▂▄▆█ −50 dBm  mouse:on 14:02 "
    );
    let row = header_row(&signal_app(Some(-74), 7, true), 120);
    assert!(row.contains("● connected  ▂▄▆█ −74 dBm"), "|{row}|");
    let buffer = frame(&signal_app(Some(-74), 7, true), 80, 24);
    let start = text(&buffer)[0].chars().position(|c| c == '▂').unwrap() as u16;
    let styles: Vec<Modifier> = (0..4)
        .map(|n| buffer.cell((start + n, 0)).unwrap().modifier)
        .collect();
    assert!(
        !styles[0].contains(Modifier::DIM) && !styles[1].contains(Modifier::DIM),
        "{styles:?}"
    );
    assert!(
        styles[2].contains(Modifier::DIM) && styles[3].contains(Modifier::DIM),
        "{styles:?}"
    );
}

/// A reading older than 120 seconds (the panel's rule) is drawn dim, bars and number; a fresh one is not.
#[test]
fn an_old_reading_is_drawn_dim_and_a_fresh_one_is_not() {
    let dim_of = |secs: u64| {
        let buffer = frame(&signal_app(Some(-50), secs, true), 80, 24);
        let row = text(&buffer)[0].clone();
        let at = row.chars().position(|c| c == '▂').unwrap() as u16;
        (0..12)
            .map(|n| {
                buffer
                    .cell((at + n, 0))
                    .unwrap()
                    .modifier
                    .contains(Modifier::DIM)
            })
            .collect::<Vec<_>>()
    };
    assert!(dim_of(119).iter().all(|d| !d), "a fresh reading is not dim");
    assert!(
        dim_of(121).iter().all(|d| *d),
        "an old one is dim, the bars and the number"
    );
}

/// No signal heard draws nothing, and a narrow header drops the reading first, then the bars,
/// before it cuts the device name.
#[test]
fn a_narrow_header_drops_the_reading_then_the_bars_before_the_name() {
    assert_eq!(
        header_row(&signal_app(None, 0, true), 80),
        " svolder  Gungnir 2 · fw 3 · 0102…08  ● connected                mouse:on 14:02 "
    );
    // As the name grows: the whole line, then no reading, then no bars, and the name is cut
    // only once nothing of the signal is left.
    let mut app = signal_app(Some(-80), 7, true);
    let mut seen = Vec::new();
    for len in 1..40 {
        app.session.product = "N".repeat(len);
        let row = header_row(&app, 80);
        let state = (
            row.contains("dBm"),
            row.contains('▂'),
            row.contains(&app.session.product),
        );
        if seen.last() != Some(&state) {
            seen.push(state);
        }
    }
    assert_eq!(
        seen,
        vec![
            (true, true, true),
            (false, true, true),
            (false, false, true),
            (false, false, false)
        ],
        "reading, then bars, then the name"
    );
    // With the link down and the readings old the signal is sized first: at 120 columns the bars
    // and the reading both stay beside the warning; at 80 the bars stay and the warning takes the
    // longest form that fits after them.
    let down = signal_app(Some(-80), 7, false);
    let row = header_row(&down, 120);
    assert!(row.contains("○ disconnected  ▂▄▆█ −80 dBm"), "|{row}|");
    assert!(row.contains("stale"), "the warning is still there: |{row}|");
    let row = header_row(&down, 80);
    assert!(row.contains("○ disconnected  ▂▄▆█"), "|{row}|");
    assert!(!row.contains("dBm"), "|{row}|");
    assert!(row.contains("Gungnir 2"), "the name stays: |{row}|");
}
