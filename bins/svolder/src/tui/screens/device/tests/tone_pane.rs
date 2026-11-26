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

/// The computer side's Crossfeed row follows the table, after a blank row where the pane has one
/// to spare: the Model and Preset rows under it go first when the pane is short, then the table's
/// rows, scrolled to keep the focused point in view; the row itself always stays.
#[test]
fn a_short_tone_pane_gives_up_the_model_and_preset_rows_first_and_the_row_stays() {
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
    // tall: blank, row, Model, Preset follow the two points; the spare rows fall below
    let (len, plain, row) = kinds(14);
    assert_eq!(len, 10, "{plain:#?}");
    assert_eq!(row, Some(7));
    assert!(
        plain[8].contains("Model") && plain[8].contains("Bauer"),
        "{plain:#?}"
    );
    assert!(plain[9].contains("Preset"), "{plain:#?}");
    // 9 rows is the least that keeps them: 4 of head, a point, a blank, row, Model, Preset
    let (len, plain, _) = kinds(9);
    assert_eq!((len, plain[7].contains("Model")), (9, true), "{plain:#?}");
    // one row fewer: they go, the row stays at the bottom, the point stays
    let (len, plain, row) = kinds(8);
    assert_eq!((len, row), (8, Some(7)), "{plain:#?}");
    assert!(plain[7].contains("Crossfeed"), "{plain:#?}");
    assert!(!plain.iter().any(|l| l.contains("Model")));
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
        app.focus_crossfeed(Xf::Space);
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
            !style_of(&buffer, "Model ", "Bauer")
                .2
                .contains(Modifier::REVERSED),
            "the model words are not boxed while the Space row has the keys"
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

/// The computer side's pane on the stages fixture (loudness, crossfeed playing) with `points`
/// points, the EQ `on` or off, drawn once at `(width, height)` so the app knows the pane's size.
fn fold_app(on: bool, points: usize, size: (u16, u16)) -> App {
    let mut app = mockup_app();
    let mut eq = crate::tui::test_support::computer_eq_stages(crate::tui::app::Space::Crossfeed);
    eq.on = on;
    eq.points.truncate(points);
    app.host.computer.eq = Some(eq);
    app.flip_side();
    assert!(app.on_computer());
    drop(frame(&app, size.0, size.1));
    app
}

/// The pane's rows at `(width, height)`, the pane's own cells only, trimmed.
fn pane_rows(app: &App, size: (u16, u16)) -> Vec<String> {
    text(&frame(app, size.0, size.1))
        .iter()
        .map(|row| {
            row.chars()
                .skip(usize::from(CONTROLS_WIDTH) + 1)
                .collect::<String>()
                .trim_end_matches(['│', ' '])
                .to_string()
        })
        .collect()
}

/// The sub-rows of the computer's pane are indented two cells inside the label column: the
/// Preset under Mode, and the Model and Preset rows under the crossfeed row. Their values stay
/// in the column the rows above them use, and the DAC side's own Preset line does not move.
#[test]
fn the_computer_panes_sub_rows_are_indented_under_their_rows() {
    for size in [(80, 24), (80, 40)] {
        let app = fold_app(true, 8, size);
        let rows = pane_rows(&app, size);
        let find = |needle: &str| {
            rows.iter()
                .find(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle}: {rows:#?}"))
                .clone()
        };
        let col = |row: &str, word: &str| {
            row.chars()
                .collect::<Vec<_>>()
                .windows(word.chars().count())
                .position(|w| w.iter().collect::<String>() == word)
        };
        let mode = find("Mode");
        let space = find("Space     ‹");
        let model = find("Model");
        let preset_xf = find("Default");
        let preset_eq = rows
            .iter()
            .find(|row| row.contains("Preset") && !row.contains('‹'))
            .cloned()
            .expect("the EQ's Preset row");
        let label_at = col(&mode, "Mode").expect("Mode");
        assert_eq!(col(&space, "Space"), Some(label_at), "{size:?}");
        for sub in [&preset_eq, &model, &preset_xf] {
            let word = if sub.contains("Model") {
                "Model"
            } else {
                "Preset"
            };
            assert_eq!(col(sub, word), Some(label_at + 2), "{size:?}: {sub}");
        }
        // values stay where they were: after the 10-cell label column
        assert_eq!(col(&preset_eq, "Bass"), col(&mode, "‹"), "{size:?}");
        assert_eq!(col(&model, "‹"), col(&mode, "‹"), "{size:?}");
    }
    // the DAC side's pane is as it was
    let rows = pane_rows(&mockup_app(), (80, 24));
    assert!(
        rows.iter().any(|row| row.contains("Preset    custom")),
        "{rows:#?}"
    );
}

/// With the EQ off and a pane too short for every point and everything under the table, the
/// table folds into the Preset line and the readout stays; tall, it stays, dimmed; with the EQ
/// on it never folds.
#[test]
fn the_computer_points_table_folds_with_the_eq_off_in_a_short_pane() {
    let size = (80, 24);
    let app = fold_app(false, 8, size);
    let rows = pane_rows(&app, size);
    assert!(
        rows.iter().any(|row| row.ends_with("Bass · 8 points")),
        "{rows:#?}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("Point") || row.contains("Freq"))
    );
    assert!(!rows.iter().any(|row| row.contains("60.0 Hz")));
    assert!(
        rows.iter()
            .any(|row| row.contains("at ") && row.contains("bass")),
        "{rows:#?}"
    );
    assert!(rows.iter().any(|row| row.contains("Model")), "{rows:#?}");
    // the blank row after Preset stays
    let at = rows
        .iter()
        .position(|row| row.contains("Bass · 8"))
        .expect("the line");
    assert_eq!(rows[at + 1], "", "{rows:#?}");
    assert!(rows[at + 2].contains("Loudness"), "{rows:#?}");

    // tall: nothing folds and the points are dimmed
    let tall = (80, 50);
    let app = fold_app(false, 8, tall);
    let rows = pane_rows(&app, tall);
    assert!(!rows.iter().any(|row| row.contains("points")), "{rows:#?}");
    assert!(rows.iter().any(|row| row.contains("Freq")), "{rows:#?}");
    let buffer = frame(&app, tall.0, tall.1);
    let y = text(&buffer)
        .iter()
        .position(|row| row.contains("105 Hz"))
        .expect("a point");
    let dim = (CONTROLS_WIDTH..buffer.area.width)
        .filter_map(|x| buffer.cell((x, u16::try_from(y).unwrap())))
        .any(|cell| cell.modifier.contains(Modifier::DIM));
    assert!(dim, "the table is dimmed");

    // EQ on at 80x24: scrolled, not folded
    let app = fold_app(true, 8, size);
    let rows = pane_rows(&app, size);
    assert!(rows.iter().any(|row| row.contains("Freq")), "{rows:#?}");
    assert!(!rows.iter().any(|row| row.contains("points")), "{rows:#?}");
}

/// A folded table's points are out of the walk: the arrows go from Mode to Loudness; a point
/// that had the cursor when the table folds gives it to Mode; turning the EQ on unfolds it; and
/// nothing is clickable where a folded row was.
#[test]
fn a_folded_table_is_skipped_by_the_arrows_and_takes_no_click() {
    let size = (80, 24);
    let mut app = fold_app(false, 8, size);
    assert!(app.tone_folded());
    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Mode;
    app.key(crossterm::event::KeyEvent::from(KeyCode::Down));
    assert_eq!(
        app.tone.row,
        ToneRow::Ld(crate::tui::app::Ld::Strength),
        "Mode, then Loudness"
    );
    app.key(crossterm::event::KeyEvent::from(KeyCode::Up));
    assert_eq!(app.tone.row, ToneRow::Mode);

    // a point focused while the table was shown moves to Mode when it folds
    let mut app = fold_app(true, 8, size);
    app.pane = Pane::Tone;
    app.focus_point(3);
    let mut eq = app.host.computer.eq.as_deref().cloned().expect("eq");
    eq.on = false;
    app.host.computer.eq = Some(Box::new(eq));
    drop(frame(&app, size.0, size.1));
    app.note_computer_view();
    assert!(app.tone_folded());
    assert_eq!(app.tone.row, ToneRow::Mode);

    // no point under the pointer where the first point's row was
    let (buffer, hits) = crate::tui::test_support::drawn(&app, size.0, size.1);
    let rows = text(&buffer);
    let preset = rows
        .iter()
        .position(|row| row.contains("Bass · 8"))
        .expect("line");
    let x = CONTROLS_WIDTH + 12;
    for y in 0..buffer.area.height {
        assert!(
            !matches!(hits.hit_at(x, y), Some(Hit::Point(_))),
            "a point hit at row {y}: {rows:#?}"
        );
    }
    assert!(preset > 0);

    // the EQ turned on unfolds at once
    let mut eq = app.host.computer.eq.as_deref().cloned().expect("eq");
    eq.on = true;
    app.host.computer.eq = Some(Box::new(eq));
    assert!(!app.tone_folded());
    app.tone.row = ToneRow::Mode;
    app.key(crossterm::event::KeyEvent::from(KeyCode::Down));
    assert!(matches!(app.tone.row, ToneRow::Point(_)));
}

/// The loudness readout never takes a point's row: with the EQ on and a short pane there is no
/// readout line, and the table holds as many points as it does with the readout's rows not asked
/// for; in a tall pane, where every point is in view, the readout shows.
#[test]
fn the_loudness_readout_never_costs_a_point_row() {
    let size = (80, 24);
    let points = |app: &App| {
        pane_rows(app, size)
            .iter()
            .filter(|row| row.contains(" Hz") && row.contains(" dB"))
            .count()
    };
    let app = fold_app(true, 8, size);
    let rows = pane_rows(&app, size);
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("at ") && row.contains("bass")),
        "{rows:#?}"
    );
    // the same pane with no loudness stage keeps one more row for the table than the row
    // itself leaves it
    let with = points(&app);
    let mut bare = fold_app(true, 8, size);
    let mut eq = bare.host.computer.eq.as_deref().cloned().expect("eq");
    eq.loudness = None;
    bare.host.computer.eq = Some(Box::new(eq));
    assert_eq!(
        points(&bare),
        with + 1,
        "only the Loudness row itself takes a row"
    );

    let tall = (80, 50);
    let app = fold_app(true, 8, tall);
    let rows = pane_rows(&app, tall);
    assert!(
        rows.iter()
            .any(|row| row.contains("at ") && row.contains("bass")),
        "{rows:#?}"
    );
}

