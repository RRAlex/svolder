// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fast tests `make check` runs: no ffmpeg, no network, and a skip where a font is missing.

mod animation;

mod text;

mod fonts;

mod generate;

mod raster;

mod stills;

mod world;

use crate::tui::screenshots::raster::Palette;

/// The amber theme's ground and text, as a literal, for the raster tests to share.
pub(super) fn palette() -> Palette {
    Palette {
        ground: (7, 6, 5),
        text: (0xf2, 0xf0, 0xea),
    }
}
