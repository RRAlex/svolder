// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The xterm colour tables [`theme`](super::theme) reduces through: the 16 ANSI colours by
//! name and RGB, hex parsing, and the two colour-depth quantisations (the 256-colour
//! cube, and nearest-ANSI-16 by RGB distance), and which of those depths the running
//! terminal supports.

use ratatui::style::Color;
use svolder_ipc::ColorDepth;

/// One of the 16 ANSI colours: the name a `[tui]` `accent` spells it with (without a
/// `bright-` prefix), the ratatui slot it draws as, and its xterm RGB.
struct Ansi {
    name: &'static str,
    bright: bool,
    color: Color,
    rgb: (u8, u8, u8),
}

/// The 16 ANSI colours, in xterm's own 0..16 order: the eight base colours, then their
/// `bright-` forms in the same order.
///
/// `white` draws as [`Color::Gray`] (ANSI 7) and `bright-white` as [`Color::White`] (ANSI 15);
/// `bright-black` draws as [`Color::DarkGray`] (ANSI 8). Every other colour's `bright-` form is
/// ratatui's `Light*` variant.
const ANSI16: [Ansi; 16] = [
    Ansi {
        name: "black",
        bright: false,
        color: Color::Black,
        rgb: (0, 0, 0),
    },
    Ansi {
        name: "red",
        bright: false,
        color: Color::Red,
        rgb: (205, 0, 0),
    },
    Ansi {
        name: "green",
        bright: false,
        color: Color::Green,
        rgb: (0, 205, 0),
    },
    Ansi {
        name: "yellow",
        bright: false,
        color: Color::Yellow,
        rgb: (205, 205, 0),
    },
    Ansi {
        name: "blue",
        bright: false,
        color: Color::Blue,
        rgb: (0, 0, 238),
    },
    Ansi {
        name: "magenta",
        bright: false,
        color: Color::Magenta,
        rgb: (205, 0, 205),
    },
    Ansi {
        name: "cyan",
        bright: false,
        color: Color::Cyan,
        rgb: (0, 205, 205),
    },
    Ansi {
        name: "white",
        bright: false,
        color: Color::Gray,
        rgb: (229, 229, 229),
    },
    Ansi {
        name: "black",
        bright: true,
        color: Color::DarkGray,
        rgb: (127, 127, 127),
    },
    Ansi {
        name: "red",
        bright: true,
        color: Color::LightRed,
        rgb: (255, 0, 0),
    },
    Ansi {
        name: "green",
        bright: true,
        color: Color::LightGreen,
        rgb: (0, 255, 0),
    },
    Ansi {
        name: "yellow",
        bright: true,
        color: Color::LightYellow,
        rgb: (255, 255, 0),
    },
    Ansi {
        name: "blue",
        bright: true,
        color: Color::LightBlue,
        rgb: (92, 92, 255),
    },
    Ansi {
        name: "magenta",
        bright: true,
        color: Color::LightMagenta,
        rgb: (255, 0, 255),
    },
    Ansi {
        name: "cyan",
        bright: true,
        color: Color::LightCyan,
        rgb: (0, 255, 255),
    },
    Ansi {
        name: "white",
        bright: true,
        color: Color::White,
        rgb: (255, 255, 255),
    },
];

