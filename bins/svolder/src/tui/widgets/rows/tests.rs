// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;

use ratatui::style::Style;
use svolder_ipc::ColorDepth;

use super::*;
use crate::tui::app::Screen;
use crate::tui::test_support::app_showing;
use crate::tui::test_support::modal_themes;

/// A label column of one cell or less has no room for any text: the draw is the single blank
/// that separates it from the value.
#[test]
fn a_label_field_of_one_cell_or_less_draws_only_the_gap() {
    for field in [0, 1] {
        assert_eq!(label("Volume", field).content, " ", "field {field}");
        assert_eq!(label("世界", field).content, " ", "field {field}");
    }
}

/// A reading of the volume at `raw`, as the app holds one.
fn volume_at(raw: i64) -> Reading {
    Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "10".into(),
            key: Some("volume".into()),
            label: "Volume".into(),
            value_type: "u8".into(),
            raw: serde_json::json!(raw),
            display: raw.to_string(),
            value: None,
            unit: None,
        },
        received: Instant::now(),
        rate: Default::default(),
    }
}

/// The bar cells of a volume row at `raw`, `width` cells long, with the zone past `clip`:
/// each cell's glyph and style.
fn volume_cells(theme: &Theme, raw: i64, width: usize, clip: Option<i64>) -> Vec<(String, Style)> {
    let app = app_showing(Screen::Device);
    let volume = app.volume().expect("the Gungnir 2 has a volume").clone();
    // The label column and the gap before a value, and nothing beside the bar: the row
    // has exactly `width` cells left for it.
    let field = volume.label.chars().count() + 1;
    let columns = Columns::fit(field + VALUE_GAP + width, volume.label.len(), 0, width);
    assert_eq!(columns.bar(), width);
    let reading = volume_at(raw);
    let line = level_line(
        &volume,
        Some(&reading),
        theme,
        columns,
        None,
        (false, false),
        (clip, None),
    );
    line.spans[1..=width]
        .iter()
        .map(|span| (span.content.to_string(), span.style))
        .collect()
}

/// The clipping-risk zone is every cell the volume bar reaches only past the step a
/// boosting curve clips from, and the tick sits on the cell before it. On a bar with a cell
/// per step, the cell the threshold step fills carries the tick, the one before it keeps the
/// accent's shade and the one the step above fills is the alert's; below the threshold the
/// empty cells up to the tick stay faint and the ones past it take the alert's dim shade —
/// at true colour and at 16 colours alike.
#[test]
fn the_zone_starts_on_the_cell_the_step_past_the_threshold_fills() {
    for (depth, theme) in modal_themes() {
        let alert = theme.style(Role::Alert).fg;
        let faint = own_colours(theme.style(Role::Faint));
        let ground = own_colours(theme.alert_ground());
        assert_ne!(
            ground, faint,
            "{depth}: the zone's ground is the faint grey"
        );
        assert!(alert.is_some() && alert != Some(theme.accent()), "{depth}");

        // At full volume, with step 109 the loudest clear of clipping: the cell step 108
        // fills (index 107) is the accent's, the one step 109 fills carries the tick, and
        // the one step 110 fills is red.
        let full = volume_cells(&theme, 127, 127, Some(109));
        assert_eq!(full[107].0, "█", "{depth}");
        assert_eq!(
            full[107].1.fg,
            bar_shade_style(&theme, 107, 127).fg,
            "{depth}: the cell before the threshold step's"
        );
        assert_eq!(full[108].0, TICK, "{depth}: the threshold step's own cell");
        for (i, (glyph, style)) in full.iter().enumerate().skip(109) {
            assert_eq!(
                (glyph.as_str(), style.fg),
                ("█", alert),
                "{depth}: cell {i}"
            );
        }

        // Below the threshold: filled in the accent, then faint up to the tick, then the
        // zone.
        let quiet = volume_cells(&theme, 100, 127, Some(109));
        for (i, (glyph, style)) in quiet.iter().enumerate() {
            // The raw value is written over the middle of the bar, and the tick is the
            // next case's.
            if (60..67).contains(&i) || i == 108 {
                continue;
            }
            let wanted = match i {
                ..100 => ("█", bar_shade_style(&theme, i, 127)),
                100..108 => ("░", faint),
                _ => ("░", ground),
            };
            assert_eq!(
                (glyph.as_str(), style.fg, style.add_modifier),
                (wanted.0, wanted.1.fg, wanted.1.add_modifier),
                "{depth}: cell {i}"
            );
        }
    }
}

