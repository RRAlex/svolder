// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The grounds the screens are drawn on and the boxes laid over them: dimming everything around
//! a modal box, fading a list away from its selection, clearing the ground a box or a toast
//! sits on, and painting the theme's greys under a finished frame.
//!
//! Every modal box (help, the editors, confirmations) starts with [`modal_ground`] so it is the
//! one thing on the terminal at full strength, and ends its line of keys with [`modal_keys`];
//! the render pass finishes a screen with [`paint_surfaces`].

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear};
use svolder_ipc::ColorDepth;

use crate::tui::app::App;
use crate::tui::theme::{Role, Surface, Theme};

/// Dim every cell of `buffer` outside `keep`, for a box about to be drawn over the screen (the
/// help overlay, the device editor, a removal's confirmation).
///
/// Every glyph stays; only its foreground changes to the theme's [`Role::Dim`], with every other
/// attribute dropped and the background left as it was painted (the theme's ground or pane
/// grey, or the terminal's own), so the box is the one thing on the terminal at full strength.
/// That's the preset's own grey at 256 and true colour; at 16 colours and in monochrome, which
/// have no grey of their own, it's the terminal's dim attribute over its default colour.
pub fn dim_around(buffer: &mut Buffer, keep: &[Rect], theme: &Theme) {
    let dim = theme.style(Role::Dim);
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if keep.iter().any(|kept| kept.contains(Position::new(x, y))) {
                continue;
            }
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.fg = dim.fg.unwrap_or(Color::Reset);
                cell.modifier = dim.add_modifier;
            }
        }
    }
}

/// How far toward the dim grey a row ten or more rows from the selection fades.
const FADE_MAX: f64 = 0.33;

/// Rows from the selection within which a row keeps its colours.
const FADE_FREE: usize = 2;

/// Rows past [`FADE_FREE`] over which the fade grows to [`FADE_MAX`].
const FADE_SPAN: f64 = 8.0;

/// `style` with its foreground blended toward the theme's dim grey by how far a row `distance`
/// rows from the selected one has faded: `0.33 × min(max(d − 2, 0) / 8, 1)`, so a distance of 2
/// or less leaves it as it is. A style with no foreground fades from the theme's text colour. A
/// foreground that is not a true colour stays as it is.
///
/// A caller applies it only while the list fade is on, at true colour.
pub(crate) fn fade(style: Style, distance: usize, theme: &Theme) -> Style {
    let strength = FADE_MAX * (distance.saturating_sub(FADE_FREE) as f64 / FADE_SPAN).min(1.0);
    if strength == 0.0 {
        return style;
    }
    let Some(Color::Rgb(dr, dg, db)) = theme.style(Role::Dim).fg else {
        return style;
    };
    let from = match style.fg {
        Some(Color::Rgb(r, g, b)) => (r, g, b),
        None | Some(Color::Reset) => match theme.style(Role::Text).fg {
            Some(Color::Rgb(r, g, b)) => (r, g, b),
            _ => return style,
        },
        Some(_) => return style,
    };
    let mix =
        |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * strength).round() as u8;
    style.fg(Color::Rgb(
        mix(from.0, dr),
        mix(from.1, dg),
        mix(from.2, db),
    ))
}

/// Fade the rows of a list already drawn into `buffer`, by their distance from the `selected`
/// index. `rows` is the area whose lines are the list's rows, one line each from index `top`,
/// at most `shown` of them. Nothing happens unless `on` (the fade is on and the effects layer
/// runs) and the theme is true colour with its colours.
///
/// The fade works on the cells, so every span of a row fades whatever style it drew with; the
/// cells' backgrounds and attributes stay.
pub(crate) fn fade_list(
    buffer: &mut Buffer,
    on: bool,
    rows: Rect,
    top: usize,
    shown: usize,
    selected: usize,
    theme: &Theme,
) {
    if !on || theme.monochrome() || theme.depth() != ColorDepth::TrueColor {
        return;
    }
    let rows = rows.intersection(buffer.area);
    for (line, y) in (rows.top()..rows.bottom()).take(shown).enumerate() {
        let distance = (top + line).abs_diff(selected);
        for x in rows.left()..rows.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            if cell.symbol().trim().is_empty() {
                continue;
            }
            let own = match cell.fg {
                Color::Reset => Style::default(),
                fg => Style::default().fg(fg),
            };
            if let Some(fg) = fade(own, distance, theme).fg {
                cell.fg = fg;
            }
        }
    }
}

