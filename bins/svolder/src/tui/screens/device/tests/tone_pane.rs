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
        let cell = buffer.cell((47, 10)).expect("a band cell");
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
    let line = tone_lines(app, &eq, &theme, width, widgets::BAR_MAX).lines[2].clone();
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
    let line = tone_lines(&app, &eq, &theme, 60, widgets::BAR_MAX).lines[2].clone();
    assert_eq!(line.spans.iter().map(Span::width).sum::<usize>(), 0);
}
