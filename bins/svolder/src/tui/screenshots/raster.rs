// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A drawn buffer as pixels, the way the user's terminal draws it: box, block and braille
//! characters from geometry ([`geometry`]), every other character from Ubuntu Sans Mono or,
//! where it has none, DejaVu Sans ([`fonts`]), each blended over its cell's ground in linear
//! light.

pub(in crate::tui::screenshots) mod fonts;
pub(in crate::tui::screenshots) mod geometry;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};
use ratatui::text::Span;

use self::fonts::Fonts;
use self::geometry::{CellSize, drawn};
pub(in crate::tui::screenshots) use crate::tui::theme::Rgb;
use crate::tui::theme::{Role, Surface, Theme};

/// Which side of a cell a colour is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::screenshots) enum Side {
    Fg,
    Bg,
}

/// What the terminal's own colours stand for, where a cell says `Reset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::screenshots) struct Palette {
    /// The theme's ground, for a background.
    pub ground: Rgb,
    /// The theme's text, for a foreground.
    pub text: Rgb,
}

impl Palette {
    /// The ground and text `theme` draws: the scenes draw with the theme background on, so a
    /// ground is always there.
    pub(in crate::tui::screenshots) fn of(theme: &Theme) -> Palette {
        let rgb = |colour: Option<Color>| {
            crate::tui::palette::rgb_of(colour.expect("a true-colour theme with its background on"))
        };
        Palette {
            ground: rgb(theme.surface(Surface::Ground)),
            text: rgb(theme.style(Role::Text).fg),
        }
    }
}

/// `colour` as RGB: as it is, `Reset` as the palette's ground or text, a named colour through
/// the crate's own xterm table; an indexed colour is an error, since the scenes draw at true
/// colour.
pub(in crate::tui::screenshots) fn resolve(
    colour: Color,
    side: Side,
    palette: &Palette,
) -> Result<Rgb, String> {
    match colour {
        Color::Rgb(r, g, b) => Ok((r, g, b)),
        Color::Reset => Ok(match side {
            Side::Fg => palette.text,
            Side::Bg => palette.ground,
        }),
        Color::Indexed(i) => Err(format!(
            "an indexed colour ({i}): the scenes draw at true colour"
        )),
        named => Ok(crate::tui::palette::rgb_of(named)),
    }
}

/// The colours one cell is drawn in, and whether in the bold weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::tui::screenshots) struct Ink {
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
}

/// How `cell` is inked: dim carries the foreground half way to the background, then reversed
/// swaps the two. A modifier other than bold, dim and reversed is an error: the TUI draws no
/// other.
pub(in crate::tui::screenshots) fn ink(cell: &Cell, palette: &Palette) -> Result<Ink, String> {
    let known = Modifier::BOLD | Modifier::DIM | Modifier::REVERSED;
    if !known.contains(cell.modifier) {
        return Err(format!(
            "a modifier the TUI does not draw: {:?}",
            cell.modifier - known
        ));
    }
    let mut fg = resolve(cell.fg, Side::Fg, palette)?;
    let mut bg = resolve(cell.bg, Side::Bg, palette)?;
    if cell.modifier.contains(Modifier::DIM) {
        fg = mix(fg, bg, 0.5);
    }
    if cell.modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    Ok(Ink {
        fg,
        bg,
        bold: cell.modifier.contains(Modifier::BOLD),
    })
}

/// `t` of the way from `a` to `b`, channel by channel, in sRGB.
pub(in crate::tui::screenshots) fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let one = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    (one(a.0, b.0), one(a.1, b.1), one(a.2, b.2))
}

/// `fg` over `bg` at `coverage`, blended in linear light and encoded back to sRGB.
pub(in crate::tui::screenshots) fn blend(fg: Rgb, bg: Rgb, coverage: f32) -> Rgb {
    if coverage >= 1.0 {
        return fg;
    }
    if coverage <= 0.0 {
        return bg;
    }
    let one = |f: u8, b: u8| {
        let l = to_linear(f) * coverage + to_linear(b) * (1.0 - coverage);
        to_srgb(l)
    };
    (one(fg.0, bg.0), one(fg.1, bg.1), one(fg.2, bg.2))
}

fn to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    match c <= 0.040_45 {
        true => c / 12.92,
        false => ((c + 0.055) / 1.055).powf(2.4),
    }
}

