// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Tone pane on the device screen: its bands, presets and rounding.

use super::*;

/// In Flat and Loudness modes the Tone pane's band rows are dimmed and it says right under them
/// that the bands apply in EQ mode; in EQ mode it does neither. Either way a blank line sets the
/// band table off from the rows above it.
#[test]
fn the_tone_pane_dims_its_bands_outside_eq_mode_and_says_why() {
    let theme = Theme::new(false);
    let tone = |row: &String| {
        row.chars()
            .skip(usize::from(CONTROLS_WIDTH))
            .collect::<String>()
    };
    for (mode, applies) in [(1, false), (2, true), (3, false)] {
        let app = boosting(109, mode);
        let rows = text(&frame_with_theme(&app, 80, 24, &theme));
        let buffer = frame_with_theme(&app, 80, 24, &theme);
        assert!(tone(&rows[5]).contains("Preset"), "mode {mode}: {rows:#?}");
        assert_eq!(tone(&rows[6]).trim_matches(['│', ' ']), "", "mode {mode}");
        assert!(tone(&rows[7]).contains("Band   Freq"), "mode {mode}");
        let hint = rows
            .iter()
            .position(|row| row.contains("bands apply in EQ mode"));
        match applies {
            true => assert_eq!(hint, None, "mode {mode}"),
            false => {
                assert_eq!(hint, Some(13), "mode {mode}: {rows:#?}");
                assert!(
                    tone(&rows[12]).contains("5  "),
                    "mode {mode}: the note is under band 5"
                );
            }
        }
        // Band 3's row, which the cursor is not on: its frequency's first digit.
        let cell = buffer.cell((49, 10)).expect("a band cell");
        assert_eq!(cell.symbol(), "6", "mode {mode}");
        assert_eq!(
            cell.modifier.contains(Modifier::DIM),
            !applies,
            "mode {mode}: {cell:?}"
        );
    }
}

/// The band the cursor is on keeps its selected style — bold, inverse and the accent
/// colour — outside EQ mode too, rather than the dim role's grey, which its unfocused
/// neighbours still take: only an unfocused band row dims where the mode does not apply the
/// bands.
#[test]
fn the_focused_band_keeps_its_selected_style_outside_eq_mode() {
    for depth in [
        ColorDepth::Ansi16,
        ColorDepth::Ansi256,
        ColorDepth::TrueColor,
    ] {
        let theme = Theme::from_config(&TuiConfig::default(), false, depth);
        let selected = theme.style(Role::Selected);
        for mode in [1, 2, 3] {
            let app = boosting(109, mode);
            assert_eq!(
                app.tone.band, 0,
                "band 1 is not the row the cursor starts on"
            );
            let buffer = frame_with_theme(&app, 80, 24, &theme);
            // Band 1's row: its frequency's first digit.
            let cell = buffer.cell((47, 8)).expect("a band cell");
            assert!(
                cell.modifier.contains(Modifier::BOLD | Modifier::REVERSED),
                "{depth:?} mode {mode}: the focused band lost its selected style: {cell:?}"
            );
            assert!(
                !cell.modifier.contains(Modifier::DIM),
                "{depth:?} mode {mode}: the focused band was dimmed: {cell:?}"
            );
            assert_eq!(
                cell.fg,
                selected.fg.unwrap_or(Color::Reset),
                "{depth:?} mode {mode}: the focused band's colour is not the selected one"
            );
        }
    }
}

/// The Tone pane reads a band the way the tone screen's table does: the gain to the hundredth a
/// device step lands on (`-2.45`, not `-2.4`) and a frequency under 100 Hz to a tenth (`40.0`).
#[test]
fn the_tone_pane_rounds_as_the_tone_screen_does() {
    let rows = text(&screen(&mockup_app()));
    let pane = |band: &str| -> String {
        rows.iter()
            .find(|row| {
                row.contains(&format!("││ ▶ {band}")) || row.contains(&format!("││   {band}"))
            })
            .cloned()
            .unwrap_or_default()
    };
    assert!(pane("1").contains("40.0 Hz"), "{}", pane("1"));
    assert!(pane("2").contains("160 Hz   -2.45 dB"), "{}", pane("2"));
    assert!(pane("4").contains("2515 Hz   +1.45 dB"), "{}", pane("4"));
}