/// Beside an accent near the alert red the zone's filled cells are the medium shade `▓`, a
/// pattern next to the bar's solid `█` where the colour alone would not part them; beside an
/// accent far from it they stay `█`, and in monochrome, where no colour tells the zone from
/// the bar, they are `▓` whatever the palette.
#[test]
fn the_zone_is_patterned_beside_an_accent_near_the_alert_red() {
    let themed = |palette: &str, accent: Option<&str>, no_color: bool, depth| {
        let config = svolder_ipc::TuiConfig {
            palette: palette.to_string(),
            accent: accent.map(str::to_string),
            ..svolder_ipc::TuiConfig::default()
        };
        Theme::from_config(&config, no_color, depth)
    };
    let zone_glyphs = |theme: &Theme| -> Vec<String> {
        let cells = volume_cells(theme, 127, 127, Some(109));
        cells[109..]
            .iter()
            .map(|(glyph, _)| glyph.clone())
            .collect()
    };
    let near = [
        (
            "#e0341c",
            themed("amber", Some("#e0341c"), false, ColorDepth::TrueColor),
        ),
        (
            "red palette",
            themed("red", None, false, ColorDepth::TrueColor),
        ),
    ];
    let mut preamp_16 = themed("amber", None, false, ColorDepth::Ansi16);
    preamp_16.set_family(svolder_profiles::Family::Preamp);
    let near = near
        .into_iter()
        .chain([("16 colours, preamplifier", preamp_16)]);
    for (name, theme) in near {
        let glyphs = zone_glyphs(&theme);
        assert_eq!(glyphs.len(), 18, "{name}");
        assert!(
            glyphs.iter().all(|glyph| glyph == "▓"),
            "{name}: {glyphs:?}"
        );
        let cells = volume_cells(&theme, 127, 127, Some(109));
        assert_eq!(cells[109].1.fg, theme.style(Role::Alert).fg, "{name}");
        // The cells before the zone stay solid.
        assert_eq!(cells[107].0, "█", "{name}");
    }
    let far = [
        (
            "default",
            themed("amber", None, false, ColorDepth::TrueColor),
        ),
        (
            "amber accent",
            themed("amber", Some("#e0a030"), false, ColorDepth::TrueColor),
        ),
        ("16 colours", Theme::new(false)),
    ];
    for (name, theme) in far {
        let glyphs = zone_glyphs(&theme);
        assert!(
            glyphs.iter().all(|glyph| glyph == "█"),
            "{name}: {glyphs:?}"
        );
    }
    let mono = themed("red", None, true, ColorDepth::TrueColor);
    assert!(
        zone_glyphs(&mono).iter().all(|glyph| glyph == "▓"),
        "monochrome"
    );
}

/// The threshold step's cell carries the tick whether the bar has filled it or not, at true
/// colour, at 16 colours and in monochrome — where the zone's colours drop out and the
/// tick's shape is what still marks the step. It is in the warn role while the reading is
/// at or below the step, and in the alert role, bold, once the reading is past it, the
/// weight telling the two apart in monochrome. Never reversed, and on no other cell.
#[test]
fn the_threshold_step_carries_a_tick_at_every_depth() {
    let [true_colour, sixteen] = modal_themes();
    for (depth, theme) in [true_colour, sixteen, ("monochrome", Theme::new(true))] {
        let warn = own_colours(theme.style(Role::Warn));
        let alert = own_colours(theme.style(Role::Alert)).add_modifier(Modifier::BOLD);
        assert_ne!(warn, alert, "{depth}");
        for (raw, wanted) in [(100, warn), (109, warn), (110, alert), (127, alert)] {
            let cells = volume_cells(&theme, raw, 127, Some(109));
            let ticks: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, (glyph, _))| glyph == TICK)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(ticks, [108], "{depth} at {raw}");
            let style = cells[108].1;
            assert_eq!(style, wanted, "{depth} at {raw}: the tick's style");
            assert!(!style.add_modifier.contains(Modifier::REVERSED), "{depth}");
        }
        assert!(
            volume_cells(&theme, 127, 127, None)
                .iter()
                .all(|(glyph, _)| glyph != TICK),
            "{depth}: a tick with no step to warn from"
        );
    }
}

