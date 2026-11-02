// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's icons: the large wordmark's first glyph, its "s", in `--r-amber` with its shadow
//! in `--r-copper` (the wordmark's lighter second shade is left out, so the icon is two colours
//! on the page's ground), centred, inside the 80% a maskable icon keeps. `icon.svg` is pinned to
//! the cells by a test; the two PNGs are drawn by [`draw`] (`make web-icons`), whole cells scaled
//! by a whole number, as PPM for ffmpeg.

use std::fmt::Write as _;

/// The wordmark's text art.
const ART: &str = include_str!("../../../svolder/assets/wordmark/large.txt");
/// The columns of the art the first glyph spans, including the shade the icon leaves out.
const COLUMNS: std::ops::RangeInclusive<usize> = 2..=11;
/// The ground and the two layers' colours: the face and its shadow.
const GROUND: [u8; 3] = [0x0f, 0x0d, 0x0b];
const LAYERS: [[u8; 3]; 2] = [[0xf0, 0xa7, 0x3a], [0xdd, 0x76, 0x2f]];

/// The glyph's cells as (column, row, layer) from its top left, a cell one unit wide and two
/// tall as in the wordmark, without the lighter shade; and its width and height in cells.
fn glyph() -> (Vec<(usize, usize, usize)>, usize, usize) {
    let rows: Vec<Vec<char>> = ART.lines().map(|line| line.chars().collect()).collect();
    let mut cells = Vec::new();
    for (row, line) in rows.iter().enumerate() {
        for column in COLUMNS {
            let layer = match line.get(column) {
                Some('▓') => 0,
                Some('▒') => 1,
                _ => continue,
            };
            cells.push((column - COLUMNS.start(), row, layer));
        }
    }
    let top = cells.iter().map(|c| c.1).min().unwrap_or(0);
    let bottom = cells.iter().map(|c| c.1).max().unwrap_or(0);
    let left = cells.iter().map(|c| c.0).min().unwrap_or(0);
    let right = cells.iter().map(|c| c.0).max().unwrap_or(0);
    for cell in &mut cells {
        cell.0 -= left;
        cell.1 -= top;
    }
    (cells, right - left + 1, bottom - top + 1)
}

/// The SVG icon: its two SPDX lines (`make spdx-check` reads every SVG beside the page), then a
/// 16-unit square of ground with the glyph centred, a path per layer.
fn svg() -> String {
    let (cells, width, height) = glyph();
    // Halves of a unit: an odd width sits half a unit off a whole one, as the PNGs' centring does.
    let (x0, y0) = ((16 - width) as f64 / 2.0, ((16 - 2 * height) / 2) as f64);
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let mut out = format!(
        "<!-- SPDX-FileCopyrightText: 2026 the svolder authors -->\n\
         <!-- SPDX-License-Identifier: GPL-3.0-or-later -->\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\">\
         <rect width=\"16\" height=\"16\" fill=\"{}\"/>",
        hex(GROUND)
    );
    for (layer, colour) in LAYERS.iter().enumerate() {
        let mut d = String::new();
        for &(x, y, _) in cells.iter().filter(|c| c.2 == layer) {
            let _ = write!(d, "M{} {}h1v2h-1z", x0 + x as f64, y0 + (2 * y) as f64);
        }
        let _ = write!(out, "<path fill=\"{}\" d=\"{d}\"/>", hex(*colour));
    }
    out.push_str("</svg>\n");
    out
}

/// A `size`-pixel square PPM of the icon: the glyph at the largest whole scale that keeps it
/// inside the middle 80%, centred.
fn ppm(size: usize) -> Vec<u8> {
    let (cells, width, height) = glyph();
    let unit = (size * 4 / 5) / (2 * height).max(width);
    let (x0, y0) = ((size - width * unit) / 2, (size - 2 * height * unit) / 2);
    let mut pixels = vec![GROUND; size * size];
    for &(x, y, layer) in &cells {
        for py in y0 + 2 * y * unit..y0 + 2 * (y + 1) * unit {
            for px in x0 + x * unit..x0 + (x + 1) * unit {
                pixels[py * size + px] = LAYERS[layer];
            }
        }
    }
    let mut out = format!("P6\n{size} {size}\n255\n").into_bytes();
    out.extend(pixels.into_iter().flatten());
    out
}

/// The committed `icon.svg` is the glyph's cells, as the wordmark's test pins the wordmark.
#[test]
fn the_icon_is_the_wordmarks_first_glyph() {
    let (cells, width, height) = glyph();
    assert_eq!(height, 6);
    assert!(width <= COLUMNS.count());
    assert!(cells.iter().any(|c| c.2 == 0) && cells.iter().any(|c| c.2 == 1));
    assert!(cells.iter().all(|c| c.2 < 2), "only the two orange colours");
    assert_eq!(include_str!("../../web/icon.svg"), svg());
}

/// Write `icon.svg` beside the page and the two PNGs' PPMs under `target/web-icons/`, for
/// `make web-icons` to encode.
#[test]
#[ignore = "draws the icons; run through make web-icons"]
fn draw() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::write(root.join("web/icon.svg"), svg()).unwrap();
    let out = root.join("../../target/web-icons");
    std::fs::create_dir_all(&out).unwrap();
    for size in [192, 512] {
        std::fs::write(out.join(format!("icon-{size}.ppm")), ppm(size)).unwrap();
    }
}