/// The Tone pane names the preset the tone control is on, as the tone-control screen's header
/// does: `custom` for a curve no preset matches, the preset's name, `NAME*` once the curve moved
/// off the one loaded last, and `loading NAME` while a recall is still being confirmed.
#[test]
fn the_tone_pane_names_the_preset_that_is_on() {
    let preset_row = |app: &App| {
        text(&screen(app))
            .into_iter()
            .find(|row| row.contains("Preset"))
            .unwrap_or_default()
    };
    let mut app = mockup_app();
    assert!(
        preset_row(&app).contains("Preset    custom"),
        "|{}|",
        preset_row(&app)
    );

    // A curve that matches nothing after Warm was loaded reads as Warm changed.
    app.presets.loaded_preset = Some("Warm".into());
    assert!(
        preset_row(&app).contains("Preset    Warm*"),
        "|{}|",
        preset_row(&app)
    );

    // Every band at 0 dB in EQ mode is Flat's curve, and the preset that matches is the one
    // named, whatever was loaded.
    let flat: Vec<ValueObject> = (1..=5)
        .map(|band| vo(&format!("12.{band}.3"), None, json!(98), "98"))
        .collect();
    hear(&mut app, flat);
    assert!(
        preset_row(&app).contains("Preset    Flat"),
        "|{}|",
        preset_row(&app)
    );

    app.readings.loading = Some(crate::tui::boxes::presets::Loading {
        name: "Bass".into(),
        last: "12.50".parse().expect("the mode's function"),
        since: Instant::now(),
    });
    assert!(
        preset_row(&app).contains("Preset    loading Bass"),
        "|{}|",
        preset_row(&app)
    );
}

/// The Preset line as the pane draws it at `width`, after its label.
fn preset_after_label(app: &App, width: usize) -> String {
    let eq = app
        .profile
        .eq
        .clone()
        .expect("the profile has a tone control");
    let theme = truecolour_theme();
    let line = tone_lines(app, &eq, &theme, width).lines[2].clone();
    let row: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    row.trim_start()
        .strip_prefix("Preset")
        .unwrap_or(&row)
        .trim()
        .to_string()
}

fn truecolour_theme() -> Theme {
    theme_for("amber", ColorDepth::TrueColor)
}

/// A name too long for the pane is cut short with `…`, its mark kept; with room for less than
/// two cells of name the lead and the mark go too, so no bare `*` or `loading ` is left.
#[test]
fn the_preset_line_cuts_the_name_and_drops_the_lead_and_mark_when_none_fits() {
    let mut app = mockup_app();
    app.presets.loaded_preset = Some("Presence".into());
    // 13 cells of cursor and label column, then the room for the name and its mark.
    assert_eq!(preset_after_label(&app, 13 + 40), "Presence*");
    assert_eq!(preset_after_label(&app, 13 + 6), "Pres…*");
    assert_eq!(preset_after_label(&app, 13 + 1), "");

    app.readings.loading = Some(crate::tui::boxes::presets::Loading {
        name: "Bass lift".into(),
        last: "12.50".parse().expect("the mode's function"),
        since: Instant::now(),
    });
    assert_eq!(preset_after_label(&app, 13 + 13), "loading Bass…");
    assert_eq!(preset_after_label(&app, 13 + 9), "");
}

/// A device with no tone control has a blank line where the Preset line would be.
#[test]
fn a_device_with_no_tone_control_has_a_blank_preset_line() {
    let mut app = mockup_app();
    let eq = app
        .profile
        .eq
        .take()
        .expect("the profile has a tone control");
    let theme = truecolour_theme();
    let line = tone_lines(&app, &eq, &theme, 60).lines[2].clone();
    assert_eq!(line.spans.iter().map(Span::width).sum::<usize>(), 0);
}

