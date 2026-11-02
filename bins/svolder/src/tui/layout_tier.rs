// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The two size tiers a screen is laid out in above the smallest terminal it is drawn in.
//!
//! A screen does not stretch with the terminal: it is laid out one way below
//! [`WIDE_FROM`] columns and another from there on, and one way below [`TALL_FROM`] rows and
//! another from there on. The wide tier gives the panes more room and air — a wider Controls
//! pane with longer bars, a column between panes side by side, two cells of blank inside every
//! pane's border, and a body no wider than [`BODY_MAX`] set in the middle of the terminal —
//! and the tall tier gives them rows to breathe in: a blank row at the top of every pane, a row
//! between panes stacked one over the other, and more between the Controls pane's groups.
//! Everything below both, 80×24 up to 119×39, is laid out as an 80×24 terminal is.
//!
//! [`Tier::of`] answers the one question from the frame's own size, and every screen asks it of
//! the area it is drawing, so the layout and everything read back from it — the mouse's hits,
//! the curve's cells, the frames the animation layer traces — agree.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Padding};

use super::screens::device::header_height;

/// Columns from which a terminal is wide.
const WIDE_FROM: u16 = 120;
/// Rows from which a terminal is tall.
const TALL_FROM: u16 = 40;
/// The widest the body below the header rule is drawn, in the wide tier.
const BODY_MAX: u16 = 256;

/// Which tiers a frame falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tier {
    /// At least [`WIDE_FROM`] columns.
    pub wide: bool,
    /// At least [`TALL_FROM`] rows.
    pub tall: bool,
}

impl Tier {
    /// The tiers a frame `area` large falls in.
    pub fn of(area: Rect) -> Tier {
        Tier {
            wide: area.width >= WIDE_FROM,
            tall: area.height >= TALL_FROM,
        }
    }

    /// Columns between two panes that sit side by side.
    pub fn gap(self) -> u16 {
        u16::from(self.wide)
    }

    /// Rows between two panes stacked one over the other.
    pub fn row_gap(self) -> u16 {
        u16::from(self.tall)
    }

    /// Rows kept blank at the top of a pane's content, inside its border.
    pub fn top(self) -> u16 {
        u16::from(self.tall)
    }

    /// The blank a pane keeps inside its border: one column on either side in the wide tier,
    /// which with the blank a row opens and closes on itself — the cursor's own column, a column
    /// title's leading space — sets the text two cells in; and a row at the top in the tall
    /// tier.
    pub fn padding(self) -> Padding {
        let side = u16::from(self.wide);
        Padding::new(side, side, self.top(), 0)
    }

    /// The same blank for content that opens and closes on no blank of its own, such as the
    /// response chart's labels and curve: two columns on either side in the wide tier.
    pub fn bare_padding(self) -> Padding {
        let side = 2 * u16::from(self.wide);
        Padding::new(side, side, self.top(), 0)
    }

    /// A pane's bordered block, with [`Tier::padding`] inside the border.
    pub fn block(self) -> Block<'static> {
        Block::bordered().padding(self.padding())
    }

    /// Columns a pane's rows keep blank before its right border on top of the padding, for the
    /// rows whose last value runs right up to where the content ends: one in the wide tier, so
    /// they stand off that border by the two cells they stand off the left one by.
    pub fn trail(self) -> usize {
        usize::from(self.wide)
    }

    /// `body` as the wide tier draws it: no wider than [`BODY_MAX`], and centred.
    pub fn body(self, body: Rect) -> Rect {
        if !self.wide || body.width <= BODY_MAX {
            return body;
        }
        Rect {
            x: body.x + (body.width - BODY_MAX) / 2,
            width: BODY_MAX,
            ..body
        }
    }
}

/// The tiers a frame `area` falls in, and the two bands every screen is laid out in over it:
/// the header and the rule under it across the whole width, and the body below them — no wider
/// than the tier lets it be — down to the terminal's last row.
pub fn split(area: Rect) -> (Tier, [Rect; 2]) {
    let tier = Tier::of(area);
    let [header, body] = Layout::vertical([
        Constraint::Length(header_height(area.width)),
        Constraint::Min(0),
    ])
    .areas(area);
    (tier, [header, tier.body(body)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_start_at_120_columns_and_40_rows() {
        let tier = |width, height| Tier::of(Rect::new(0, 0, width, height));
        assert_eq!(tier(80, 24), Tier::default());
        assert_eq!(tier(119, 39), Tier::default());
        assert_eq!(
            tier(120, 39),
            Tier {
                wide: true,
                tall: false
            }
        );
        assert_eq!(
            tier(119, 40),
            Tier {
                wide: false,
                tall: true
            }
        );
        assert_eq!(
            tier(126, 62),
            Tier {
                wide: true,
                tall: true
            }
        );
    }

    #[test]
    fn a_wide_body_is_held_to_256_columns_in_the_middle() {
        let (_, [header, body]) = split(Rect::new(0, 0, 300, 60));
        assert_eq!(header.width, 300);
        assert_eq!((body.x, body.width), (22, BODY_MAX));
        for width in [126, 200, 256] {
            let (_, [_, uncapped]) = split(Rect::new(0, 0, width, 62));
            assert_eq!((uncapped.x, uncapped.width), (0, width));
        }
    }

    #[test]
    fn the_body_runs_to_the_last_row() {
        for (width, height) in [(80, 24), (126, 30), (100, 50), (200, 60), (300, 60)] {
            let area = Rect::new(0, 0, width, height);
            let (_, [header, body]) = split(area);
            assert_eq!(header.bottom(), body.y, "{width}x{height}");
            assert_eq!(body.bottom(), height, "{width}x{height}");
        }
    }

    #[test]
    fn the_smallest_tier_adds_nothing() {
        let tier = Tier::default();
        assert_eq!(tier.padding(), Padding::ZERO);
        assert_eq!(tier.bare_padding(), Padding::ZERO);
        assert_eq!((tier.gap(), tier.row_gap(), tier.trail()), (0, 0, 0));
    }
}
