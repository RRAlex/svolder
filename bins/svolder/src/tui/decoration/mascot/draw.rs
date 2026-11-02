// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawing the squirrel in half blocks: where its box sits at home on the Tone pane or floating
//! over the body, whether the pane has room for it, and the cell-by-cell painter that leaves
//! see-through pixels to what is underneath.

use ratatui::Frame;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier};

use super::looks::Costume;
use super::{BOX_H, BOX_W, Place, Showing};
use crate::tui::app::{App, Screen};
use crate::tui::boxes::toast;
use crate::tui::layout_tier;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::screens;
use crate::tui::theme::Theme;

/// Cell rows the box takes.
const ROWS: u16 = (BOX_H / 2) as u16;
/// Columns between the box's right column and the frame's right edge: the border and one blank.
const INSET: i32 = 3;

/// The box's top-left cell for a right column and a bottom row; may lie off the screen.
fn anchored(right: i32, bottom: i32) -> (i32, i32) {
    (right - (BOX_W as i32 - 1), bottom - (i32::from(ROWS) - 1))
}

/// A cell position as an unsigned coordinate, floored at the screen's edge.
fn coordinate(value: i32) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

/// The box on `pane`: its right column one blank inside the right border, its bottom row on the
/// bottom border. At home that is the Tone pane; a visitor floats over the body the same way.
pub(crate) fn home_box(pane: Rect) -> Rect {
    boxed(anchored(
        i32::from(pane.right()) - INSET,
        i32::from(pane.bottom()) - 1,
    ))
}

fn boxed((x, y): (i32, i32)) -> Rect {
    Rect {
        x: coordinate(x),
        y: coordinate(y),
        width: BOX_W as u16,
        height: ROWS,
    }
}

/// Whether a pane's box has room at home: no line of the pane's content, the first on row `top`
/// and each one's text ending just before the column `ends` gives it, reaches into the box or
/// the blank column left of it. A short line leaves the box its rows; a line above it never
/// takes them.
pub(crate) fn fits_home(pane: Rect, top: u16, ends: &[u16]) -> bool {
    let home = home_box(pane);
    ends.iter()
        .zip(top..)
        .all(|(end, y)| y < home.y || y >= home.bottom() || *end < home.x)
}

/// Whether the squirrel's home box has room on the home pane of a frame of `app` over `area`:
/// the device screen has a home and the box fits on it.
pub(crate) fn home_has_room(area: Rect, app: &App, theme: &Theme) -> bool {
    screens::device::home_of(area, app, theme).is_some_and(|pane| has_room(&pane))
}

/// Whether the box has room on `pane`.
fn has_room(pane: &screens::device::HomePane) -> bool {
    fits_home(pane.rect, pane.top, &pane.ends)
}

/// The squirrel's cells as it painted them under a box that dims the screen, put back over the
/// flat grey each box lays on everything around it, and again over the loop's own dimming.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sprite(Vec<(Position, Cell)>);

impl Sprite {
    /// Only the cells outside every one of `boxes`, which are drawn over the squirrel.
    pub(crate) fn outside(mut self, boxes: &[Rect]) -> Sprite {
        self.0
            .retain(|(at, _)| !boxes.iter().any(|b| b.contains(*at)));
        self
    }

    /// Put every cell back as it was painted.
    pub(crate) fn put_back(&self, buffer: &mut Buffer) {
        for (at, painted) in &self.0 {
            if let Some(cell) = buffer.cell_mut(*at) {
                *cell = painted.clone();
            }
        }
    }
}