/// The computer side's Crossfeed row sits at the bottom of the pane whatever its height: the
/// summary under it goes first when the pane is short, then the table's rows, scrolled to keep
/// the focused point in view; the row itself always stays.
#[test]
fn a_short_tone_pane_gives_up_the_crossfeed_summary_first_and_the_row_stays() {
    use crate::tui::app::Screen;
    let theme = Theme::new(false);
    let mut app = crate::tui::test_support::app_showing(Screen::Device);
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_crossfeed(
        true, "bauer", None,
    ));
    app.flip_side();
    let eq = app.profile.eq.clone().expect("a tone control");
    let kinds = |height: usize| {
        let pane = tone_lines_in(&app, &eq, &theme, (38, height));
        let plain: Vec<String> = pane.lines.iter().map(|l| l.to_string()).collect();
        (pane.lines.len(), plain, pane.crossfeed.map(|row| row.line))
    };
    // tall: blank, row, summary at the very bottom
    let (len, plain, row) = kinds(14);
    assert_eq!(len, 14, "{plain:#?}");
    assert_eq!(row, Some(12));
    assert!(plain[13].contains("37 %"), "{plain:#?}");
    // 8 rows is the least that keeps the summary: 4 of head, a point, a blank, row, summary
    let (len, plain, _) = kinds(8);
    assert_eq!((len, plain[7].contains("37 %")), (8, true), "{plain:#?}");
    // one row fewer: the summary goes, the row stays at the bottom, the point stays
    let (len, plain, row) = kinds(7);
    assert_eq!((len, row), (7, Some(6)), "{plain:#?}");
    assert!(plain[6].contains("Crossfeed"), "{plain:#?}");
    assert!(!plain.iter().any(|l| l.contains("37 %")));
    assert!(
        plain[4].contains("1000"),
        "the table keeps a row: {plain:#?}"
    );
}

/// The computer's `Mode ‹ Off | EQ ›` in the Tone pane is drawn as the Controls pane's
/// `Target ‹ NAME | Computer ›` is: the side in force, the other side, the label and the arrows
/// take the same styles, with the keys on the row and off it, whichever side is chosen, in 16
/// colours and in monochrome.
#[test]
fn the_mode_choice_is_styled_as_the_target_choice_in_the_tone_pane() {
    use crate::tui::test_support::{style_at, style_of};
    for mono in [false, true] {
        let theme = Theme::new(mono);
        for eq_on in [false, true] {
            let mut app = mockup_app();
            app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
                eq_on,
                &[("PK", 1000.0, 0.0, 1.0)],
            ));
            app.flip_side();
            assert!(app.on_computer());
            let volume = app.volume().expect("a volume").id;
            app.set_selected(
                app.rows()
                    .iter()
                    .position(|row| row.control.id == volume)
                    .expect("the volume row"),
            );
            let name = app.session.product.clone();
            let both = |app: &App| {
                let buffer = frame_with_theme(app, 140, 40, &theme);
                let (on, off) = match eq_on {
                    true => ("EQ", "Off"),
                    false => ("Off", "EQ"),
                };
                let target = [
                    style_of(&buffer, "Target ‹ ", "Computer"),
                    style_of(&buffer, "Target ‹ ", &name),
                    style_of(&buffer, "Target ‹ ", "Target"),
                    style_of(&buffer, "Target ‹ ", "‹"),
                    style_of(&buffer, "Target ‹ ", "›"),
                    style_at(&buffer, "Target ‹ ", "Target", 6),
                ];
                let mode = [
                    style_of(&buffer, "Mode", on),
                    style_of(&buffer, "Mode", off),
                    style_of(&buffer, "Mode", "Mode"),
                    style_of(&buffer, "Mode", "‹"),
                    style_of(&buffer, "Mode", "›"),
                    style_at(&buffer, "Mode", "Mode", 4),
                ];
                (target, mode)
            };
            app.tone.row = ToneRow::Point(0);
            let (target_rest, mode_rest) = both(&app);
            assert_eq!(mode_rest, target_rest, "mono={mono} on={eq_on} at rest");
            app.key(crossterm::event::KeyEvent::from(KeyCode::Up));
            assert!(app.switch_focused());
            let (target_keys, _) = both(&app);
            app.set_selected(app.selected());
            app.pane = Pane::Tone;
            app.tone.row = ToneRow::Mode;
            assert!(!app.switch_focused());
            let (_, mode_keys) = both(&app);
            assert_eq!(mode_keys, target_keys, "mono={mono} on={eq_on} keys on");
            assert_ne!(mode_keys[0], mode_keys[1], "the two sides differ");
            assert_ne!(mode_keys, mode_rest, "the keys show");
        }
    }
}

