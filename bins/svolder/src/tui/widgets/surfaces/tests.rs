// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use svolder_ipc::ColorDepth;

use super::*;
use crate::tui::test_support::theme_for;

/// Dimming keeps every glyph and the background outside the box, clears every attribute of its
/// own, and draws it in the dim role — the preset grey at true colour, the dim attribute
/// at 16 colours and in monochrome — and leaves the cells inside the box as they were.
#[test]
fn dimming_keeps_the_ground_and_clears_the_attributes_outside_the_box_only() {
    use ratatui::style::Modifier;
    use svolder_ipc::TuiConfig;

    let keep = Rect::new(2, 1, 3, 2);
    for (what, theme) in [
        (
            "true colour",
            Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor),
        ),
        ("16 colours", Theme::new(false)),
        ("monochrome", Theme::new(true)),
    ] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 4));
        let loud = Style::default()
            .fg(Color::Yellow)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED);
        for y in 0..4 {
            for x in 0..8 {
                buffer
                    .cell_mut((x, y))
                    .expect("a cell")
                    .set_symbol("x")
                    .set_style(loud);
            }
        }
        dim_around(&mut buffer, &[keep], &theme);
        let outside = buffer.cell((0, 0)).expect("a cell outside the box");
        assert_eq!(outside.symbol(), "x", "{what}: the glyph went");
        assert_eq!(
            outside.bg,
            Color::Blue,
            "{what}: the background was cleared"
        );
        assert!(
            !outside
                .modifier
                .intersects(Modifier::BOLD | Modifier::REVERSED),
            "{what}: {:?} stayed",
            outside.modifier
        );
        match theme.style(Role::Dim).fg {
            Some(grey) => assert_eq!(outside.fg, grey, "{what}"),
            None => {
                assert_eq!(outside.fg, Color::Reset, "{what}");
                assert!(outside.modifier.contains(Modifier::DIM), "{what}");
            }
        }
        let inside = buffer.cell((3, 2)).expect("a cell inside the box");
        assert_eq!(
            (inside.fg, inside.bg),
            (Color::Yellow, Color::Blue),
            "{what}: the box was dimmed"
        );
        assert!(
            inside
                .modifier
                .contains(Modifier::BOLD | Modifier::REVERSED)
        );
    }
}

/// The truecolor theme the fade cases draw with.
fn fade_theme() -> Theme {
    theme_for("amber", ColorDepth::TrueColor)
}

/// The row styles are unchanged through a distance of 2, then blend `0.33 × (d − 2) / 8` of the
/// way to the palette's dim grey, capped at 0.33 from 10 on. The amber text colour #f2f0ea
/// (242, 240, 234) toward its dim grey #8a8a8a (138) gives these literals, each channel
/// `from + (138 - from) × t` rounded half away from zero, as `f64::round` does:
/// t = 0.04125 gives (238, 236, 230), 0.165 gives (225, 223, 218), 0.33 gives (208, 206, 202).
#[test]
fn fade_blends_toward_the_dim_grey_with_distance() {
    let theme = fade_theme();
    assert_eq!(theme.style(Role::Dim).fg, Some(Color::Rgb(138, 138, 138)));
    let style = Style::default().fg(Color::Rgb(0xf2, 0xf0, 0xea));
    for d in [0, 1, 2] {
        assert_eq!(fade(style, d, &theme), style, "distance {d}");
    }
    for (d, rgb) in [
        (3, (238, 236, 230)),
        (6, (225, 223, 218)),
        (10, (208, 206, 202)),
        (20, (208, 206, 202)),
    ] {
        assert_eq!(
            fade(style, d, &theme).fg,
            Some(Color::Rgb(rgb.0, rgb.1, rgb.2)),
            "distance {d}"
        );
    }
}

/// A style with no colour of its own fades from the theme's text colour, and the modifiers it
/// carries stay.
#[test]
fn fade_starts_a_plain_style_from_the_text_colour() {
    let theme = fade_theme();
    let faded = fade(Style::default().add_modifier(Modifier::BOLD), 10, &theme);
    assert_ne!(faded.fg, None);
    assert_ne!(faded.fg, theme.style(Role::Text).fg);
    assert!(faded.add_modifier.contains(Modifier::BOLD));
}

/// Draw `rows` single-cell rows into a buffer, fade them against `selected`, and answer the
/// foreground of each.
fn faded_column(on: bool, theme: &Theme, selected: usize, top: usize, rows: usize) -> Vec<Color> {
    let area = Rect::new(0, 0, 3, u16::try_from(rows).unwrap());
    let mut buffer = Buffer::empty(area);
    buffer.set_style(area, Style::default().fg(Color::Rgb(0xc8, 0x64, 0x20)));
    for y in 0..area.height {
        buffer[(0, y)].set_symbol("x");
    }
    fade_list(&mut buffer, on, area, top, rows, selected, theme);
    (0..area.height).map(|y| buffer[(0, y)].fg).collect()
}

/// The rows of a list fade by their distance from the selected index, which a window that has
/// scrolled counts from the index rather than from its own top.
#[test]
fn fade_list_counts_distance_from_the_selected_index() {
    let theme = fade_theme();
    let plain = Color::Rgb(0xc8, 0x64, 0x20);
    let fgs = faded_column(true, &theme, 20, 14, 12);
    // Indices 14..26; the selection is 20, so rows 18..=22 are within two of it.
    for index in 18..=22 {
        assert_eq!(fgs[index - 14], plain, "index {index}");
    }
    assert_ne!(fgs[0], plain, "six away");
    assert_ne!(fgs[11], plain, "five away");
    assert_ne!(fgs[0], fgs[3], "six away is fainter than three away");
}

/// Nothing fades with the switch off, below true colour, or in monochrome.
#[test]
fn fade_list_stays_off_outside_the_effects_layer() {
    let plain = Color::Rgb(0xc8, 0x64, 0x20);
    let all_plain = |fgs: Vec<Color>| fgs.iter().all(|fg| *fg == plain);
    assert!(all_plain(faded_column(false, &fade_theme(), 0, 0, 20)));
    for depth in [ColorDepth::Ansi256, ColorDepth::Ansi16] {
        assert!(all_plain(faded_column(
            true,
            &theme_for("amber", depth),
            0,
            0,
            20
        )));
    }
    let mono = Theme::from_config(
        &svolder_ipc::TuiConfig::default(),
        true,
        ColorDepth::TrueColor,
    );
    assert!(all_plain(faded_column(true, &mono, 0, 0, 20)));
}
