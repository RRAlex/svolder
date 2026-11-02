// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fonts the raster draws text from: Ubuntu Sans Mono at two weights, and DejaVu Sans for
//! the characters it lacks. One scale fits the mono font's advance to sixteen pixels, and every
//! font is drawn at it.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, VariableFont, point};

use super::geometry::{CellSize, Coverage};

/// Where Ubuntu Sans Mono (a variable font) lives: Debian and Ubuntu.
pub(in crate::tui::screenshots) const UBUNTU_SANS_MONO: &[&str] =
    &["/usr/share/fonts/truetype/ubuntu/UbuntuSansMono[wght].ttf"];

/// Where DejaVu Sans lives: Debian and Ubuntu, Fedora, Arch.
pub(in crate::tui::screenshots) const DEJAVU_SANS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

/// Where DejaVu Sans Bold lives, in the same places.
pub(in crate::tui::screenshots) const DEJAVU_SANS_BOLD: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
];

/// The width of a cell, in pixels.
const CELL_WIDTH: f32 = 16.0;

/// A font the machine does not have.
#[derive(Debug)]
pub(in crate::tui::screenshots) struct Missing {
    /// The Debian package that carries it.
    pub package: &'static str,
    /// The paths looked at, each with what was wrong with it.
    pub tried: Vec<String>,
    /// The font's name.
    name: &'static str,
}

impl Missing {
    /// What to tell the user: the font, the paths tried and the package to install.
    pub(in crate::tui::screenshots) fn message(&self) -> String {
        format!(
            "{} not found (tried {}): install {}",
            self.name,
            self.tried.join(", "),
            self.package
        )
    }
}

/// The fonts, scaled to the cell.
pub(in crate::tui::screenshots) struct Fonts {
    mono: FontVec,
    mono_bold: FontVec,
    sans: FontVec,
    sans_bold: FontVec,
    scale: PxScale,
    cell: CellSize,
    baseline: f32,
}

/// Reads the first of `paths` that is a font, or says what was tried.
fn read(paths: &[&str], name: &'static str, package: &'static str) -> Result<FontVec, Missing> {
    let mut tried = Vec::new();
    for path in paths {
        match std::fs::read(path) {
            Err(why) => tried.push(format!("{path}: {why}")),
            Ok(bytes) => match FontVec::try_from_vec(bytes) {
                Ok(font) => return Ok(font),
                Err(why) => tried.push(format!("{path}: {why}")),
            },
        }
    }
    Err(Missing {
        package,
        tried,
        name,
    })
}

/// The mono font at a weight.
fn weighted(mono: &[&str], weight: f32) -> Result<FontVec, Missing> {
    let mut font = read(mono, "Ubuntu Sans Mono", "fonts-ubuntu")?;
    if !font.set_variation(b"wght", weight) {
        return Err(Missing {
            package: "fonts-ubuntu",
            tried: vec![format!("{mono:?}: no weight axis")],
            name: "Ubuntu Sans Mono",
        });
    }
    Ok(font)
}

/// Loads the fonts from the given paths, the mono font first.
pub(in crate::tui::screenshots) fn load(
    mono: &[&str],
    sans: &[&str],
    sans_bold: &[&str],
) -> Result<Fonts, Missing> {
    let mono_regular = weighted(mono, 400.0)?;
    let mono_bold = weighted(mono, 700.0)?;
    let sans = read(sans, "DejaVu Sans", "fonts-dejavu-core")?;
    let sans_bold = read(sans_bold, "DejaVu Sans Bold", "fonts-dejavu-core")?;
    let unit = mono_regular.as_scaled(PxScale::from(1.0));
    let scale = PxScale::from(CELL_WIDTH / unit.h_advance(mono_regular.glyph_id('M')));
    let scaled = mono_regular.as_scaled(scale);
    let height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).round();
    let baseline = (scaled.line_gap() / 2.0 + scaled.ascent()).round();
    Ok(Fonts {
        cell: CellSize {
            w: CELL_WIDTH as u32,
            h: height as u32,
        },
        mono: mono_regular,
        mono_bold,
        sans,
        sans_bold,
        scale,
        baseline,
    })
}

/// Loads the fonts from where the distributions put them.
pub(in crate::tui::screenshots) fn load_system() -> Result<Fonts, Missing> {
    load(UBUNTU_SANS_MONO, DEJAVU_SANS, DEJAVU_SANS_BOLD)
}

impl Fonts {
    /// The size of a cell.
    pub(in crate::tui::screenshots) fn cell(&self) -> CellSize {
        self.cell
    }

    /// How much of each pixel of a cell `c` covers, from the mono font, else centred from DejaVu
    /// Sans, else `None`.
    pub(in crate::tui::screenshots) fn glyph(&self, c: char, bold: bool) -> Option<Coverage> {
        let (mono, sans) = match bold {
            true => (&self.mono_bold, &self.sans_bold),
            false => (&self.mono, &self.sans),
        };
        match mono.glyph_id(c).0 != 0 {
            true => Some(self.draw(mono, c, 0.0)),
            false if sans.glyph_id(c).0 != 0 => {
                let advance = sans.as_scaled(self.scale).h_advance(sans.glyph_id(c));
                Some(self.draw(sans, c, (CELL_WIDTH - advance) / 2.0))
            }
            false => None,
        }
    }

    /// `c` from `font`, its origin `x` pixels from the cell's left edge, on the baseline.
    fn draw(&self, font: &FontVec, c: char, x: f32) -> Coverage {
        let (w, h) = (self.cell.w as i64, self.cell.h as i64);
        let mut cov = vec![0.0f32; (w * h) as usize];
        let glyph = font
            .glyph_id(c)
            .with_scale_and_position(self.scale, point(x, self.baseline));
        let Some(outline) = font.outline_glyph(glyph) else {
            return cov;
        };
        let min = outline.px_bounds().min;
        let (ox, oy) = (min.x.floor() as i64, min.y.floor() as i64);
        outline.draw(|gx, gy, value| {
            let (px, py) = (ox + i64::from(gx), oy + i64::from(gy));
            if (0..w).contains(&px) && (0..h).contains(&py) {
                let at = (py * w + px) as usize;
                cov[at] = cov[at].max(value);
            }
        });
        cov
    }
}