/// The Tone pane's Space row and Crossfeed row are drawn as the `Target` choice is: the word in
/// force in its chosen style, and with the keys on the row the box on the label and that word
/// only, the rest of the row plain, in 16 colours and in monochrome.
#[test]
fn the_pane_s_space_and_crossfeed_rows_are_styled_as_the_target_choice() {
    use crate::tui::app::Xf;
    use crate::tui::test_support::{computer_eq_with_speakers, style_at, style_of};
    for mono in [false, true] {
        let theme = Theme::new(mono);
        let mut app = mockup_app();
        app.host.computer.eq = Some(computer_eq_with_speakers(false, None, &[]));
        app.flip_side();
        let volume = app.volume().expect("a volume").id;
        app.set_selected(
            app.rows()
                .iter()
                .position(|row| row.control.id == volume)
                .expect("the volume row"),
        );
        let name = app.session.product.clone();
        let shot = |app: &App| frame_with_theme(app, 140, 40, &theme);
        let target = |app: &App| {
            let buffer = shot(app);
            [
                style_of(&buffer, "Target ‹ ", "Computer"),
                style_of(&buffer, "Target ‹ ", &name),
                style_of(&buffer, "Target ‹ ", "Target"),
                style_at(&buffer, "Target ‹ ", "Target", 6),
            ]
        };
        app.tone.row = ToneRow::Point(0);
        let rest = target(&app);
        let space = |buffer: &Buffer| {
            [
                style_of(buffer, "Space", "Off"),
                style_of(buffer, "Space", "‹"),
                style_of(buffer, "Space", "Space"),
                style_at(buffer, "Space", "Space", 5),
            ]
        };
        let buffer = shot(&app);
        assert_eq!(space(&buffer)[0], rest[0], "mono={mono} Space at rest");
        app.key(crossterm::event::KeyEvent::from(KeyCode::Up));
        let keys = target(&app);
        app.set_selected(app.selected());
        app.focus_crossfeed(Xf::Space);
        assert!(!app.switch_focused());
        let buffer = shot(&app);
        let drawn = space(&buffer);
        assert_eq!(drawn[0], keys[0], "mono={mono} Space keys on");
        assert_eq!(drawn[2], keys[2], "mono={mono} Space label");
        assert_eq!(drawn[3], keys[3], "mono={mono} Space pad");

        // the Crossfeed row, on
        app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_crossfeed(
            true, "bauer", None,
        ));
        app.tone.row = ToneRow::Point(0);
        let rest = shot(&app);
        assert_eq!(
            style_of(&rest, "Crossfeed ‹", "on"),
            target(&app)[0],
            "mono={mono} on at rest"
        );
        app.focus_crossfeed(Xf::Preset);
        let buffer = shot(&app);
        let row = "Crossfeed ‹";
        assert_eq!(style_of(&buffer, row, "on"), keys[0], "mono={mono} on keys");
        assert_eq!(
            style_of(&buffer, row, "Crossfeed"),
            keys[2],
            "mono={mono} label"
        );
        assert_eq!(
            style_at(&buffer, row, "Crossfeed", 9),
            keys[3],
            "mono={mono} pad"
        );
        assert!(
            !style_of(&buffer, row, "Bauer")
                .2
                .contains(Modifier::REVERSED),
            "the model words are not boxed"
        );
    }
}

/// With the chain's running not verified, the Mode row's `›` is still the step-on hit.
#[test]
fn the_pane_s_mode_arrows_keep_their_hits_when_the_chain_is_not_verified() {
    let mut app = mockup_app();
    let mut eq = crate::tui::test_support::computer_eq(false, &[("PK", 1000.0, 0.0, 1.0)]);
    eq.running_unknown = true;
    app.host.computer.eq = Some(eq);
    app.flip_side();
    let (buffer, hits) = crate::tui::test_support::drawn(&app, 140, 40);
    let row = text(&buffer)
        .iter()
        .position(|row| row.contains("Mode"))
        .and_then(|y| u16::try_from(y).ok())
        .expect("the Mode row");
    let at = |symbol: &str| {
        (CONTROLS_WIDTH..buffer.area.width)
            .find(|&x| buffer.cell((x, row)).is_some_and(|c| c.symbol() == symbol))
            .expect("the arrow")
    };
    assert_eq!(hits.hit_at(at("‹"), row), Some(&Hit::ModePrev));
    assert_eq!(hits.hit_at(at("›"), row), Some(&Hit::ModeNext));
}
