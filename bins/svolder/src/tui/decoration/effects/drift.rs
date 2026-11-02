// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The drifts: the tone curve's spectrum carried round the hue circle, the glow carried along
//! the palette's spectrum, both following the level, and the spectrum's colouring of the curve.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::Effects;
use crate::tui::theme::Theme;

/// How long one drift takes to go all the way round, in seconds: the spectrum once round the
/// hue circle, or the glow's ripple once through its own loop.
pub(super) const HUE_PERIOD: f32 = 20.0;
/// How far into the cool half of the spectrum the ripple reaches with no level to follow, short
/// of the cool end so a silent screen still has somewhere colder to go.
pub(super) const RIPPLE_REACH: f32 = -0.6;
/// How far the ripple carries the glow from one end of its swing to the other while something
/// plays, half either side of where the level reads: enough to see the decoration breathe, too
/// little to read as a level of its own.
pub(super) const RIPPLE_SWING: f32 = 0.1;
/// Fraction of the remaining gap to the level the glow closes over [`LEVEL_EASE_SPAN`].
/// [`Effects::render`] scales this to the real span `last_tick` covers, so a burst of readings
/// moves the decoration at the same pace whatever the loop's actual tick rate is.
pub(super) const LEVEL_EASE: f32 = 0.35;
/// The span [`LEVEL_EASE`] is measured over: the loop's slow tick, a quarter of a second.
pub(super) const LEVEL_EASE_SPAN: f32 = 0.25;
/// The first Braille character. The tone chart draws its curve in these and nothing else on
/// that screen uses them, so they are what the spectrum recolours.
const BRAILLE_FIRST: char = '\u{2800}';
/// The last Braille character.
const BRAILLE_LAST: char = '\u{28ff}';

impl Effects {
    /// Take what the frame just drawn says about the tone curve: where it drew one, if
    /// anywhere, and whether that curve is muted, which the scroll never moves.
    pub fn drew_curve(&mut self, curve: Option<Rect>, muted: bool) {
        self.curve_scrolls = curve.is_some_and(|curve| !curve.is_empty()) && !muted;
    }

    /// Whether the tone curve's spectrum is scrolling on the screen: the layer and its
    /// `spectrum_drift` switch on, over a curve the frame last armed from drew unmuted.
    pub fn curve_scrolling(&self) -> bool {
        self.enabled() && self.config.spectrum_drift && self.curve_scrolls
    }

    /// How far round the hue circle the tone curve's spectrum has drifted, in degrees. Zero
    /// while the layer is off or `spectrum_drift` is: the colouring is the curve's own, so this
    /// leaves it where the frequencies put it (red at the lowest, violet at the highest) rather
    /// than stopped wherever the drift had carried it.
    pub fn spectrum_hue(&self) -> f32 {
        match self.enabled() && self.config.spectrum_drift {
            true => self.spectrum_hue,
            false => 0.0,
        }
    }

    /// Say how loud what is playing is now, `0.0..=1.0` (the device's meters while they stream,
    /// else the host sink capture), or `None` where neither says anything (no capture running, a
    /// profile with no scale, or no reading has arrived yet).
    ///
    /// The next frame eases the glow toward this rather than jumping, so the decoration follows
    /// the music at the screen's own redraw pace. A non-finite level is treated as no level.
    pub fn set_level(&mut self, level: Option<f64>) {
        self.level = level.filter(|level| level.is_finite());
    }

    /// How far along the palette's spectrum the decoration is drawing, `-1.0..=1.0`: 0 is the
    /// accent it rests on, negative toward the cool end and positive toward the hot one.
    ///
    /// Zero whenever hue-drift or the layer itself is off, so switching the mode off puts the
    /// decoration back on the palette's plain colour on the very next frame rather than leaving
    /// it stopped somewhere along the spectrum.
    pub fn glow_position(&self) -> f32 {
        match self.enabled() {
            true => self.glow,
            false => 0.0,
        }
    }

    /// Where along the palette's spectrum the level itself reads, `0.0..=1.0`: the eased level
    /// as a position where one streams, else [`Effects::glow_position`] — so an animation asking
    /// for this draws in whatever colour the screen's own decoration is already resting or
    /// drifting at, rather than always the plain accent.
    ///
    /// This is the level as a colour rather than as a pace: hue-drift carries the whole screen's
    /// decoration along the spectrum, and this carries one animation's own. Gated on
    /// [`Effects::enabled`] exactly as `glow_position` is.
    pub fn level_position(&self) -> f64 {
        if !self.enabled() {
            return 0.0;
        }
        self.loudness().map_or_else(
            || f64::from(self.glow_position()),
            |level| level.clamp(0.0, 1.0),
        )
    }
}

/// Colour the tone curve drawn in `curve` as a spectrum by frequency, `hue_offset` degrees round
/// the circle from where it starts; or, `muted`, desaturated and resting at its start regardless
/// of `hue_offset`.
///
/// The chart's x axis is logarithmic in frequency and the colour runs evenly across it
/// ([`Theme::curve_colour`]): red at the lowest, violet at the highest (or, on a palette with its
/// own curve stops, from the first to the last — `red`'s dark red to off-white). This colouring
/// is the curve's own and is applied regardless of the animation layer; the layer only supplies
/// `hue_offset`, which carries the spectrum round the circle and rests at zero while off.
///
/// A shaped curve (EQ mode, a band off 0 dB) recolours only the Braille cells the line is drawn
/// with, leaving the bands' own point cells in their themed roles. A muted curve (flat in EQ
/// mode, or Flat mode — see [`Graph`](crate::tui::screens::tone) and
/// [`curve_of`](crate::tui::screens::tone::curve_of)) recolours the Dot point cells too, since
/// curve and points read as one faded shape there. Axes and labels keep their themed roles
/// either way; `hue_offset` is ignored when `muted`, since the rainbow scroll never plays over a
/// curve the bands aren't applying it round. A theme with no spectrum (16 colours, or monochrome)
/// leaves the curve in whatever role the widgets drew it in
/// ([`Role::CurveSum`](crate::tui::theme::Role::CurveSum) shaped,
/// [`Role::Dim`](crate::tui::theme::Role::Dim) muted).
pub fn spectrum(buf: &mut Buffer, curve: Rect, theme: &Theme, hue_offset: f32, muted: bool) {
    let span = f32::from(curve.width.saturating_sub(1)).max(1.0);
    let hue_offset = if muted { 0.0 } else { hue_offset };
    for x in curve.x..curve.right() {
        let along = f32::from(x - curve.x) / span;
        let Some(colour) = theme.curve_colour(along, hue_offset, muted) else {
            // A theme with no spectrum has none for any column of the curve.
            return;
        };
        for y in curve.y..curve.bottom() {
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            if is_braille(cell.symbol()) || (muted && is_dot(cell.symbol())) {
                cell.set_fg(colour);
            }
        }
    }
}

/// Whether `symbol` is one of the Braille characters the response curve is drawn with.
pub(super) fn is_braille(symbol: &str) -> bool {
    symbol
        .chars()
        .next()
        .is_some_and(|c| (BRAILLE_FIRST..=BRAILLE_LAST).contains(&c))
}

/// Whether `symbol` is the dot a band's centre is marked with.
fn is_dot(symbol: &str) -> bool {
    symbol == ratatui::symbols::DOT
}
