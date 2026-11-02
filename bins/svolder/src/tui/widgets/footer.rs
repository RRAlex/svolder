// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The setup wizard's key line: its pieces, the window of them that fits a width, and where the
//! mouse can press each key.
//!
//! The wizard has no app behind it, so its line is built from a [`Footing`] alone. [`draw_footer`]
//! draws it into the bottom row and records every key on it as a mouse target, so a click on a key
//! is the key; the line is measured in cells, so a narrow terminal cuts it at its right edge.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::text::cells;
use crate::tui::app::Screen;
use crate::tui::keys;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::theme::{Role, Theme};

/// What a key line is drawn from, beside the keys themselves.
///
/// Only the setup wizard draws a key line, and it has no app behind it: it says what the keys
/// that are not always worth pressing are answered against for itself.
pub struct Footing {
    /// What the keys that are not always worth pressing are answered against.
    pub scene: keys::Scene,
}

impl Footing {
    /// A key line on a screen like `scene`.
    pub fn resting(scene: keys::Scene) -> Footing {
        Footing { scene }
    }
}

/// The setup wizard's bottom line, and where the mouse can press each key it names.
pub struct Footer {
    /// The line to draw.
    pub line: Line<'static>,
    /// Where each key's label landed on it, for the keys that are on the screen.
    pub labels: Vec<LabelAt>,
}

/// One key's label, as one frame drew it.
pub(crate) struct LabelAt {
    /// Which of the screen's own [`keys::hints_for`] the label is for.
    pub hint: usize,
    /// Its first cell, from the key row's left edge.
    pub x: u16,
    /// How many of its cells are on the screen.
    pub width: u16,
}

/// Draw `screen`'s key line into `area` and record every key on it as a mouse target.
///
/// The setup wizard draws its key row through this, so a click on a key is the key.
pub fn draw_footer(
    frame: &mut Frame,
    area: Rect,
    footing: &Footing,
    theme: &Theme,
    screen: Screen,
    hits: &mut HitMap,
) {
    let footer = footer_line(footing, theme, screen, area.width);
    frame.render_widget(Paragraph::new(footer.line), area);
    for label in footer.labels {
        let target = Rect {
            x: area.x + label.x,
            y: area.y,
            width: label.width,
            height: 1,
        };
        hits.push(target.intersection(area), Hit::Footer(label.hint));
    }
}

/// The setup wizard's bottom line: `screen`'s own name and every key it takes, in key-row order.
///
/// The screen's name draws as a badge, each key's label in the accent (dimmed where
/// [`keys::available`] says it is not worth pressing now), what it does in dim grey.
///
/// The line is cut at the right edge of `width` cells.
pub fn footer_line(footing: &Footing, theme: &Theme, screen: Screen, width: u16) -> Footer {
    window(&pieces(footing, screen), theme, 0, usize::from(width))
}

/// One piece of a key line: what it says, what it means, and the key whose label it is.
struct Piece {
    /// The text itself.
    text: String,
    /// What it draws as, or nothing for the spaces between.
    role: Option<Role>,
    /// Which of the screen's own keys this is the label of.
    hint: Option<usize>,
}

/// The pieces `screen`'s key line is made of, in order and without a theme's own colours, so
/// that its width can be measured wherever there is no theme to hand.
fn pieces(footing: &Footing, screen: Screen) -> Vec<Piece> {
    let mut pieces = vec![
        plain(" "),
        Piece {
            text: screen_name(screen).to_string(),
            role: Some(Role::Badge),
            hint: None,
        },
        plain("  "),
    ];
    // A wide gap separates a screen's own keys (navigation and actions, read as one list) from
    // the keys that switch screens and the ones that mean the same everywhere; a single space
    // separates keys within each group.
    let mut cluster = None;
    for (index, hint) in keys::hints_for(screen).iter().enumerate() {
        let this = hint.group.max(1);
        if let Some(previous) = cluster {
            pieces.push(plain(if this == previous { " " } else { "  " }));
        }
        cluster = Some(this);
        pieces.push(Piece {
            text: hint.label_in(footing.scene).to_string(),
            role: Some(label_role(footing, hint)),
            hint: Some(index),
        });
        pieces.push(plain(" "));
        pieces.push(Piece {
            text: hint.does_in(footing.scene).to_string(),
            role: Some(Role::KeyDesc),
            hint: None,
        });
    }
    pieces
}

/// How one key's label is drawn: a key that is not worth pressing now dimmed, and every other
/// key in the accent.
fn label_role(footing: &Footing, hint: &keys::KeyHint) -> Role {
    match keys::available_in(hint, footing.scene) {
        true => Role::KeyHint,
        false => Role::Faint,
    }
}

/// One piece of the spacing between keys.
fn plain(text: &str) -> Piece {
    Piece {
        text: text.to_string(),
        role: None,
        hint: None,
    }
}

/// The `room` cells of `pieces` from cell `offset` on, as the line to draw and where each key's
/// label landed in it. A wide character cut by either edge of the window is drawn as blanks.
///
/// A piece is measured by the cells of its whole string, while [`slice_cells`] sums the cells of
/// each character; the two agree except for sequences a terminal joins (emoji joined with a
/// zero-width joiner, a variation selector), which no key label holds.
fn window(pieces: &[Piece], theme: &Theme, offset: usize, room: usize) -> Footer {
    let end = offset + room;
    let mut spans = Vec::new();
    let mut labels = Vec::new();
    let mut at = 0;
    for piece in pieces {
        let cells = cells(&piece.text);
        let (start, stop) = (at, at + cells);
        at = stop;
        if stop <= offset || start >= end {
            continue;
        }
        let from = offset.saturating_sub(start);
        let take = cells.min(end - start) - from;
        let text = slice_cells(&piece.text, from, take);
        if let Some(hint) = piece.hint {
            labels.push(LabelAt {
                hint,
                x: u16::try_from(start + from - offset).unwrap_or(0),
                width: u16::try_from(take).unwrap_or(0),
            });
        }
        spans.push(match piece.role {
            Some(role) => Span::styled(text, theme.style(role)),
            None => Span::raw(text),
        });
    }
    Footer {
        line: Line::from(spans),
        labels,
    }
}

/// The part of `text` in display cells `from..from + take`; a wide character with one half
/// outside that range is a blank, so the result is always exactly `take` cells.
fn slice_cells(text: &str, from: usize, take: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    for c in text.chars() {
        let width = cells(c.encode_utf8(&mut [0; 4]));
        let (a, b) = (column, column + width);
        column = b;
        if b <= from || a >= from + take {
            continue;
        }
        match a >= from && b <= from + take {
            true => out.push(c),
            false => out.extend(std::iter::repeat_n(' ', b.min(from + take) - a.max(from))),
        }
    }
    out
}

/// The name a key row's badge gives `screen`.
fn screen_name(screen: Screen) -> &'static str {
    match screen {
        Screen::Device => "Device",
        Screen::Tone => "Tone",
        Screen::Telemetry => "Telemetry",
        Screen::Devices => "Devices",
        Screen::Settings => "Settings",
        Screen::Setup => "Setup",
    }
}

#[cfg(test)]
mod tests;