/// The computer's pane, EQ off with `points` points, drawn at `size` while the DAC side is shown
/// (a band focused), so the app knows the pane's size when the side is flipped.
fn dac_side_then_flip_app(points: usize, size: (u16, u16)) -> App {
    let mut app = mockup_app();
    let mut eq = crate::tui::test_support::computer_eq_stages(crate::tui::app::Space::Crossfeed);
    eq.on = false;
    eq.points.truncate(points);
    app.host.computer.eq = Some(eq);
    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Band;
    drop(frame(&app, size.0, size.1));
    assert!(!app.on_computer());
    app
}

/// Whether a key's intents write a point.
fn writes_a_point(intents: &[crate::tui::app::Intent]) -> bool {
    format!("{intents:?}").contains("Set {")
}

/// A `C` flip from the DAC side onto a folded table leaves no point focused, and the keys that
/// edit a point do nothing to a hidden one.
#[test]
fn flipping_to_a_folded_table_leaves_no_point_focused() {
    let size = (80, 24);
    let mut app = dac_side_then_flip_app(8, size);
    app.flip_side();
    assert!(app.on_computer());
    assert!(app.tone_folded());
    assert_eq!(app.tone.row, ToneRow::Mode);
    for code in [KeyCode::Right, KeyCode::Left, KeyCode::Char('.')] {
        let before = app.computer_points();
        let intents = app.key(crossterm::event::KeyEvent::from(code));
        assert!(!writes_a_point(&intents), "{code:?}: {intents:?}");
        assert_eq!(app.computer_points(), before);
    }
}

