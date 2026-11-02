// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The characters the raster draws from geometry rather than from a font: box-drawing lines,
//! block elements and braille. Drawn from rectangles that share their edges, they meet their
//! neighbours in the next cell with no seam, which a font's glyphs do not guarantee.

use Arm::{Heavy as H, Light as L, None as N};

/// The size of one cell, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::tui::screenshots) struct CellSize {
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// How much of each pixel of a cell a character covers, `w × h`, row-major, 0.0 to 1.0.
pub(in crate::tui::screenshots) type Coverage = Vec<f32>;

/// The coverage of `c` in a cell of size `cell`, or `None` where `c` is drawn from a font. A
/// box-drawing character the raster does not draw is an error naming it.
pub(in crate::tui::screenshots) fn drawn(
    c: char,
    cell: CellSize,
) -> Option<Result<Coverage, String>> {
    match c as u32 {
        0x2500..=0x257f => Some(boxed(c, cell)),
        0x2580..=0x259f => Some(Ok(block(c, cell))),
        0x2800..=0x28ff => Some(Ok(braille(c as u32 - 0x2800, cell))),
        _ => None,
    }
}

/// An empty coverage for `cell`.
fn blank(cell: CellSize) -> Coverage {
    vec![0.0; (cell.w * cell.h) as usize]
}

/// Sets the pixels of columns `x0..x1` and rows `y0..y1` to `value`.
fn fill(cov: &mut [f32], cell: CellSize, (x0, x1): (u32, u32), (y0, y1): (u32, u32), value: f32) {
    for y in y0..y1.min(cell.h) {
        for x in x0..x1.min(cell.w) {
            cov[(y * cell.w + x) as usize] = value;
        }
    }
}

/// How heavy one arm of a box-drawing character is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    None,
    Light,
    Heavy,
}

/// The arms (up, right, down, left) of a box-drawing character, if the raster draws it.
fn arms(c: char) -> Option<[Arm; 4]> {
    Some(match c {
        '─' => [N, L, N, L],
        '━' => [N, H, N, H],
        '│' => [L, N, L, N],
        '┃' => [H, N, H, N],
        '┌' | '╭' => [N, L, L, N],
        '┐' | '╮' => [N, N, L, L],
        '└' | '╰' => [L, L, N, N],
        '┘' | '╯' => [L, N, N, L],
        '┏' => [N, H, H, N],
        '┓' => [N, N, H, H],
        '┗' => [H, H, N, N],
        '┛' => [H, N, N, H],
        '├' => [L, L, L, N],
        '┤' => [L, N, L, L],
        '┬' => [N, L, L, L],
        '┴' => [L, L, N, L],
        '┼' => [L, L, L, L],
        '┣' => [H, H, H, N],
        '┫' => [H, N, H, H],
        '┳' => [N, H, H, H],
        '┻' => [H, H, N, H],
        '╋' => [H, H, H, H],
        _ => return None,
    })
}

/// A box-drawing character as arms from the centre of the cell to its edges. The centre band is
/// always covered and each arm runs from its far edge to the cell's edge, so arms of
/// neighbouring cells meet exactly.
fn boxed(c: char, cell: CellSize) -> Result<Coverage, String> {
    let Some([up, right, down, left]) = arms(c) else {
        return Err(format!(
            "U+{:04X} {c}: a box-drawing character the raster does not draw",
            c as u32
        ));
    };
    let light = (cell.w as f32 / 8.0).round().max(1.0) as u32;
    let thickness = |arm: Arm| if arm == Arm::Heavy { light * 2 } else { light };
    let mut cov = blank(cell);
    // The centre band is as thick as the heaviest arm.
    let heaviest = [up, right, down, left]
        .into_iter()
        .map(thickness)
        .max()
        .unwrap_or(light);
    let band = |len: u32, t: u32| {
        let a = len.saturating_sub(t) / 2;
        (a, a + t)
    };
    let (cx0, cx1) = band(cell.w, heaviest);
    let (cy0, cy1) = band(cell.h, heaviest);
    fill(&mut cov, cell, (cx0, cx1), (cy0, cy1), 1.0);
    for (arm, vertical, far) in [
        (up, true, false),
        (down, true, true),
        (left, false, false),
        (right, false, true),
    ] {
        if arm == Arm::None {
            continue;
        }
        let t = thickness(arm);
        if vertical {
            let (x0, x1) = band(cell.w, t);
            let (y0, y1) = if far { (cy0, cell.h) } else { (0, cy1) };
            fill(&mut cov, cell, (x0, x1), (y0, y1), 1.0);
        } else {
            let (y0, y1) = band(cell.h, t);
            let (x0, x1) = if far { (cx0, cell.w) } else { (0, cx1) };
            fill(&mut cov, cell, (x0, x1), (y0, y1), 1.0);
        }
    }
    Ok(cov)
}

