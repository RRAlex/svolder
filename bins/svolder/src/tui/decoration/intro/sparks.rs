// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the intro's sparks are made of, for the build and the exit alike:
//! - the gentle curve a spark flies;
//! - the braille dots it lights with its trail;
//! - the colours it glows in;
//! - the large mark split into the cells and letters the sparks land on.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use crate::tui::decoration::wordmark::{self, Size};
use crate::tui::palette::rgb_of;
use crate::tui::theme::{DIM_GREY, Rgb, Role, Surface, Theme};

use super::rng::Rng;

/// The glyph of the large mark's face; its shadows are `▒` and `░`.
pub(in crate::tui) const FACE: char = '▓';
/// How long a landed cell takes to cool from its flash to its own colour, in seconds.
pub(in crate::tui) const FLASH: f32 = 0.15;
/// How far towards the hot colour a cell flashes as a spark lands on it.
pub(in crate::tui) const FLASH_HEAT: f32 = 0.9;
/// How far the front leans from level, in degrees. At 0° it rises straight up the rows; at 90°
/// it sweeps across the columns.
const TILT_DEGREES: f32 = 30.0;
/// The white a spark's heat runs to.
const WHITE: Rgb = (0xff, 0xf8, 0xe8);
/// How far towards white the hot colour is from the accent.
const HOT: f32 = 0.8;
/// How far a curve's control point is pushed sideways off the midpoint, at most, as a share of
/// the path's length.
const BEND: f32 = 0.35;
/// How far apart in time the trail's dots are, in seconds.
const TRAIL_STEP: f32 = 0.022;
/// The brightness of the trail's dots behind the head, nearest first.
const TRAIL: [f32; 3] = [0.55, 0.32, 0.16];
/// Which braille dot lights for each quarter of a cell's height and half of its width.
const BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
/// The braille character with no dots, which every other one is counted from.
const BRAILLE: u32 = 0x2800;
/// The large mark's rows.
const MARK_ROWS: usize = 8;

/// The colours the sparks, and the marks they build, are drawn in, taken once from the theme the
/// intro runs under.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::tui) struct Colours {
    /// What a dim spark fades into: the theme's ground, else the dim grey today's fade starts from.
    pub(in crate::tui) ground: Rgb,
    /// The accent a cooled spark glows in.
    pub(in crate::tui) accent: Rgb,
    /// The accent most of the way to white: a hot spark, a landing flash, the glint.
    pub(in crate::tui) hot: Rgb,
    /// The notice's own dim colour.
    pub(in crate::tui) dim: Rgb,
    /// The large mark's colour on each of its rows, top first: the wordmark's own gradient.
    pub(in crate::tui) rows: [Rgb; MARK_ROWS],
}

impl Colours {
    /// The colours `theme` draws the intro in.
    pub(in crate::tui) fn of(theme: &Theme) -> Colours {
        let accent = theme.accent_rgb();
        let ground = theme
            .surface(Surface::Ground)
            .map_or(rgb_of(DIM_GREY), rgb_of);
        let dim = theme.style(Role::Dim).fg.map_or(ground, rgb_of);
        let mut rows = [accent; MARK_ROWS];
        for (row, line) in rows.iter_mut().zip(wordmark::lines(Size::Large, theme)) {
            if let Some(fg) = line.spans.first().and_then(|span| span.style.fg) {
                *row = rgb_of(fg);
            }
        }
        Colours {
            ground,
            accent,
            hot: mix(accent, WHITE, HOT),
            dim,
            rows,
        }
    }
}

/// `a` moved `t` of the way to `b`, `t` held to `0.0..=1.0`.
pub(in crate::tui) fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let one = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    (one(a.0, b.0), one(a.1, b.1), one(a.2, b.2))
}

/// `rgb` as a cell colour.
pub(in crate::tui) fn colour((r, g, b): Rgb) -> Color {
    Color::Rgb(r, g, b)
}

/// An ease that starts fast and lands soft, over `0.0..=1.0`.
pub(in crate::tui) fn quad_out(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    1.0 - (1.0 - u) * (1.0 - u)
}

/// The lowest and highest of `values`.
pub(in crate::tui) fn span(values: impl Iterator<Item = f32>) -> (f32, f32) {
    values.fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), v| {
        (low.min(v), high.max(v))
    })
}

