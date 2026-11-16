// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The row table: the golden rows, the keys the table names and the accent rows.

use super::*;

/// Every row of every category as one line (category, label, value, explain and config line,
/// tab-separated, `\n` and `\t` escaped), as the file `rows.golden` holds them.
fn rows_text() -> String {
    let app = plain();
    let escaped = |text: &str| text.replace('\t', "\\t").replace('\n', "\\n");
    let mut out = String::new();
    for (category, rows) in CATEGORIES {
        for setting in rows {
            let fields = [
                category,
                setting.label(),
                &setting.value(&app),
                &explain(*setting),
                &config_line(*setting, &app.tui),
            ];
            let fields: Vec<String> = fields.iter().map(|field| escaped(field)).collect();
            out.push_str(&fields.join("\t"));
            out.push('\n');
        }
    }
    out
}

/// A new row, or a changed label, value, help or config line, shows in `rows.golden` as a
/// diff.
#[test]
fn the_rows_are_the_golden_rows() {
    assert_eq!(rows_text(), include_str!("../rows.golden"));
}

/// No row is listed in two places.
#[test]
fn every_setting_is_listed_once() {
    let all = every_setting();
    for (at, setting) in all.iter().enumerate() {
        assert!(
            !all[..at].contains(setting),
            "{setting:?} is in CATEGORIES twice"
        );
    }
}

/// Every plain key a row names is an entry of the table, and every entry of the table has a
/// row: a `Setting::Key` of its own, or a named row that carries its label.
#[test]
fn the_rows_and_the_table_name_the_same_keys() {
    let rows = every_setting();
    for setting in &rows {
        if let Setting::Key(key) = setting {
            assert!(
                TUI_SETTINGS.iter().any(|entry| entry == key),
                "{key:?} is not an entry of TUI_SETTINGS"
            );
        }
    }
    for entry in TUI_SETTINGS {
        // `accent` has one named row per family and no label of its own.
        if entry.key == "accent" {
            continue;
        }
        assert!(
            rows.iter().any(|setting| match setting {
                Setting::Key(key) => key == &entry,
                named => named.label() == entry.label,
            }),
            "{entry:?} has no row"
        );
    }
}

/// A path under a home named in wide glyphs is fitted by cells: never wider than its room, and
/// a wide glyph straddling the cut is dropped whole rather than split.
#[test]
fn a_path_with_wide_glyphs_is_fitted_by_cells() {
    let path = "/home/世界/.config/svolder/config.toml";
    for room in 0..=widgets::text::cells(path) {
        let fitted = fitted(path, room);
        assert!(
            widgets::text::cells(&fitted) <= room,
            "room {room}: |{fitted}|"
        );
    }
    assert_eq!(fitted(path, 22), "…/svolder/config.toml");
    // A file name of wide glyphs alone keeps its tail, a glyph the room cannot hold whole left
    // out.
    let name = "世界世界世界";
    assert_eq!(fitted(name, 6), "…世界");
    assert_eq!(fitted(name, 4), "…界");
    assert_eq!(fitted(name, 3), "…界");
    assert_eq!(fitted(name, 2), "…");
}

/// An accent the palette's stops lack, written by hand or another palette's, stays a stop of its
/// own ahead of the preset's: Right goes on to the preset's own colour and Left comes back to it;
/// Left from it wraps to the palette's last colour and Right from there comes back.
#[test]
fn an_accent_the_cycle_lacks_stays_a_stop_to_step_back_to() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accents.dac = Some("#e0341c".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(Family::Dac, None))],
    );
    app.tui.accents.dac = None;
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#e0341c".into())
        ))],
        "the colour of the user's own was lost"
    );
    app.tui.accents.dac = Some("#e0341c".into());
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#f6cc7a".into())
        ))],
    );
    app.tui.accents.dac = Some("#f6cc7a".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#e0341c".into())
        ))],
    );
}

/// With a palette name no preset carries the only stops are the user's own colour and the
/// palette's: the arrows go between the two.
#[test]
fn an_accent_row_on_an_unknown_palette_steps_between_the_own_colour_and_the_palettes() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.palette = "no-such-palette".into();
    app.tui.accents.dac = Some("#123456".into());
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(Family::Dac, None))],
    );
    app.tui.accents.dac = None;
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Dac,
            Some("#123456".into())
        ))],
    );
}

/// A bare `accent` that a step clears is held for all three rows: once the file reads `(preset)`
/// on another family's row, the colour is still a stop there.
#[test]
fn a_bare_accent_is_held_for_every_family() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accent = Some("#e0341c".into());
    press(&mut app, KeyCode::Right);
    app.tui.accent = None;
    select(&mut app, Setting::Accent(Family::Amp));
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Tui(TuiChange::Accent(
            Family::Amp,
            Some("#e0341c".into())
        ))],
        "the colour was held for the stepped family only"
    );
}

/// A bare `accent` on file is a colour of the row's own: stepping the row holds it.
#[test]
fn a_bare_accent_is_held_as_the_rows_own_colour() {
    let mut app = app();
    select(&mut app, Setting::Accent(Family::Dac));
    app.tui.accent = Some("#0000ff".into());
    press(&mut app, KeyCode::Right);
    assert_eq!(app.overlays.held_accents.get(Family::Dac), Some("#0000ff"));
}

// The Web UI box.