/// A resize that folds the table under a focused point gives the cursor to Mode before the next
/// key acts.
#[test]
fn a_resize_that_folds_the_table_takes_the_cursor_off_a_point() {
    let mut app = fold_app(false, 8, (80, 50));
    app.pane = Pane::Tone;
    app.focus_point(3);
    assert!(!app.tone_folded());
    drop(frame(&app, 80, 24));
    assert!(app.tone_folded());
    let before = app.computer_points();
    let intents = app.key(crossterm::event::KeyEvent::from(KeyCode::Right));
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert!(!writes_a_point(&intents), "{intents:?}");
    assert_eq!(app.computer_points(), before);
    // `a` on a folded table adds nothing out of sight
    let intents = app.key(crossterm::event::KeyEvent::from(KeyCode::Char('a')));
    assert!(intents.is_empty(), "{intents:?}");
    assert_eq!(app.tone.row, ToneRow::Mode);
}

/// The pane's lines as plain text at an exact interior size, whatever the terminal could give.
fn lines_at(app: &App, size: (usize, usize)) -> Vec<String> {
    let theme = Theme::new(false);
    let eq = app.profile.eq.as_ref().expect("a tone control");
    super::super::tone_pane::tone_lines_in(app, eq, &theme, size)
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

/// The fold counts exactly the rows the layout draws: with no Crossfeed row there is no blank row
/// over what is under the table, so the table folds only below `6 + points` rows (Loudness and its
/// readout) and, with a Crossfeed row and no Loudness, below `4 + points + 4`.
#[test]
fn the_fold_is_decided_at_the_exact_height_the_rows_need() {
    for (loudness, crossfeed, need) in [(true, false, 6 + 8), (false, true, 4 + 8 + 4)] {
        let mut app = fold_app(false, 8, (80, 24));
        let mut eq = app.host.computer.eq.as_deref().cloned().expect("eq");
        if !loudness {
            eq.loudness = None;
        }
        if !crossfeed {
            eq.crossfeed = None;
        }
        app.host.computer.eq = Some(Box::new(eq));
        for height in need - 1..=need {
            let folded = crate::tui::screens::device::computer_folds(&app, (80, height));
            assert_eq!(
                folded,
                height < need,
                "{loudness} {crossfeed}: pane {height}"
            );
            let shown = lines_at(&app, (80, height));
            assert_eq!(
                shown.iter().any(|row| row.contains("Freq")),
                !folded,
                "{shown:#?}"
            );
            assert!(shown.len() <= height || folded, "{shown:#?}");
            if loudness && !folded {
                assert!(shown.iter().any(|row| row.contains("at ")), "{shown:#?}");
            }
        }
    }
}

/// A folded table says what it holds: `1 point`, `custom · N points` for no preset name, and just
/// `custom` for no points and no name.
#[test]
fn a_folded_preset_line_counts_points_and_names_custom() {
    let preset_line = |points: usize, name: Option<&str>| {
        let mut app = fold_app(false, points, (80, 24));
        let mut eq = app.host.computer.eq.as_deref().cloned().expect("eq");
        eq.preset = name.map(str::to_string);
        app.host.computer.eq = Some(Box::new(eq));
        assert!(crate::tui::screens::device::computer_folds(&app, (80, 6)));
        lines_at(&app, (80, 6))
            .into_iter()
            .find(|row| row.contains("Preset") && !row.contains('\u{2039}'))
            .expect("the Preset line")
            .trim_end()
            .to_string()
    };
    assert!(preset_line(1, Some("Bass")).ends_with("Bass \u{b7} 1 point"));
    assert!(preset_line(8, None).ends_with("custom \u{b7} 8 points"));
    assert_eq!(preset_line(0, None).trim_start(), "Preset  custom");
    assert!(preset_line(0, Some("Bass")).ends_with("Preset  Bass"));
}

/// The rows under the computer's table follow it, one blank row between, and the pane's spare
/// rows fall at its bottom: a short table leaves no empty rows before Loudness.
#[test]
fn the_rows_under_the_table_follow_it_and_the_spare_rows_fall_at_the_bottom() {
    let size = (80, 24);
    let app = fold_app(true, 3, size);
    let rows = pane_rows(&app, size);
    let last_point = rows
        .iter()
        .rposition(|row| row.contains(" Hz") && row.contains(" dB"))
        .expect("a point");
    let loudness = rows
        .iter()
        .position(|row| row.contains("Loudness"))
        .expect("the Loudness row");
    assert_eq!(loudness, last_point + 2, "one blank row only: {rows:#?}");
    assert!(rows[last_point + 1].trim().is_empty(), "{rows:#?}");
    // the Space row and its sub-rows follow the readout with no gap, then the pane's spare rows
    let preset = rows
        .iter()
        .rposition(|row| row.contains("Preset"))
        .expect("the crossfeed Preset row");
    assert!(
        rows[preset + 1..preset + 3]
            .iter()
            .all(|row| row.trim().is_empty()),
        "the pane's last rows are blank: {rows:#?}"
    );
    // the lines themselves stop at the last row they hold
    let lines = lines_at(&app, (38, 18));
    assert!(lines.len() < 18, "{lines:#?}");
    assert!(lines[lines.len() - 1].contains("Preset"), "{lines:#?}");
}

/// In a tall pane with the EQ on and the Space off, the bottom-right corner of the Tone pane is
/// free for the squirrel's home.
#[test]
fn a_tall_pane_with_the_eq_on_leaves_the_squirrel_its_corner() {
    for size in [(80, 40), (120, 50)] {
        let mut app = mockup_app();
        let mut eq = crate::tui::test_support::computer_eq_stages(crate::tui::app::Space::Off);
        eq.on = true;
        eq.points.truncate(3);
        app.host.computer.eq = Some(eq);
        app.flip_side();
        drop(frame(&app, size.0, size.1));
        assert!(
            crate::tui::decoration::mascot::draw::home_has_room(
                Rect::new(0, 0, size.0, size.1),
                &app,
                &truecolour_theme(),
            ),
            "{size:?}: {:#?}",
            pane_rows(&app, size)
        );
    }
}

/// With more points than fit, the table scrolls and every row under it is drawn, down to the
/// pane's last row, as before.
#[test]
fn a_table_longer_than_the_pane_still_fills_it_with_the_rows_under_it() {
    let app = fold_app(true, 8, (80, 24));
    let lines = lines_at(&app, (38, 16));
    assert_eq!(lines.len(), 16, "{lines:#?}");
    let tail: Vec<&String> = lines[lines.len() - 5..].iter().collect();
    assert!(tail[0].trim().is_empty(), "{lines:#?}");
    assert!(tail[1].contains("Loudness"), "{lines:#?}");
    assert!(tail[2].contains("Space"), "{lines:#?}");
    assert!(tail[3].contains("Model"), "{lines:#?}");
    assert!(tail[4].contains("Preset"), "{lines:#?}");
}

/// A click on the Loudness row and on the Space row, where the rows now sit, reaches them.
#[test]
fn a_click_on_the_loudness_and_space_rows_at_their_new_lines_acts_on_them() {
    use crate::tui::mouse::MouseAction;
    let size = (80, 24);
    let app = fold_app(true, 3, size);
    let (buffer, hits) = crate::tui::test_support::drawn(&app, size.0, size.1);
    let rows = text(&buffer);
    let at = |word: &str| {
        u16::try_from(
            rows.iter()
                .position(|row| row.contains(word))
                .unwrap_or_else(|| panic!("a {word} row: {rows:#?}")),
        )
        .expect("row")
    };
    let x = CONTROLS_WIDTH + 4;
    let loudness = at("Loudness");
    // right under the table: the last point's row, a blank row, then Loudness
    let last_point = rows[..usize::from(loudness)]
        .iter()
        .rposition(|row| row.contains(" Hz "))
        .expect("a point row above Loudness");
    assert_eq!(usize::from(loudness), last_point + 2, "{rows:#?}");
    assert!(matches!(hits.hit_at(x, loudness), Some(Hit::Ld(_))));
    let space = rows
        .iter()
        .position(|row| row.contains("Crossfeed") && row.contains('\u{2039}'))
        .map(|y| u16::try_from(y).expect("row"))
        .expect("the Space row");
    assert!(matches!(hits.hit_at(x, space), Some(Hit::Crossfeed(_))));
    for y in [loudness, space] {
        let action = crate::tui::mouse::resolve(&hits, crate::tui::test_support::click(x, y), &app);
        assert!(
            matches!(
                action,
                Some(MouseAction::Ld { .. } | MouseAction::Crossfeed { .. })
            ),
            "row {y}: {action:?}"
        );
    }
}

/// With loudness and no crossfeed, the Loudness row follows the table after one blank row where
/// the pane has a row to spare, and the leftover rows fall at the bottom.
#[test]
fn a_loudness_only_pane_keeps_its_blank_row_under_the_table() {
    let mut app = fold_app(true, 3, (80, 24));
    let mut eq = app.host.computer.eq.as_deref().cloned().expect("eq");
    eq.crossfeed = None;
    app.host.computer.eq = Some(Box::new(eq));
    let shown = lines_at(&app, (38, 16));
    let loudness = shown
        .iter()
        .position(|row| row.contains("Loudness"))
        .expect("a Loudness row");
    let last_point = shown[..loudness]
        .iter()
        .rposition(|row| row.contains(" Hz "))
        .expect("a point row");
    assert_eq!(loudness, last_point + 2, "{shown:#?}");
    assert!(shown[last_point + 1].trim().is_empty(), "{shown:#?}");
    assert!(
        shown.len() < 16,
        "the leftover rows are not drawn as padding: {shown:#?}"
    );
}