/// The raw value written inside the bar moves aside for the tick where the bar has room,
/// and gives way to it on the one cell they share where it has not. On ten cells `63/127`
/// is centred over cells 2 to 7 and step 109's tick falls on cell 7, so the value moves one
/// cell left; on eleven cells `127/127` sits over cells 2 to 8, a threshold at step 60 puts
/// the tick on cell 4, and neither side has room for all seven characters, so the tick
/// takes the value's `7`.
#[test]
fn the_raw_value_moves_aside_for_the_tick_or_gives_it_the_cell() {
    for (depth, theme) in modal_themes() {
        let moved = volume_cells(&theme, 63, 10, Some(109));
        let drawn: String = moved.iter().map(|(glyph, _)| glyph.as_str()).collect();
        assert_eq!(drawn, "█63/127┃░░", "{depth}");
        // 63 fills four cells: `63/` over the fill, `127` over empty cells.
        for (glyph, style) in &moved[1..4] {
            assert!(
                style.add_modifier.contains(Modifier::REVERSED),
                "{depth}: {glyph}"
            );
        }

        let cells = volume_cells(&theme, 127, 11, Some(60));
        let drawn: String = cells.iter().map(|(glyph, _)| glyph.as_str()).collect();
        assert_eq!(drawn, "██12┃/127██", "{depth}");
        for i in [2, 3, 5, 6, 7, 8] {
            assert!(
                cells[i].1.add_modifier.contains(Modifier::REVERSED),
                "{depth}: the value's cell {i} is no hole in the bar"
            );
        }
    }
}

/// With no step to warn from — no curve boosting, or none in EQ mode — no cell of the bar
/// is the alert's.
#[test]
fn without_a_step_to_warn_from_the_bar_has_no_zone() {
    for (depth, theme) in modal_themes() {
        let alert = theme.style(Role::Alert).fg;
        let ground = own_colours(theme.alert_ground());
        for raw in [0, 100, 127] {
            for (i, (_, style)) in volume_cells(&theme, raw, 127, None).iter().enumerate() {
                assert_ne!(style.fg, alert, "{depth} at {raw}: cell {i}");
                assert_ne!(*style, ground, "{depth} at {raw}: cell {i}");
            }
        }
    }
}

/// The raw value written inside the bar keeps its own rule over the zone: reversed on the
/// alert's red where the bar has filled the cell, in the text role where it has not.
#[test]
fn the_raw_value_over_the_zone_keeps_its_own_contrast() {
    for (depth, theme) in modal_themes() {
        let alert = theme.style(Role::Alert).fg;
        let text = own_colours(theme.style(Role::Text));
        // Eleven cells, the zone taking every one: `127/127` over cells 2 to 8.
        let full = volume_cells(&theme, 127, 11, Some(0));
        let letters: String = full[2..9].iter().map(|(glyph, _)| glyph.as_str()).collect();
        assert_eq!(letters, "127/127", "{depth}");
        for (glyph, style) in &full[2..9] {
            assert_eq!(style.fg, alert, "{depth}: {glyph}");
            assert!(style.add_modifier.contains(Modifier::REVERSED), "{depth}");
        }
        // At 20, the bar fills one cell and `20/127` sits on empty ones.
        let quiet = volume_cells(&theme, 20, 11, Some(0));
        for (glyph, style) in &quiet[2..8] {
            assert_eq!(*style, text, "{depth}: {glyph}");
        }
    }
}

/// `fill`'s arithmetic — `max - min` and the distance from the range's bottom — uses
/// `saturating_sub`, so a range spanning the whole of i64's own bounds does not overflow it.
#[test]
fn fill_does_not_panic_on_a_range_spanning_i64s_bounds() {
    let cells = super::fill(0, (i64::MIN, i64::MAX), 10);
    assert!(
        cells <= 10,
        "fill must stay inside the bar it was asked for: got {cells}"
    );
}

/// `bar_value`'s distance-from-centre subtraction also uses `saturating_sub`, so a centre at
/// one extreme with a raw reading at the other does not overflow it.
#[test]
fn bar_value_does_not_panic_on_a_centre_and_raw_at_opposite_extremes() {
    let control = Control {
        id: svolder_proto::FunctionId::one(1),
        key: "k".into(),
        label: "K".into(),
        kind: ControlKind::Level,
        value_type: svolder_proto::ValueType::I64,
        values: vec![],
        range: Some((i64::MIN, i64::MAX)),
        center: Some(i64::MAX),
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    };
    let reading = Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "1".into(),
            key: Some("k".into()),
            label: "K".into(),
            value_type: "i64".into(),
            raw: serde_json::json!(i64::MIN),
            display: "0".into(),
            value: None,
            unit: None,
        },
        received: Instant::now(),
        rate: Default::default(),
    };
    let text = super::bar_value(&control, Some(&reading));
    assert!(
        text.is_some(),
        "bar_value should still produce text for a pathological centre, not panic"
    );
}

