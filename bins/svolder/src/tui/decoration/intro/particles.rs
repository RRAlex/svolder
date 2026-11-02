// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Particles: the large mark built from sparks.
//!
//! The sparks wait along the frame's edges, are thrown in on gentle curves and land on the
//! letters' faces along a front that rises from the bottom left at 30°. Each landed cell
//! flashes and cools, a letter's shadows slide out from under it once its face is built, a
//! glint runs along the front, and the notice fades in. Each frame is a function of the time,
//! the frame size and the seed alone.

use std::collections::HashSet;
use std::f32::consts::TAU;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::{Paragraph, Widget};

use crate::tui::theme::{Rgb, Theme};
use crate::tui::widgets;

use super::rng::Rng;
use super::sparks::{
    Colours, Dots, FACE, FLASH, FLASH_HEAT, MarkCell, Path, centre, colour, mark_cells, mix, norm,
    quad_out, span,
};

/// How long the build lasts, from its first frame to the finished mark held, in seconds.
pub(in crate::tui) const LENGTH: f32 = 3.6;
/// From here every cell has settled and the frame is the still one.
const SETTLED: f32 = 3.2;
/// How long the waiting sparks take to fade in.
const APPEAR: f32 = 0.3;
/// No spark is thrown before this.
const MIN_THROW: f32 = 0.25;
/// When the front reaches its first face cell.
const LAND_FIRST: f32 = 1.0;
/// How long the front takes from its first face cell to its last.
const LAND_SPREAD: f32 = 1.2;
/// How far a landing strays from the front, either way.
const LAND_JITTER: f32 = 0.04;
/// How long a flight lasts.
const FLIGHT: f32 = 0.9;
/// How far a flight's length strays, either way.
const FLIGHT_JITTER: f32 = 0.12;
/// A waiting spark's middle brightness.
const WAIT_GLOW: f32 = 0.22;
/// How far a waiting spark's brightness swings.
const WAIT_SWING: f32 = 0.12;
/// How fast a waiting spark swings, in cycles a second.
const WAIT_HZ: f32 = 1.3;
/// A waiting spark's heat.
const WAIT_HEAT: f32 = 0.3;
/// How much heat a spark loses over its flight.
const COOL: f32 = 0.65;
/// How long after its letter's face is built a shadow starts, for `▒` and for `░`.
const SHADOW_DELAY: [f32; 2] = [0.05, 0.15];
/// How long a shadow takes to slide out and fade in.
const SHADOW_SLIDE: f32 = 0.3;
/// When the glint starts.
const GLINT_START: f32 = 2.6;
/// How long the glint takes to cross.
const GLINT_LENGTH: f32 = 0.35;
/// How wide the glint's band is, in front units.
const GLINT_BAND: f32 = 4.0;
/// How far beyond the mark's lowest and highest front the glint starts and ends.
const GLINT_REACH: f32 = 6.0;
/// How far towards the hot colour the glint lifts a cell at its centre.
const GLINT_HEAT: f32 = 0.75;
/// When the notice starts fading in.
const NOTICE_START: f32 = 2.6;
/// How long the notice takes to fade in.
const NOTICE_FADE: f32 = 0.6;

/// The build planned for one frame size and seed.
#[derive(Debug)]
pub(in crate::tui) struct Build {
    /// The frame it is planned for.
    area: Rect,
    /// Where the large mark stands in it.
    mark: Rect,
    /// One spark per face cell.
    face: Vec<Spark>,
    /// The shadow cells, which slide rather than fly.
    shadows: Vec<Shadow>,
    /// The lowest and highest front of all the mark's cells, which the glint crosses.
    low: f32,
    high: f32,
}

/// One face cell's spark.
#[derive(Debug)]
struct Spark {
    cell: MarkCell,
    /// Where it waits on the edge.
    start: (f32, f32),
    path: Path,
    /// When it is thrown.
    throw: f32,
    /// When it lands.
    land: f32,
    /// Where in its swing it waits, `0.0..1.0`.
    phase: f32,
}

/// One shadow cell.
#[derive(Debug)]
struct Shadow {
    cell: MarkCell,
    /// 1 for `▒`, 2 for `░`: how many cells up-left of its place it starts from.
    depth: u16,
    /// When it starts sliding out.
    start: f32,
}

impl Build {
    /// The build for a frame `area` from `seed`, or `None` for a frame too small for the
    /// large mark.
    pub(in crate::tui) fn new(area: Rect, seed: u64) -> Option<Build> {
        let mark = super::large_mark(area)?;
        let cells = mark_cells();
        let (low, high) = span(cells.iter().map(|cell| cell.front));
        let (face_low, face_high) = span(
            cells
                .iter()
                .filter(|cell| cell.glyph == FACE)
                .map(|cell| cell.front),
        );
        let around = 2.0 * (f32::from(area.width) + f32::from(area.height));
        let mut rng = Rng::new(seed);
        let mut face = Vec::new();
        for cell in cells.iter().filter(|cell| cell.glyph == FACE) {
            let land = LAND_FIRST
                + norm(cell.front, face_low, face_high) * LAND_SPREAD
                + rng.signed() * LAND_JITTER;
            let throw = (land - (FLIGHT + rng.signed() * FLIGHT_JITTER)).max(MIN_THROW);
            let start = edge(rng.unit() * around, area);
            let path = Path::curve(start, centre(mark, cell), &mut rng);
            face.push(Spark {
                cell: *cell,
                start,
                path,
                throw,
                land,
                phase: rng.unit(),
            });
        }
        let letters = cells.iter().map(|cell| cell.letter + 1).max().unwrap_or(0);
        let mut built = vec![0.0f32; letters];
        for spark in &face {
            built[spark.cell.letter] = built[spark.cell.letter].max(spark.land);
        }
        let shadows = cells
            .iter()
            .filter(|cell| cell.glyph != FACE)
            .map(|cell| {
                let depth: u16 = if cell.glyph == '▒' { 1 } else { 2 };
                Shadow {
                    cell: *cell,
                    depth,
                    start: built[cell.letter] + SHADOW_DELAY[usize::from(depth - 1)],
                }
            })
            .collect();
        Some(Build {
            area,
            mark,
            face,
            shadows,
            low,
            high,
        })
    }