/// Where `value` lies between `low` and `high`, as `0.0..=1.0`; 0 when they are one.
pub(in crate::tui) fn norm(value: f32, low: f32, high: f32) -> f32 {
    match high > low {
        true => (value - low) / (high - low),
        false => 0.0,
    }
}

/// The gentle curve a spark flies: a quadratic Bézier from where it starts to where it lands,
/// travelled on an ease that throws it fast and lands it soft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::tui) struct Path {
    /// Where it starts, in cells.
    from: (f32, f32),
    /// The control point the curve bends towards.
    bend: (f32, f32),
    /// Where it lands.
    to: (f32, f32),
}

impl Path {
    /// A curve from `from` to `to`, in cells, its control point pushed sideways off the midpoint
    /// by up to [`BEND`] of the path's length, on a side and by an amount `rng` picks. A row
    /// counts as two columns, so a curve bends as much across the rows as along them.
    pub(in crate::tui) fn curve(from: (f32, f32), to: (f32, f32), rng: &mut Rng) -> Path {
        let dx = to.0 - from.0;
        let dy = (to.1 - from.1) * 2.0;
        let length = dx.hypot(dy).max(f32::EPSILON);
        let bend = rng.signed() * BEND * length;
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        Path {
            from,
            bend: (mid.0 - dy / length * bend, mid.1 + dx / length * bend / 2.0),
            to,
        }
    }

    /// Where the spark is `u` of the way through its flight; past 1 it stays landed.
    pub(in crate::tui) fn at(&self, u: f32) -> (f32, f32) {
        let e = quad_out(u);
        let (a, b, d) = ((1.0 - e) * (1.0 - e), 2.0 * (1.0 - e) * e, e * e);
        (
            a * self.from.0 + b * self.bend.0 + d * self.to.0,
            a * self.from.1 + b * self.bend.1 + d * self.to.1,
        )
    }
}

/// The braille dots one frame's sparks light, gathered before any is painted. A cell shows every
/// dot lit in it, in the colour of its brightest dot.
#[derive(Debug)]
pub(in crate::tui) struct Dots {
    /// The cells a dot may light.
    area: Rect,
    /// The lit cells by position.
    lit: HashMap<(u16, u16), Lit>,
}

/// One cell's dots.
#[derive(Debug, Clone, Copy)]
struct Lit {
    /// Which of its eight dots are lit.
    bits: u8,
    /// Its brightest dot's brightness.
    brightness: f32,
    /// That dot's heat.
    heat: f32,
}

impl Dots {
    /// No dots lit yet, over `area`.
    pub(in crate::tui) fn new(area: Rect) -> Dots {
        Dots {
            area,
            lit: HashMap::new(),
        }
    }

    /// Light the dot under `(x, y)`, in cells from the buffer's origin, at `brightness` and
    /// `heat`, both `0.0..=1.0`. A point outside the area, or at no brightness, lights nothing.
    pub(in crate::tui) fn put(&mut self, (x, y): (f32, f32), brightness: f32, heat: f32) {
        let inside = x >= f32::from(self.area.left())
            && y >= f32::from(self.area.top())
            && x < f32::from(self.area.right())
            && y < f32::from(self.area.bottom());
        if brightness <= 0.0 || !inside {
            return;
        }
        let (cx, cy) = (x.floor(), y.floor());
        let bit = BITS[(((y - cy) * 4.0) as usize).min(3)][(((x - cx) * 2.0) as usize).min(1)];
        let lit = self.lit.entry((cx as u16, cy as u16)).or_insert(Lit {
            bits: 0,
            brightness: 0.0,
            heat: 0.0,
        });
        lit.bits |= bit;
        if brightness > lit.brightness {
            lit.brightness = brightness;
            lit.heat = heat;
        }
    }

    /// Light a spark `tau` seconds into a flight of `flight` along `path`: its head at full
    /// brightness and its trail behind it. All of it is at the heat the spark has cooled to,
    /// having lost `cool` of it by its landing.
    pub(in crate::tui) fn fly(&mut self, path: &Path, tau: f32, flight: f32, cool: f32) {
        debug_assert!(flight > 0.0, "a flight takes time");
        let heat = 1.0 - cool * tau / flight;
        self.put(path.at(tau / flight), 1.0, heat);
        for (k, brightness) in TRAIL.iter().enumerate() {
            let back = tau - (k + 1) as f32 * TRAIL_STEP;
            if back >= 0.0 {
                self.put(path.at(back / flight), *brightness, heat);
            }
        }
    }