/// A pane's columns never run past its interior: a long label beside a wide value gives up its
/// own width (the label is cut with `…` where it is drawn) before the bar goes under its
/// minimum, and a row that already fits keeps the label column its longest label asks for.
#[test]
fn the_label_gives_way_before_the_bar_and_the_columns_fit_where_the_value_does() {
    // A 32-column row, a 14-letter label and a 12-cell value: the old floor on the bar left the
    // three 7 columns over.
    let columns = Columns::fit(32, 14, 12, 15);
    assert!(
        columns.field() + columns.bar() + VALUE_GAP + 12 <= 32,
        "label {} + bar {} + gap + value 12 run past 32 columns",
        columns.field(),
        columns.bar()
    );
    assert_eq!(columns.bar(), BAR_MIN, "the bar kept its minimum");
    assert!(columns.field() < 13, "the label did not give way");

    for room in 20..=60 {
        for longest in 1..=20 {
            for widest in 0..=14 {
                let columns = Columns::fit(room, longest, widest, 15);
                let wanted = longest.min(LABEL_MAX) + 1;
                if wanted + VALUE_GAP + widest + BAR_MIN <= room {
                    assert_eq!(columns.field(), wanted, "{room}/{longest}/{widest}");
                }
                if LABEL_FLOOR + VALUE_GAP + widest + BAR_MIN <= room {
                    assert!(
                        columns.field() + columns.bar() + VALUE_GAP + widest <= room
                            && columns.bar() >= BAR_MIN,
                        "{room}/{longest}/{widest}: {columns:?}"
                    );
                }
            }
        }
    }
}

/// The value beside a level's bar is written as every front end writes a level: a space before
/// the unit.
#[test]
fn a_level_beside_its_bar_reads_with_a_space_before_its_unit() {
    let profile = crate::tui::test_support::profile();
    let volume = profile.control_by_key("volume").expect("a volume");
    let reading = Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "10".into(),
            key: Some("volume".into()),
            label: "Volume".into(),
            value_type: "u8".into(),
            raw: serde_json::json!(109),
            display: "109 (-6.0 dB)".into(),
            value: Some(-6.021),
            unit: Some("dB".into()),
        },
        received: Instant::now(),
        rate: Default::default(),
    };
    assert_eq!(
        super::bar_value(volume, Some(&reading)).as_deref(),
        Some("-6.0 dB")
    );
}

/// One volume reading at raw `raw`, carrying `value` as its converted value: in-process a
/// conversion's own number, `-inf` at silence; over the daemon's socket `None` at silence.
fn volume_carrying(raw: i64, value: Option<f64>) -> Reading {
    Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "10".into(),
            key: Some("volume".into()),
            label: "Volume".into(),
            value_type: "u8".into(),
            raw: serde_json::json!(raw),
            display: raw.to_string(),
            value,
            unit: Some("dB".into()),
        },
        received: Instant::now(),
        rate: Default::default(),
    }
}

/// The text beside the Volume's bar is built from the raw step, so silence reads `silence`
/// whichever transport the reading came over.
#[test]
fn silence_reads_silence_over_either_transport() {
    let profile = crate::tui::test_support::profile();
    let volume = profile.control_by_key("volume").expect("a volume");
    for value in [Some(f64::NEG_INFINITY), None] {
        let reading = volume_carrying(0, value);
        assert_eq!(
            super::bar_value(volume, Some(&reading)).as_deref(),
            Some("silence"),
            "{value:?}"
        );
    }
}

/// A level's text comes from its raw step, not from the converted value the reading carried.
#[test]
fn a_level_beside_its_bar_reads_from_its_raw_step() {
    let profile = crate::tui::test_support::profile();
    let volume = profile.control_by_key("volume").expect("a volume");
    let reading = volume_carrying(109, None);
    assert_eq!(
        super::bar_value(volume, Some(&reading)).as_deref(),
        Some("-6.0 dB")
    );
}