    /// The frame this build is planned for.
    pub(in crate::tui) fn area(&self) -> Rect {
        self.area
    }

    /// Paint the build `t` seconds in over `buf`, a blank frame of [`Build::area`].
    pub(in crate::tui) fn frame(&self, t: f32, buf: &mut Buffer, theme: &Theme, colours: &Colours) {
        if t >= SETTLED {
            super::draw_still(buf, self.area, theme);
            return;
        }
        widgets::surfaces::paint_surfaces(buf, &[], theme);
        let mut dots = Dots::new(self.area);
        for spark in self.face.iter().filter(|spark| t < spark.land) {
            if t < spark.throw {
                let swing = (TAU * (WAIT_HZ * t + spark.phase)).sin();
                let glow = (WAIT_GLOW + WAIT_SWING * swing) * (t / APPEAR).min(1.0);
                dots.put(spark.start, glow, WAIT_HEAT);
            } else {
                dots.fly(&spark.path, t - spark.throw, spark.land - spark.throw, COOL);
            }
        }
        dots.paint(buf, colours);
        let mut drawn: Vec<((u16, u16), char, Rgb, f32)> = Vec::new();
        let mut solid = HashSet::new();
        for spark in self.face.iter().filter(|spark| t >= spark.land) {
            let at = self.at(&spark.cell);
            let heat = FLASH_HEAT * (1.0 - (t - spark.land) / FLASH).max(0.0);
            let rgb = mix(colours.rows[usize::from(spark.cell.row)], colours.hot, heat);
            drawn.push((at, FACE, rgb, spark.cell.front));
            solid.insert(at);
        }
        for shadow in self.shadows.iter().filter(|shadow| t >= shadow.start) {
            let e = quad_out((t - shadow.start) / SHADOW_SLIDE);
            let off = (f32::from(shadow.depth) * (1.0 - e)).round() as u16;
            let (x, y) = self.at(&shadow.cell);
            let at = (x.saturating_sub(off), y.saturating_sub(off));
            if solid.contains(&at) {
                continue;
            }
            let rgb = mix(
                colours.ground,
                colours.rows[usize::from(shadow.cell.row)],
                e,
            );
            drawn.push((at, shadow.cell.glyph, rgb, shadow.cell.front));
        }
        let g = (t - GLINT_START) / GLINT_LENGTH;
        if g > 0.0 && g < 1.0 {
            let band = self.low - GLINT_REACH + (self.high - self.low + 2.0 * GLINT_REACH) * g;
            for (_, _, rgb, front) in &mut drawn {
                let near = (1.0 - (*front - band).abs() / GLINT_BAND).max(0.0);
                *rgb = mix(*rgb, colours.hot, GLINT_HEAT * near);
            }
        }
        for (at, glyph, rgb, _) in drawn {
            if let Some(cell) = buf.cell_mut(at) {
                cell.set_char(glyph);
                cell.set_fg(colour(rgb));
                cell.modifier = Modifier::empty();
            }
        }
        let a = ((t - NOTICE_START) / NOTICE_FADE).clamp(0.0, 1.0);
        if a > 0.0
            && let Some((lines, area)) = super::notice(self.area, self.mark, theme)
        {
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .render(area, buf);
            let fg = colour(mix(colours.ground, colours.dim, a));
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y))
                        && cell.symbol() != " "
                    {
                        cell.set_fg(fg);
                    }
                }
            }
        }
    }

    /// Where `cell` of the mark stands on the frame.
    fn at(&self, cell: &MarkCell) -> (u16, u16) {
        (self.mark.x + cell.col, self.mark.y + cell.row)
    }
}

/// The point `along` cells round the edge of `area`, clockwise from its top-left corner, a
/// tenth of a cell inside it.
fn edge(along: f32, area: Rect) -> (f32, f32) {
    const IN: f32 = 0.1;
    let (width, height) = (f32::from(area.width), f32::from(area.height));
    let (x0, y0) = (f32::from(area.x), f32::from(area.y));
    let mut u = along;
    if u < width {
        return (x0 + u, y0 + IN);
    }
    u -= width;
    if u < height {
        return (x0 + width - IN, y0 + u);
    }
    u -= height;
    // Along the bottom and up the left the count runs back towards the origin, so its first
    // point is held a tenth inside the far edge, as every other point is.
    if u < width {
        return (x0 + (width - u).min(width - IN), y0 + height - IN);
    }
    u -= width;
    (x0 + IN, y0 + (height - u).clamp(0.0, height - IN))
}

#[cfg(test)]
mod tests;