/// Make ready the cells of `area` for a modal box: everything around it dimmed (see
/// [`dim_around`]), and the box itself cleared and set in the theme's primary text over the
/// raised grey, so every piece of it that names no role of its own reads as text.
pub fn modal_ground(frame: &mut Frame, area: Rect, theme: &Theme) {
    modal_ground_on(frame, area, theme, Surface::Raised);
}

/// The same over `surface`: the Settings box takes the pane grey, so the palette it picks
/// reads in it as the screens draw it.
pub fn modal_ground_on(frame: &mut Frame, area: Rect, theme: &Theme, surface: Surface) {
    dim_around(frame.buffer_mut(), &[area], theme);
    lay(frame, area, theme, surface);
}

/// Clear `area` and set it in the theme's primary text over the raised grey, where the theme
/// has one: the ground of a box, a band's edit line or a toast.
pub fn raise(frame: &mut Frame, area: Rect, theme: &Theme) {
    lay(frame, area, theme, Surface::Raised);
}

/// Clear `area` and set it in the theme's primary text over `surface`'s grey, where the theme
/// has one.
fn lay(frame: &mut Frame, area: Rect, theme: &Theme, surface: Surface) {
    frame.render_widget(Clear, area);
    let mut style = theme.style(Role::Text);
    if let Some(grey) = theme.surface(surface) {
        style = style.bg(grey);
    }
    frame.buffer_mut().set_style(area, style);
}

/// Paint the theme's backgrounds under what a screen has just drawn: every cell still on the
/// terminal's own background takes the pane grey inside one of `frames` (border included) and
/// the ground grey anywhere else. A cell with a background of its own keeps it. Nothing
/// is painted where the theme has no greys.
pub fn paint_surfaces(buffer: &mut Buffer, frames: &[Rect], theme: &Theme) {
    let (Some(ground), Some(pane)) = (theme.surface(Surface::Ground), theme.surface(Surface::Pane))
    else {
        return;
    };
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            if cell.bg != Color::Reset {
                continue;
            }
            let inside = frames
                .iter()
                .any(|frame| frame.contains(Position::new(x, y)));
            cell.bg = if inside { pane } else { ground };
        }
    }
}

/// `block` drawn as the box a modal is: its border in the style a focused pane's contour
/// takes and `title` in the selected role a focused pane's title takes, so the one box in
/// focus is drawn in the theme's own colours while everything around it is dimmed.
pub fn modal_block(block: Block<'static>, title: &str, app: &App, theme: &Theme) -> Block<'static> {
    block
        .border_style(crate::tui::screens::device::border_style(app, theme))
        .title(Span::styled(title.to_string(), theme.style(Role::Selected)))
}

/// The keys a modal box names on its last line, after the blank it opens with: each label in
/// the accent the key hints take and what it does in the primary text, the pairs separated by
/// ` · `. A key whose words are empty is named alone.
pub fn modal_keys(keys: &[(&str, &str)], theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (at, (key, does)) in keys.iter().enumerate() {
        if at > 0 {
            spans.push(Span::styled(" · ", theme.style(Role::Text)));
        }
        spans.push(Span::styled(key.to_string(), theme.style(Role::KeyHint)));
        if !does.is_empty() {
            spans.push(Span::styled(format!(" {does}"), theme.style(Role::Text)));
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests;
