// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Svolder wordmark, the product's own identity on the screen.
//!
//! A block-letter lowercase "svolder" in the demoscene tradition, held back to shading
//! characters and a gentle italic: a three-quarter-shade face with a half-shade and a
//! quarter-shade falling one and two cells down-right, so the word stands off the
//! background without an outline. The art is a text asset beside the source, one row per
//! line, every row the same display width and every glyph a single cell, so [`lines`] can
//! hand a row to ratatui whole.
//!
//! Two sizes: [`Size::Large`] for the intro, [`Size::Small`] for the header beside the
//! device name. The mark carries no colour of its own — it is drawn as a two-tone gradient
//! of the active accent, brightest on the top row and half as bright on the last, so it
//! changes with the device family the screen is showing.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::tui::theme::Theme;

/// The eight-row mark the intro shows.
const LARGE: &str = include_str!("../../../assets/wordmark/large.txt");
/// The two-row mark the header carries.
const SMALL: &str = include_str!("../../../assets/wordmark/small.txt");

/// What is left of the accent on a mark's last row, as a fraction of the accent itself.
const FLOOR: f64 = 0.5;

/// Which of the two marks to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// The eight-row mark, drawn by the intro.
    Large,
    /// The two-row mark, drawn in the header beside the device name.
    Small,
}

impl Size {
    /// The art this size is drawn from.
    fn art(self) -> &'static str {
        match self {
            Size::Large => LARGE,
            Size::Small => SMALL,
        }
    }
}

/// How many rows `size` is drawn across, so a caller can lay a screen out around the mark
/// without rendering it.
pub fn height(size: Size) -> u16 {
    size.art().lines().count() as u16
}

/// How many columns `size` is drawn across, the same for every one of its rows, so a caller
/// can leave room for the mark without rendering it.
pub fn width(size: Size) -> u16 {
    size.art()
        .lines()
        .next()
        .map_or(0, |row| row.chars().count() as u16)
}

/// The art `size` is drawn from, one row per line, for what lays the mark out cell by cell.
pub(crate) fn art(size: Size) -> &'static str {
    size.art()
}

/// The mark at `size`, one line per row, ready to render.
///
/// Row `i` of `n` is drawn in [`Theme::accent_shade`] at `1.0 - 0.5 * i / (n - 1)`, which
/// ramps the accent from its full self on the top row down to half on the last at 256 and
/// true colour, and draws flat in the family's own ANSI slot at 16 colours, where there is
/// no room for a ramp. A monochrome theme leaves the glyphs unstyled and lets the shading
/// characters carry the shape on their own.
pub fn lines(size: Size, theme: &Theme) -> Vec<Line<'static>> {
    let rows: Vec<&'static str> = size.art().lines().collect();
    let last = rows.len().saturating_sub(1).max(1) as f64;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let style = match theme.monochrome() {
                true => Style::default(),
                false => Style::default().fg(theme.accent_shade(1.0 - FLOOR * i as f64 / last)),
            };
            Line::from(Span::styled(*row, style))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use svolder_ipc::{ColorDepth, TuiConfig};
    use svolder_profiles::Family;

    use super::*;
    use crate::tui::theme::Theme;

    #[test]
    fn the_large_mark_is_eight_rows_of_equal_width_and_the_small_two() {
        for (size, rows) in [(Size::Large, 8), (Size::Small, 2)] {
            let lines = lines(size, &Theme::new(false));
            assert_eq!(lines.len(), rows);
            let widths: Vec<usize> = lines.iter().map(|l| l.width()).collect();
            assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}");
            assert_eq!(height(size), rows as u16);
            assert_eq!(usize::from(width(size)), widths[0]);
        }
    }

    #[test]
    fn the_mark_is_a_gradient_of_the_accent_at_truecolor_and_flat_at_sixteen() {
        let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
        theme.set_family(Family::Dac);
        let l = lines(Size::Large, &theme);
        let top = l[0].spans[0].style.fg.unwrap();
        let bottom = l[7].spans[0].style.fg.unwrap();
        assert_ne!(top, bottom);
        let sixteen = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
        assert!(
            lines(Size::Large, &sixteen)
                .iter()
                .all(|l| l.spans[0].style.fg == Some(Color::Yellow))
        );
    }
}