fn to_srgb(l: f32) -> u8 {
    let c = match l <= 0.003_130_8 {
        true => l * 12.92,
        false => 1.055 * l.powf(1.0 / 2.4) - 0.055,
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// The margin of the theme's ground colour around the grid in every picture, in pixels.
pub(in crate::tui::screenshots) const MARGIN: u32 = 24;

/// A picture: `width × height` pixels, three bytes each, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::tui::screenshots) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// One cell's look: what its pixels depend on, so that a repeat is drawn once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Tile {
    symbol: char,
    bold: bool,
    fg: Rgb,
    bg: Rgb,
}

/// Turns drawn buffers into pictures, remembering each distinct cell it has drawn.
pub(in crate::tui::screenshots) struct Raster {
    fonts: Fonts,
    palette: Palette,
    tiles: HashMap<Tile, Vec<u8>>,
}

impl Raster {
    /// A raster drawing with `fonts`, where a cell says `Reset` as `palette` says.
    pub(in crate::tui::screenshots) fn new(fonts: Fonts, palette: Palette) -> Raster {
        Raster {
            fonts,
            palette,
            tiles: HashMap::new(),
        }
    }

    /// The size of a cell.
    pub(in crate::tui::screenshots) fn cell(&self) -> CellSize {
        self.fonts.cell()
    }

    /// The size of the picture `buffer` paints as, margin included.
    pub(in crate::tui::screenshots) fn picture_size(&self, buffer: &Buffer) -> (u32, u32) {
        let cell = self.cell();
        (
            u32::from(buffer.area.width) * cell.w + 2 * MARGIN,
            u32::from(buffer.area.height) * cell.h + 2 * MARGIN,
        )
    }

    /// `buffer` as a picture, in a margin of the palette's ground. `place` names the picture in
    /// an error: a cell the raster cannot draw is an error rather than a hole.
    pub(in crate::tui::screenshots) fn paint(
        &mut self,
        buffer: &Buffer,
        place: &str,
    ) -> Result<Image, String> {
        let cell = self.cell();
        let (cols, rows) = (u32::from(buffer.area.width), u32::from(buffer.area.height));
        let (width, height) = self.picture_size(buffer);
        let ground = [
            self.palette.ground.0,
            self.palette.ground.1,
            self.palette.ground.2,
        ];
        let mut rgb: Vec<u8> = ground.repeat((width * height) as usize);
        for row in 0..rows {
            for col in 0..cols {
                let (x, y) = (col as u16 + buffer.area.x, row as u16 + buffer.area.y);
                let there = format!("{place}, cell ({x}, {y}): ");
                let tile = self.tile(&buffer[(x, y)], &there)?;
                for line in 0..cell.h {
                    let from = (line * cell.w * 3) as usize;
                    let to = (((MARGIN + row * cell.h + line) * width + MARGIN + col * cell.w) * 3)
                        as usize;
                    let len = (cell.w * 3) as usize;
                    rgb[to..to + len].copy_from_slice(&tile[from..from + len]);
                }
            }
        }
        Ok(Image { width, height, rgb })
    }

    /// The pixels of one cell, from the cache or drawn now.
    fn tile(&mut self, cell: &Cell, there: &str) -> Result<&[u8], String> {
        let mut chars = cell.symbol().chars();
        let symbol = match (chars.next(), chars.next()) {
            (None, _) => ' ',
            (Some(c), None) => c,
            _ => {
                return Err(format!(
                    "{there}a cell of several characters: {:?}",
                    cell.symbol()
                ));
            }
        };
        if Span::raw(cell.symbol()).width() > 1 {
            return Err(format!(
                "{there}U+{:04X} {symbol}: a wide character, which the raster does not draw",
                symbol as u32
            ));
        }
        let ink = ink(cell, &self.palette).map_err(|why| format!("{there}{why}"))?;
        // The tail of a wide character has no symbol and draws as a blank: its own background.
        let key = Tile {
            symbol,
            bold: ink.bold,
            fg: ink.fg,
            bg: ink.bg,
        };
        if !self.tiles.contains_key(&key) {
            let size = self.cell();
            let coverage = match drawn(key.symbol, size) {
                Some(drawn) => drawn.map_err(|why| format!("{there}{why}"))?,
                None => self.fonts.glyph(key.symbol, ink.bold).ok_or_else(|| {
                    format!(
                        "{there}U+{:04X} {}: neither Ubuntu Sans Mono nor DejaVu Sans has it",
                        key.symbol as u32, key.symbol
                    )
                })?,
            };
            let mut pixels = Vec::with_capacity(coverage.len() * 3);
            for c in coverage {
                let (r, g, b) = blend(ink.fg, ink.bg, c);
                pixels.extend_from_slice(&[r, g, b]);
            }
            self.tiles.insert(key, pixels);
        }
        Ok(&self.tiles[&key])
    }
}

/// Writes `image` as a binary PPM (P6).
pub(in crate::tui::screenshots) fn write_ppm(path: &Path, image: &Image) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    write!(out, "P6\n{} {}\n255\n", image.width, image.height)?;
    out.write_all(&image.rgb)?;
    out.flush()
}

/// Frames of one size appended to a file as raw RGB, for ffmpeg to read.
pub(in crate::tui::screenshots) struct RawStream {
    out: BufWriter<File>,
    path: PathBuf,
    width: u32,
    height: u32,
    frames: usize,
}

impl RawStream {
    /// A stream of `width × height` frames in the file at `path`.
    pub(in crate::tui::screenshots) fn create(
        path: &Path,
        width: u32,
        height: u32,
    ) -> std::io::Result<RawStream> {
        Ok(RawStream {
            out: BufWriter::new(File::create(path)?),
            path: path.to_path_buf(),
            width,
            height,
            frames: 0,
        })
    }

    /// Appends one frame; one of another size is an error.
    pub(in crate::tui::screenshots) fn push(&mut self, image: &Image) -> std::io::Result<()> {
        if (image.width, image.height) != (self.width, self.height) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "a {}x{} frame in a {}x{} stream",
                    image.width, image.height, self.width, self.height
                ),
            ));
        }
        self.out.write_all(&image.rgb)?;
        self.frames += 1;
        Ok(())
    }

    /// Flushes the stream and writes `<path>.size` as `W H N`. Answers the frame count.
    pub(in crate::tui::screenshots) fn finish(mut self) -> std::io::Result<usize> {
        self.out.flush()?;
        let mut name = self.path.clone().into_os_string();
        name.push(".size");
        std::fs::write(
            name,
            format!("{} {} {}\n", self.width, self.height, self.frames),
        )?;
        Ok(self.frames)
    }
}