/// A block element: rectangles in eighths and quadrants, or a uniform shade.
fn block(c: char, cell: CellSize) -> Coverage {
    let (w, h) = (cell.w, cell.h);
    let (hw, hh) = (w / 2, h / 2);
    let mut cov = blank(cell);
    let code = c as u32;
    let rows_eighths = |n: u32| (h * n + 4) / 8;
    let cols_eighths = |n: u32| (w * n + 4) / 8;
    // Quadrants: upper left, upper right, lower left, lower right.
    let quadrants = |cov: &mut Coverage, set: [bool; 4]| {
        let spans = [
            ((0, hw), (0, hh)),
            ((hw, w), (0, hh)),
            ((0, hw), (hh, h)),
            ((hw, w), (hh, h)),
        ];
        for (on, (xs, ys)) in set.into_iter().zip(spans) {
            if on {
                fill(cov, cell, xs, ys, 1.0);
            }
        }
    };
    match code {
        0x2580 => fill(&mut cov, cell, (0, w), (0, hh), 1.0),
        0x2581..=0x2588 => {
            let rows = rows_eighths(code - 0x2580);
            fill(&mut cov, cell, (0, w), (h - rows, h), 1.0);
        }
        0x2589..=0x258f => {
            let cols = cols_eighths(0x2590 - code);
            fill(&mut cov, cell, (0, cols), (0, h), 1.0);
        }
        0x2590 => fill(&mut cov, cell, (hw, w), (0, h), 1.0),
        0x2591..=0x2593 => cov.fill((code - 0x2590) as f32 * 0.25),
        0x2594 => fill(&mut cov, cell, (0, w), (0, rows_eighths(1)), 1.0),
        0x2595 => fill(&mut cov, cell, (w - cols_eighths(1), w), (0, h), 1.0),
        0x2596 => quadrants(&mut cov, [false, false, true, false]),
        0x2597 => quadrants(&mut cov, [false, false, false, true]),
        0x2598 => quadrants(&mut cov, [true, false, false, false]),
        0x2599 => quadrants(&mut cov, [true, false, true, true]),
        0x259a => quadrants(&mut cov, [true, false, false, true]),
        0x259b => quadrants(&mut cov, [true, true, true, false]),
        0x259c => quadrants(&mut cov, [true, true, false, true]),
        0x259d => quadrants(&mut cov, [false, true, false, false]),
        0x259e => quadrants(&mut cov, [false, true, true, false]),
        _ => quadrants(&mut cov, [false, true, true, true]),
    }
    cov
}

/// A braille character: `bits` are its dots, as the Unicode block numbers them, each a disc
/// antialiased by 4×4 supersampling.
fn braille(bits: u32, cell: CellSize) -> Coverage {
    // Bit n as (column, row) of the 2×4 grid.
    const DOTS: [(u32, u32); 8] = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let (w, h) = (cell.w as f32, cell.h as f32);
    let radius = 0.7 * (w / 2.0).min(h / 4.0) / 2.0;
    let mut cov = blank(cell);
    for (bit, (col, row)) in DOTS.into_iter().enumerate() {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let cx = ((2 * col + 1) * cell.w / 4) as f32;
        let cy = ((2 * row + 1) * cell.h / 8) as f32;
        let (x0, x1) = (
            (cx - radius - 1.0).max(0.0) as u32,
            (cx + radius + 2.0) as u32,
        );
        let (y0, y1) = (
            (cy - radius - 1.0).max(0.0) as u32,
            (cy + radius + 2.0) as u32,
        );
        for y in y0..y1.min(cell.h) {
            for x in x0..x1.min(cell.w) {
                let dist = |px: f32, py: f32| ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let value = if dist(x as f32 + 0.5, y as f32 + 0.5) <= radius {
                    1.0
                } else {
                    let mut hit = 0;
                    for sy in 0..4 {
                        for sx in 0..4 {
                            let (px, py) = (
                                x as f32 + (sx as f32 + 0.5) / 4.0,
                                y as f32 + (sy as f32 + 0.5) / 4.0,
                            );
                            hit += u32::from(dist(px, py) <= radius);
                        }
                    }
                    hit as f32 / 16.0
                };
                let at = &mut cov[(y * cell.w + x) as usize];
                *at = at.max(value);
            }
        }
    }
    cov
}