/// The 6 levels a 256-colour cube channel is quantised to.
const CUBE_STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The ratatui colour and RGB an ANSI colour name (`black` .. `white`, its `bright-` form
/// too, case-insensitive) draws as; `None` for anything else.
pub(super) fn named(name: &str) -> Option<(Color, (u8, u8, u8))> {
    let lower = name.to_ascii_lowercase();
    let (bright, base) = match lower.strip_prefix("bright-") {
        Some(rest) => (true, rest),
        None => (false, lower.as_str()),
    };
    ANSI16
        .iter()
        .find(|entry| entry.bright == bright && entry.name == base)
        .map(|entry| (entry.color, entry.rgb))
}

/// The RGB a `#rrggbb` accent names, or `None` for anything shorter, longer, or with a
/// non-hex digit.
pub(super) fn hex(text: &str) -> Option<(u8, u8, u8)> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&text[at..at + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// `rgb` as the `#rrggbb` an accent is written with, which is what [`hex`] reads back.
pub(super) fn hex_text((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The RGB a ratatui colour draws as: exact for [`Color::Rgb`], the ANSI table for the 16
/// named colours, and a mid grey for anything else (this codebase never reduces through an
/// [`Color::Indexed`] stop or accent).
pub(super) fn rgb_of(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        named => ANSI16
            .iter()
            .find(|entry| entry.color == named)
            .map_or((127, 127, 127), |entry| entry.rgb),
    }
}

/// The 16-colour ANSI colour nearest `rgb`, by squared RGB distance against the xterm table.
pub(super) fn nearest_ansi16(rgb: (u8, u8, u8)) -> Color {
    ANSI16
        .iter()
        .min_by_key(|entry| distance_sq(entry.rgb, rgb))
        .map_or(Color::White, |entry| entry.color)
}

/// The 256-colour cube index nearest `rgb`: `16 + 36r + 6g + b`, each channel quantised to
/// the nearest of [`CUBE_STEPS`].
pub(super) fn nearest_cube(rgb: (u8, u8, u8)) -> Color {
    let quantise = |c: u8| -> u16 {
        CUBE_STEPS
            .iter()
            .enumerate()
            .min_by_key(|&(_, &step)| (i32::from(step) - i32::from(c)).abs())
            .map_or(0, |(i, _)| i as u16)
    };
    let (r, g, b) = (quantise(rgb.0), quantise(rgb.1), quantise(rgb.2));
    Color::Indexed((16 + 36 * r + 6 * g + b) as u8)
}

/// A palette's three background greys — the ground, the pane and the raised box — at 256
/// colours, among all 240 entries above the ANSI sixteen (the cube and the grey ramp, where
/// [`nearest_cube`] has no grey darker than `#5f`): the raised grey's nearest entry, then the pane's nearest among the entries darker
/// than that, then the ground's among those darker still. Greys too close for their nearest
/// entries to part stay three greys, in their own order; one with no darker entry left takes
/// its plain nearest.
pub(super) fn surfaces_256(greys: [(u8, u8, u8); 3]) -> [Color; 3] {
    let lightness = |index: u8| {
        let (r, g, b) = entry_256(index);
        u16::from(r) + u16::from(g) + u16::from(b)
    };
    let below = |rgb: (u8, u8, u8), above: u8| {
        nearest_256_of(rgb, |index| lightness(index) < lightness(above))
            .or_else(|| nearest_256_of(rgb, |_| true))
            .unwrap_or(16)
    };
    let [ground, pane, raised] = greys;
    let raised = nearest_256_of(raised, |_| true).unwrap_or(16);
    let pane = below(pane, raised);
    let ground = below(ground, pane);
    [ground, pane, raised].map(Color::Indexed)
}

/// The 256-colour entry nearest the grey `level`, among all 240 above the ANSI sixteen, so a grey
/// keeps the ramp's steps rather than the cube's few.
pub(super) fn nearest_grey_256(level: u8) -> Color {
    Color::Indexed(nearest_256_of((level, level, level), |_| true).unwrap_or(16))
}

/// The index nearest `rgb` among the 240 entries above the ANSI sixteen that `keep` lets
/// through, or `None` where it lets none through.
fn nearest_256_of(rgb: (u8, u8, u8), keep: impl Fn(u8) -> bool) -> Option<u8> {
    (16..=255u8)
        .filter(|&index| keep(index))
        .min_by_key(|&index| distance_sq(entry_256(index), rgb))
}

/// The RGB of the 256-colour entry `index`, one of the 240 above the ANSI sixteen: a cube
/// entry, or a level of the grey ramp.
fn entry_256(index: u8) -> (u8, u8, u8) {
    match index {
        232.. => {
            let level = 8 + 10 * (index - 232);
            (level, level, level)
        }
        _ => {
            let at = usize::from(index.saturating_sub(16));
            (
                CUBE_STEPS[at / 36],
                CUBE_STEPS[at / 6 % 6],
                CUBE_STEPS[at % 6],
            )
        }
    }
}

/// How far apart two colours are, as the Euclidean distance between their RGB triples: how
/// near one colour comes to another, for a caller that has to draw them apart when they are
/// too close to tell by colour alone.
pub(super) fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    f64::from(distance_sq(a, b)).sqrt()
}

/// The squared Euclidean distance between two RGB triples, which is enough to compare
/// distances without a square root.
fn distance_sq(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let d = |x: u8, y: u8| i32::from(x) - i32::from(y);
    let (dr, dg, db) = (d(a.0, b.0), d(a.1, b.1), d(a.2, b.2));
    dr * dr + dg * dg + db * db
}

/// The colour depth the running terminal supports, from its own `COLORTERM` and `TERM`.
pub(super) fn detect_depth() -> ColorDepth {
    detect_depth_from(
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
}

/// The colour depth `colorterm` and `term` (a terminal's `COLORTERM` and `TERM`) mean: a
/// `COLORTERM` of `truecolor` or `24bit`, or a `TERM` ending in `-direct`, means true colour; a
/// `TERM` containing `256color` means the 256-colour palette; anything else means the 16 ANSI
/// colours.
pub(super) fn detect_depth_from(colorterm: Option<&str>, term: Option<&str>) -> ColorDepth {
    let truecolor = matches!(colorterm, Some("truecolor" | "24bit"))
        || term.is_some_and(|t| t.ends_with("-direct"));
    if truecolor {
        return ColorDepth::TrueColor;
    }
    if term.is_some_and(|t| t.contains("256color")) {
        return ColorDepth::Ansi256;
    }
    ColorDepth::Ansi16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_256_keeps_greys_too_close_for_their_nearest_entries_apart_and_in_order() {
        // Each grey's nearest is 232, 232 and 233: the ground and the pane would merge.
        let close = [(0x07, 0x06, 0x05), (0x0b, 0x0a, 0x08), (0x10, 0x0e, 0x0b)];
        assert_eq!(
            surfaces_256(close),
            [16, 232, 233].map(Color::Indexed),
            "the pane steps under the raised box and the ground under the pane"
        );
        // Greys whose nearest entries already part keep them.
        let apart = [(0x0c, 0x0c, 0x0c), (0x14, 0x14, 0x14), (0x1e, 0x1e, 0x1e)];
        assert_eq!(surfaces_256(apart), [232, 233, 234].map(Color::Indexed));
        // A pane with no entry darker than the raised box's left takes its plain nearest.
        let black = [(0, 0, 0), (0, 0, 0), (0, 0, 0)];
        assert_eq!(surfaces_256(black), [16, 16, 16].map(Color::Indexed));
    }

    #[test]
    fn named_maps_the_eight_colours_and_their_bright_forms() {
        assert_eq!(named("blue"), Some((Color::Blue, (0, 0, 238))));
        assert_eq!(named("WHITE"), Some((Color::Gray, (229, 229, 229))));
        assert_eq!(named("bright-white"), Some((Color::White, (255, 255, 255))));
        assert_eq!(
            named("bright-black"),
            Some((Color::DarkGray, (127, 127, 127)))
        );
        assert_eq!(named("bright-red"), Some((Color::LightRed, (255, 0, 0))));
        assert_eq!(named("chartreuse"), None);
    }

    #[test]
    fn hex_parses_six_hex_digits_with_or_without_the_hash() {
        assert_eq!(hex("#5fafff"), Some((0x5f, 0xaf, 0xff)));
        assert_eq!(hex("5FAFFF"), Some((0x5f, 0xaf, 0xff)));
        assert_eq!(hex("#5faff"), None, "five digits is not a colour");
        assert_eq!(hex("#5faffz"), None, "z is not a hex digit");
    }

    #[test]
    fn the_cube_reduction_matches_the_documented_example() {
        assert_eq!(nearest_cube((0x5f, 0xaf, 0xff)), Color::Indexed(75));
    }

    #[test]
    fn the_nearest_ansi16_reduction_matches_the_documented_example() {
        assert_eq!(nearest_ansi16((0x5f, 0xaf, 0xff)), Color::LightBlue);
    }

    #[test]
    fn distance_measures_how_near_one_colour_comes_to_another() {
        // The `red` preset's DAC accent against its alert colour, and the `amber` preset's.
        assert!((distance((0xe0, 0x34, 0x1c), (0xff, 0x5a, 0x4a)) - 67.2).abs() < 0.1);
        assert!((distance((0xe0, 0xa0, 0x30), (0xff, 0x3b, 0x30)) - 105.7).abs() < 0.1);
        assert_eq!(distance((1, 2, 3), (1, 2, 3)), 0.0);
    }
}
