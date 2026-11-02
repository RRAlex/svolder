// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tab strip that the rule row draws, its parts and its hits.

use super::*;

/// The tab strip a screen of the device draws across a terminal `width` columns wide, with the
/// current tab marked and `end` after the help mark.
fn strip(current: &str, width: usize, end: &str) -> String {
    let names = ["Controls", "EQ", "Telemetry", "Devices"];
    let mut tabs = String::from("─");
    for (n, name) in names.iter().enumerate() {
        if n > 0 {
            tabs.push_str("──");
        }
        match *name == current {
            true => tabs.push_str(&format!("┤ {name} ├")),
            false => tabs.push_str(&format!(" {name} ")),
        }
    }
    let tail = format!(" ? help ─ esc {end} ─");
    let fill = width - tabs.chars().count() - tail.chars().count();
    format!("{tabs}{}{tail}", "─".repeat(fill))
}

/// Every screen of the device draws its tab strip as the header's last row at 80, 100, 120 and
/// 200 columns, wide enough to fill the frame, with the current tab between `┤ ├`.
#[test]
fn the_rule_is_the_tab_strip_at_every_width() {
    for (screen, current, end) in [
        (Screen::Device, "Controls", "menu"),
        (Screen::Tone, "EQ", "back"),
        (Screen::Telemetry, "Telemetry", "back"),
        (Screen::Devices, "Devices", "back"),
    ] {
        let mut app = mockup_app();
        app.show(screen);
        for width in [80u16, 100, 120, 200] {
            let rows = text(&frame(&app, width, 30));
            let row = usize::from(header_height(width)) - 1;
            assert_eq!(
                rows[row],
                strip(current, usize::from(width), end),
                "{screen:?} at {width}"
            );
            assert_eq!(rows[row].chars().count(), usize::from(width));
        }
    }
    let mut app = mockup_app();
    assert_eq!(
        text(&frame(&app, 80, 24))[1],
        "─┤ Controls ├── EQ ── Telemetry ── Devices ───────────────── ? help ─ esc menu ─"
    );
    app.show(Screen::Devices);
    assert_eq!(
        text(&frame(&app, 80, 24))[1],
        "─ Controls ── EQ ── Telemetry ──┤ Devices ├───────────────── ? help ─ esc back ─"
    );
}

/// At 100 columns and wider the strip is the third row of the header, under the wordmark and
/// the text row.
#[test]
fn at_a_hundred_columns_the_strip_is_the_third_row() {
    let rows = text(&frame(&mockup_app(), 100, 30));
    assert!(rows[2].starts_with("─┤ Controls ├"), "|{}|", rows[2]);
    assert!(!rows[1].contains("Controls"), "|{}|", rows[1]);
}

/// The strip's dashes and `┤ ├` are the rule, the current tab is bold in the accent, the others
/// are text with their key letter bold in the hint role, and `?` and `esc` are bold hints.
#[test]
fn the_strip_styles_its_parts() {
    let theme = Theme::new(false);
    let buffer = frame(&mockup_app(), 80, 24);
    let at = |x: u16| buffer.cell((x, 1)).expect("a cell on the strip").clone();
    let find = |word: &str| {
        let row = text(&buffer)[1].clone();
        let byte = row.find(word).expect("the word is on the strip");
        u16::try_from(row[..byte].chars().count()).expect("a column")
    };
    let rule = theme.style(Role::Rule);
    assert_eq!(
        (at(0).symbol(), at(0).fg),
        ("─", rule.fg.expect("a colour"))
    );
    assert_eq!(at(1).symbol(), "┤");
    assert_eq!(at(1).fg, rule.fg.expect("a colour"));
    let controls = at(find("Controls"));
    assert_eq!(controls.fg, theme.style(Role::Cursor).fg.expect("a colour"));
    assert!(controls.modifier.contains(Modifier::BOLD));
    for (word, rest) in [
        ("EQ", "Q"),
        ("Telemetry", "elemetry"),
        ("Devices", "evices"),
    ] {
        let first = at(find(word));
        assert_eq!(
            first.fg,
            theme.style(Role::KeyHint).fg.expect("a colour"),
            "{word}"
        );
        assert!(first.modifier.contains(Modifier::BOLD), "{word}");
        let tail = at(find(word) + 1);
        assert_eq!(
            tail.fg,
            theme.style(Role::Text).fg.expect("a colour"),
            "{rest}"
        );
        assert!(!tail.modifier.contains(Modifier::BOLD), "{rest}");
    }
    for key in ["?", "esc"] {
        let cell = at(find(&format!(" {key}")) + 1);
        assert_eq!(
            cell.fg,
            theme.style(Role::KeyHint).fg.expect("a colour"),
            "{key}"
        );
        assert!(cell.modifier.contains(Modifier::BOLD), "{key}");
    }
    for word in ["help", "menu"] {
        let cell = at(find(word));
        assert_eq!(
            cell.fg,
            theme.style(Role::Text).fg.expect("a colour"),
            "{word}"
        );
    }
}

/// A device with no tone control has its `EQ` tab faint, whole, and no hit on it.
#[test]
fn the_eq_tab_is_faint_on_a_device_with_no_tone_control() {
    let theme = Theme::new(false);
    let app = app_on(profile_named("lyr5"));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();
    let faint = theme.style(Role::Faint).fg.expect("a colour");
    // `─┤ Controls ├── EQ ──`: the name sits on columns 16 and 17.
    for x in [16u16, 17] {
        assert_eq!(buffer.cell((x, 1)).expect("a cell").fg, faint, "column {x}");
    }
    assert_eq!(hits.hit_at(16, 1), None);
}

/// The strip records a hit on each other tab's name, and the two marks; the current tab has none.
#[test]
fn the_strip_records_its_tabs_and_marks() {
    let theme = Theme::new(false);
    let app = mockup_app();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let row = text(terminal.backend().buffer())[1].clone();
    let col = |word: &str| {
        u16::try_from(row[..row.find(word).expect("on the strip")].chars().count())
            .expect("a column")
    };
    assert_eq!(hits.hit_at(col("EQ"), 1), Some(&Hit::Tab(Screen::Tone)));
    assert_eq!(
        hits.hit_at(col("Telemetry"), 1),
        Some(&Hit::Tab(Screen::Telemetry))
    );
    assert_eq!(
        hits.hit_at(col("Devices"), 1),
        Some(&Hit::Tab(Screen::Devices))
    );
    assert_eq!(hits.hit_at(col("? help"), 1), Some(&Hit::HelpMark));
    assert_eq!(hits.hit_at(col("esc menu") + 7, 1), Some(&Hit::EscMark));
    assert_eq!(hits.hit_at(col("Controls"), 1), None);
}