    /// Paint every lit cell into `buf`: its braille character, in the accent heated towards
    /// the hot colour, mixed into the cell's own background by its brightness. A background that is
    /// not an RGB colour mixes from `colours.ground`.
    pub(in crate::tui) fn paint(&self, buf: &mut Buffer, colours: &Colours) {
        for (&at, lit) in &self.lit {
            let Some(cell) = buf.cell_mut(at) else {
                continue;
            };
            let under = match cell.bg {
                Color::Rgb(r, g, b) => (r, g, b),
                _ => colours.ground,
            };
            let glow = mix(colours.accent, colours.hot, lit.heat);
            cell.set_char(char::from_u32(BRAILLE + u32::from(lit.bits)).unwrap_or(' '));
            cell.set_fg(colour(mix(under, glow, lit.brightness)));
            cell.modifier = Modifier::empty();
        }
    }
}

/// One non-blank cell of the large mark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::tui) struct MarkCell {
    /// Its column within the mark.
    pub(in crate::tui) col: u16,
    /// Its row within the mark, 0 the top.
    pub(in crate::tui) row: u16,
    /// `▓`, `▒` or `░`.
    pub(in crate::tui) glyph: char,
    /// Which of the seven letters it belongs to, 0 the leftmost.
    pub(in crate::tui) letter: usize,
    /// How far along the tilted front it lies: the front reaches low values first.
    pub(in crate::tui) front: f32,
}

/// The centre of `cell`, in cells, with the mark standing at `mark`.
pub(in crate::tui) fn centre(mark: Rect, cell: &MarkCell) -> (f32, f32) {
    (
        f32::from(mark.x + cell.col) + 0.5,
        f32::from(mark.y + cell.row) + 0.5,
    )
}

/// Every non-blank cell of the large mark with its letter and its place on the front.
///
/// The letters are the mark's 4-connected groups of non-blank cells, numbered by their leftmost
/// column. The front is `col·sin 30° + (bottom − row)·2·cos 30°`, a row counting as two columns.
pub(in crate::tui) fn mark_cells() -> Vec<MarkCell> {
    let rows: Vec<Vec<char>> = wordmark::art(Size::Large)
        .lines()
        .map(|row| row.chars().collect())
        .collect();
    let bottom = rows.len().saturating_sub(1);
    let (sin, cos) = TILT_DEGREES.to_radians().sin_cos();
    let mut cells: Vec<MarkCell> = Vec::new();
    for (row, chars) in rows.iter().enumerate() {
        for (col, &glyph) in chars.iter().enumerate() {
            if glyph != ' ' {
                cells.push(MarkCell {
                    col: col as u16,
                    row: row as u16,
                    glyph,
                    letter: usize::MAX,
                    front: col as f32 * sin + (bottom - row) as f32 * 2.0 * cos,
                });
            }
        }
    }
    let at: HashMap<(u16, u16), usize> = cells
        .iter()
        .enumerate()
        .map(|(i, cell)| ((cell.col, cell.row), i))
        .collect();
    let mut lefts: Vec<u16> = Vec::new();
    for start in 0..cells.len() {
        if cells[start].letter != usize::MAX {
            continue;
        }
        let group = lefts.len();
        let mut left = cells[start].col;
        cells[start].letter = group;
        let mut stack = vec![start];
        while let Some(i) = stack.pop() {
            let (col, row) = (cells[i].col, cells[i].row);
            left = left.min(col);
            let next = [
                col.checked_add(1).map(|c| (c, row)),
                col.checked_sub(1).map(|c| (c, row)),
                row.checked_add(1).map(|r| (col, r)),
                row.checked_sub(1).map(|r| (col, r)),
            ];
            for near in next.into_iter().flatten() {
                if let Some(&j) = at.get(&near)
                    && cells[j].letter == usize::MAX
                {
                    cells[j].letter = group;
                    stack.push(j);
                }
            }
        }
        lefts.push(left);
    }
    let mut order: Vec<usize> = (0..lefts.len()).collect();
    order.sort_by_key(|&group| lefts[group]);
    let mut rank = vec![0; lefts.len()];
    for (k, &group) in order.iter().enumerate() {
        rank[group] = k;
    }
    for cell in &mut cells {
        cell.letter = rank[cell.letter];
    }
    cells
}

#[cfg(test)]
mod tests;