/// Draw the squirrel if it is on and showing, and record its cells as [`Hit::Mascot`]. Returns
/// its dimmed greys under a box that dims the screen, and its cells each with what was drawn
/// there before it.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    theme: &Theme,
    hits: &mut HitMap,
) -> (Option<Sprite>, Vec<(Position, Cell)>) {
    let none = (None, Vec::new());
    if !app.mascot_on() {
        return none;
    }
    let Some(mut showing) = app.mascot.showing() else {
        return none;
    };
    if showing.paint.costume == Costume::Amber {
        showing.paint.accent = theme.accent_override();
    }
    let (_, [_, body]) = layout_tier::split(area);
    let home = match (showing.place, app.screen) {
        (Place::Home, Screen::Device) => screens::device::home_of(area, app, theme),
        _ => None,
    };
    let (clip, right, mut bottom) = match &home {
        Some(pane) => {
            if !has_room(pane) {
                return none;
            }
            // The pane's inner width: the squirrel's feet stand on the bottom border, but
            // nothing crosses the side borders.
            let inner = Rect {
                x: pane.rect.x.saturating_add(1),
                width: pane.rect.width.saturating_sub(2),
                ..pane.rect
            };
            (
                inner,
                i32::from(pane.rect.right()) - INSET,
                i32::from(pane.rect.bottom()) - 1,
            )
        }
        None => (
            body,
            i32::from(body.right()) - INSET,
            i32::from(body.bottom()) - 1,
        ),
    };
    let width = match showing.pose.wide {
        true => BOX_W.max(usize::try_from(right + 1 - i32::from(clip.x)).unwrap_or(0)),
        false => BOX_W,
    };
    if let Some(toast) = app.toast() {
        let (toast_area, _) = toast::area(body, &toast.text);
        let extent = Rect {
            x: coordinate(right + 1 - width as i32),
            y: coordinate(anchored(right, bottom).1),
            width: u16::try_from(width).unwrap_or(u16::MAX),
            height: ROWS,
        };
        if !toast_area.intersection(extent).is_empty() {
            // At home the squirrel waits for the toast to go; a visitor stands above it.
            if home.is_some() {
                return none;
            }
            bottom = i32::from(toast_area.y) - 1;
            if bottom < i32::from(body.y) {
                return none;
            }
        }
    }
    // Under a box that dims the screen the squirrel keeps its shading, in greys darkened as the
    // dimmed text is.
    let dimmed = app.modal_open();
    let behind = paint(frame, clip, right, bottom, width, &showing, dimmed, theme);
    let cells: Vec<Position> = behind.iter().map(|(at, _)| *at).collect();
    if showing.place == Place::Home
        && let Some(bounds) = bounds(&cells)
    {
        // A visitor lets clicks through to what is under it.
        hits.push(bounds, Hit::Mascot);
    }
    let buffer = frame.buffer_mut();
    let sprite = dimmed.then(|| {
        Sprite(
            cells
                .iter()
                .filter_map(|at| Some((*at, buffer.cell(*at)?.clone())))
                .collect(),
        )
    });
    (sprite, behind)
}

/// Paint `showing` into a grid `width` pixels wide whose right edge is column `right` and whose
/// last cell row is `bottom`, writing only cells inside `clip` and the frame, in greys
/// darkened as the dimmed text is when `dimmed`. Returns the cells it painted, each with what
/// was drawn there before it.
#[expect(
    clippy::too_many_arguments,
    reason = "the painter takes the grid and its look"
)]
pub(crate) fn paint(
    frame: &mut Frame,
    clip: Rect,
    right: i32,
    bottom: i32,
    width: usize,
    showing: &Showing,
    dimmed: bool,
    theme: &Theme,
) -> Vec<(Position, Cell)> {
    let clip = clip.intersection(frame.area());
    let grid = showing.pose.pixels_wide(width);
    let left = right + 1 - width as i32;
    let top = bottom + 1 - i32::from(ROWS);
    let offset = width as i64 - BOX_W as i64;
    let mut cells = Vec::new();
    for row in 0..usize::from(ROWS) {
        for (col, (upper, lower)) in grid[2 * row].iter().zip(&grid[2 * row + 1]).enumerate() {
            if upper.is_none() && lower.is_none() {
                continue;
            }
            let (x, y) = (left + col as i32, top + row as i32);
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                continue;
            };
            if !clip.contains((x, y).into()) {
                continue;
            }
            // The colour wheel and chases turn on the box's own pixel columns.
            let px = (col as i64 - offset).rem_euclid(BOX_W as i64) as usize;
            let colour = |letter: char, py: usize| {
                let rgb = showing.paint.rgb(letter, px, py, showing.elapsed);
                match dimmed {
                    true => theme.dimmed(rgb),
                    false => theme.quantised(rgb),
                }
            };
            let Some(cell) = frame.buffer_mut().cell_mut((x, y)) else {
                continue;
            };
            let behind = cell.clone();
            let under = cell.bg;
            let (symbol, fg, bg): (&str, Color, Color) = match (upper, lower) {
                (Some(top_px), Some(bottom_px)) => (
                    "▀",
                    colour(*top_px, 2 * row),
                    colour(*bottom_px, 2 * row + 1),
                ),
                (Some(top_px), None) => ("▀", colour(*top_px, 2 * row), under),
                (None, Some(bottom_px)) => ("▄", colour(*bottom_px, 2 * row + 1), under),
                (None, None) => continue,
            };
            cell.set_symbol(symbol);
            cell.fg = fg;
            cell.bg = bg;
            cell.modifier = Modifier::empty();
            cells.push((Position::new(x, y), behind));
        }
    }
    cells
}

/// The smallest rect holding every one of `cells`.
pub(crate) fn bounds(cells: &[Position]) -> Option<Rect> {
    let x0 = cells.iter().map(|at| at.x).min()?;
    let y0 = cells.iter().map(|at| at.y).min()?;
    let x1 = cells.iter().map(|at| at.x).max()?;
    let y1 = cells.iter().map(|at| at.y).max()?;
    Some(Rect {
        x: x0,
        y: y0,
        width: x1 - x0 + 1,
        height: y1 - y0 + 1,
    })
}
