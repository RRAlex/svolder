// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch between the device and the computer, `Target ‹ NAME | Computer ›`: one
//! drawing for the Controls pane and the tone-control screen, so their words cannot drift apart.
//! [`styles`] and [`choice`] are the drawing of every choice among words (the computer's
//! `Mode ‹ Off | EQ ›`, the Space and Crossfeed rows too), so none looks different from it.

use ratatui::style::Style;
use ratatui::text::Span;

use crate::tui::app::{App, Side};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

use super::device::cut_cells;

/// What the switch is called: the target of the volume and the tone control together.
pub(crate) const LABEL: &str = "Target";
/// The fewest cells of the device's name the switch is drawn with.
const MIN_NAME: usize = 3;
/// What the switch calls the computer.
pub(crate) const COMPUTER: &str = "Computer";

/// The switch drawn: its spans, and where its two names lie from its start.
pub(crate) struct Switch {
    /// The label, the arrows and the two names.
    pub spans: Vec<Span<'static>>,
    /// Each name's first cell, its width and the side it picks.
    pub names: [(usize, usize, Side); 2],
}

/// Whether `room` cells hold the switch.
pub(crate) fn fits(room: usize) -> bool {
    room.saturating_sub(
        widgets::text::cells(LABEL) + 1 + 2 + 3 + widgets::text::cells(COMPUTER) + 2,
    ) >= MIN_NAME
}

/// The styles of a two-way choice: the label's, the chosen side's and the other side's.
///
/// While the keys are on it (`focused`) the label and the chosen side take the selected role's
/// box and the other side is plain dim with no box, so the side in force is what stands out on a
/// line the cursor is on. Otherwise the label keeps `head`, the chosen side is the select-name
/// role and the other dim. Every two-way choice (`Target`, the computer's `Mode`) takes these.
pub(crate) fn styles(theme: &Theme, head: Style, focused: bool) -> (Style, Style, Style) {
    match focused {
        true => (
            theme.style(Role::Selected),
            theme.style(Role::Selected),
            theme.style(Role::Dim),
        ),
        false => (head, theme.style(Role::SelectName), theme.style(Role::Dim)),
    }
}

/// The spans of `‹ A | B ›` with the second side chosen where `second` holds, the chosen side in
/// `lit` and the other in `off` (from [`styles`]), the arrows and the bar in the arrow role.
pub(crate) fn choice(
    theme: &Theme,
    (a, b): (String, String),
    second: bool,
    (lit, off): (Style, Style),
) -> Vec<Span<'static>> {
    let arrow = theme.style(Role::ModeArrow);
    let (first, last) = match second {
        true => (off, lit),
        false => (lit, off),
    };
    vec![
        Span::styled("‹ ", arrow),
        Span::styled(a, first),
        Span::styled(" | ", arrow),
        Span::styled(b, last),
        Span::styled(" ›", arrow),
    ]
}

/// The switch in `room` cells, the label in `head`; `None` where there is no room for it.
///
/// While the keys are on it (`focused`) the label and the side shown take the selected role's
/// box, and the other side is plain dim with no box, so the side in force is what stands out on
/// a line the cursor is on. Otherwise the side shown is bold and the other dim.
pub(crate) fn draw(
    app: &App,
    theme: &Theme,
    room: usize,
    head: Style,
    focused: bool,
) -> Option<Switch> {
    let product = &app.session.product;
    // the label, a blank, `‹ `, ` | `, the computer, ` ›`
    let fixed = widgets::text::cells(LABEL) + 1 + 2 + 3 + widgets::text::cells(COMPUTER) + 2;
    let left = room.saturating_sub(fixed);
    if left < MIN_NAME {
        return None;
    }
    let label = LABEL;
    let name = cut_cells(product, left);
    let (head, lit, off) = styles(theme, head, focused);
    let first = widgets::text::cells(label) + 1 + 2;
    let (name_w, computer_w) = (widgets::text::cells(&name), widgets::text::cells(COMPUTER));
    let second = first + name_w + 3;
    let mut spans = vec![Span::styled(label.to_string(), head), Span::raw(" ")];
    spans.extend(choice(
        theme,
        (name, COMPUTER.to_string()),
        app.on_computer(),
        (lit, off),
    ));
    Some(Switch {
        spans,
        names: [
            (first, name_w, Side::Dac),
            (second, computer_w, Side::Computer),
        ],
    })
}

/// The words of the switch's place where the computer's controls cannot be used: the label and
/// the reason, dim, cut to `room`. The row is not focusable.
pub(crate) fn unavailable(
    theme: &Theme,
    room: usize,
    head: Style,
    why: &str,
) -> Vec<Span<'static>> {
    let used = widgets::text::cells(LABEL) + 1;
    vec![
        Span::styled(LABEL, head),
        Span::raw(" "),
        Span::styled(
            cut_cells(why, room.saturating_sub(used)),
            theme.style(Role::Dim),
        ),
    ]
}
