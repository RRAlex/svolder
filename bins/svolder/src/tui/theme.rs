// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the pieces of a screen mean, and the styles a theme gives them.
//!
//! A screen never names a colour, only a role; the theme decides how that role looks.
//! [`Theme::from_config`] resolves that once, from the `[tui]` config, `--no-color`/`NO_COLOR`,
//! and the terminal's colour depth, into a [`Theme`] every screen draws through for the run.
//!
//! Colours come from a [`Preset`]: a table naming an accent per device family, the three
//! semantic colours, and the text greys. [`Theme::set_family`] says which family the screen is
//! showing, and every accented role follows it, so a DAC and an amp read as two colours. A
//! `[tui]` `accent` overrides all three accents; `[tui.accents]` overrides them one at a time.
//!
//! A preset also carries the stops the decoration travels (a cool end below the accent, a hot
//! end above it), and [`Theme::set_glow`] says how far along them the screen is drawing. The
//! rule, the key hints and an unfocused pane's title (only on a preset with no emphasis colour)
//! take that colour ([`Theme::glow`]); roles that carry a value — a bar, the cursor, the
//! selected row, a curve's points — keep the family's resting accent ([`Theme::accent`]), so a
//! reading never changes colour with the music. `[tui.spectrum]` `stops` replaces a preset's own.
//!
//! A preset may name an emphasis colour ([`Theme::emphasis`]), which then draws the bars, the
//! selected row's inverse video and the pane titles in place of the accent, leaving the accent
//! for the rule, the key hints, the `▶` marker and the frame the keys are in (`red` does this, so
//! it reads as a red-ruled page of off-white on black). Such a preset draws unfocused frames
//! faint. Any preset may name its own tone-curve stops in place of the hue circle (`red` and
//! `neon` do, [`Theme::curve_colour`]).
//!
//! Monochrome (`--no-color`, `NO_COLOR`, or `monochrome = true`) drops every colour and marks the
//! selected row by reverse video alone. At 16 colours each preset colour takes the ANSI slot it
//! means: accents take the slots the preset names for its families (yellow, cyan and red on
//! `amber` and `ice`; bright magenta, cyan and magenta on `neon`; red, red and bright red on
//! `red`), emphasis takes white, the semantic colours are green,
//! yellow and red, greys are the two ANSI greys — a nearest-RGB reduction would otherwise land a
//! muted colour on a grey. At 256 and true colour the preset's own values are drawn (reduced
//! through the cube where needed); the extra depth also feeds the level bar and meter gradients
//! ([`Theme::bar_shade`], [`Theme::glow_shade`], [`Theme::gradient`]). Colour tables and
//! reductions live in [`palette`], which this module reduces through.

use ratatui::style::{Color, Modifier, Style};
use svolder_ipc::{ColorDepth, TuiConfig};
use svolder_profiles::Family;

use super::palette;

/// A colour as a preset names it and the reductions take it: its red, green and blue bytes.
pub type Rgb = (u8, u8, u8);

/// What a piece of the screen means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Text a user reads a value from.
    Text,
    /// A secondary value or a column title, held back from the text around it.
    Dim,
    /// The part of a widget that is not there yet: a bar's empty cells, a track's ground.
    Faint,
    /// A state that reads as healthy.
    Good,
    /// A state that wants watching.
    Warn,
    /// A state that is wrong now.
    Alert,
    /// The accent rule under the header.
    Rule,
    /// A state worth a badge of its own, drawn in inverse video.
    Badge,
    /// The header's own name and identity.
    Header,
    /// The connection dot and word, with the link up.
    Connected,
    /// The connection dot and word, with the link down.
    Disconnected,
    /// A value that is known to be out of date.
    Stale,
    /// The focused row.
    Selected,
    /// The `▶` that marks the focused row.
    Cursor,
    /// A toggle that reads on.
    ToggleOn,
    /// The name a select gives its value.
    SelectName,
    /// A level bar's filled cells.
    Bar,
    /// A meter inside its normal range.
    MeterOk,
    /// A meter above the class-A tick.
    MeterHot,
    /// A meter near full scale.
    MeterClip,
    /// A fault the device reports.
    Fault,
    /// The tone control's summed response curve, where the theme has no spectrum to draw it in.
    CurveSum,
    /// The focused band's point on that curve.
    CurveSelected,
    /// Every other band's point.
    CurvePoint,
    /// The imported correction's target curve, drawn dotted under the fit.
    CurveTarget,
    /// The stretch where the fit misses the target by more than the gap's threshold, filled
    /// under both curves.
    CurveGap,
    /// The curve's 0 dB axis.
    Axis,
    /// A reading no control names.
    Unknown,
    /// An update rate above once per second.
    RateFast,
    /// A message about the last thing that was asked for.
    Notice,
    /// A key's own label on a border word, the wizard's key row and the help overlay.
    KeyHint,
    /// What a key does, on the wizard's key row.
    KeyDesc,
    /// The title of a pane the keys are not in.
    Title,
    /// The `‹ ›` the tone control's mode is stepped with.
    ModeArrow,
}

/// How dark [`Theme::accent_shade`]'s cold end is, as a fraction of the accent's own
/// brightness.
const SHADE_DARK: f64 = 0.4;

/// How close the accent may come to the preset's alert colour before a fault stops relying on
/// its colour alone and takes the inverse-video row as well.
const ALERT_NEAR: f64 = 80.0;

/// How far toward white [`Theme::glow_lift_at`] carries the glow: far enough to read as a
/// brighter band of the same colour, short of white (which would read as a colour of its own).
const GLOW_LIFT: f64 = 0.6;

/// The white a lifted colour is carried toward.
const WHITE: Rgb = (0xff, 0xff, 0xff);

/// The grey the animation layer fades a reading up from once the link comes back.
pub const DIM_GREY: Color = Color::Rgb(0x66, 0x66, 0x66);

/// The whole hue circle, in degrees.
pub const HUE_CIRCLE: f32 = 360.0;
/// How much of the hue circle the tone curve spans from its lowest frequency to its highest, in
/// degrees, on a preset with no stops of its own: red at 0° to violet, short of the full circle
/// so the two ends never meet in the same colour.
const SPECTRUM_ARC: f32 = 270.0;
/// How saturated a spectrum colour is.
const SPECTRUM_SATURATION: f32 = 1.0;
/// How light one is: bright enough to read on a dark terminal, dark enough on a light one.
const SPECTRUM_LIGHTNESS: f32 = 0.55;
/// How much of a muted spectrum colour's own saturation [`mute`] keeps against its luma grey:
/// about a third.
const MUTE_SATURATION: f32 = 0.33;
/// How much darker [`mute`] leaves the blend, once desaturated.
const MUTE_DARKEN: f32 = 0.85;

/// Primary text: off-white rather than white, so a full screen of it is not a wall of glare.
const TEXT: Rgb = (0xf2, 0xf0, 0xea);
/// Secondary values and column titles.
const DIM: Rgb = (0x8a, 0x8a, 0x8a);
/// What a widget draws where nothing is yet.
const FAINT: Rgb = (0x5a, 0x5a, 0x5a);
/// The green three of the four presets read healthy in.
const GOOD: Rgb = (0x5f, 0xbf, 0x6f);
/// The amber three of the four presets warn in.
const WARN: Rgb = (0xe0, 0xc0, 0x40);
/// The red three of the four presets alert in.
const ALERT: Rgb = (0xff, 0x3b, 0x30);

/// One palette: an accent per device family and the ANSI slots they take at 16 colours, the
/// three semantic colours, the text greys, the stops the glow travels, and optionally an
/// emphasis colour and the stops the tone curve runs through.
///
/// No family accent may be one of the preset's own semantic colours, so a fault never dresses
/// as the thing it interrupts.
pub struct Preset {
    /// What `palette` under `[tui]` calls it.
    pub name: &'static str,
    /// What a DAC accents with.
    pub dac: Rgb,
    /// What an amplifier accents with.
    pub amp: Rgb,
    /// What a preamplifier accents with.
    pub preamp: Rgb,
    /// A state that reads as healthy.
    pub good: Rgb,
    /// A state that wants watching.
    pub warn: Rgb,
    /// A state that is wrong now.
    pub alert: Rgb,
    /// Text a user reads a value from.
    pub text: Rgb,
    /// Secondary values and column titles.
    pub dim: Rgb,
    /// What a widget draws where nothing is yet.
    pub faint: Rgb,
    /// The stops the glow travels, cool end to hot end.
    pub spectrum: &'static [Rgb],
    /// Which of those stops the accent rests on: the glow leaves from here toward the cool end
    /// below it and the hot end above it. This is the accent the palette gives a DAC, so a glow
    /// of nothing draws exactly what the palette names.
    pub rest: usize,
    /// The ANSI slots the DAC, amplifier and preamplifier accents take at 16 colours, in that
    /// order.
    pub slots: [Color; 3],
    /// What draws the level and meter bars, the selected row's inverse video and the pane
    /// titles in place of the accent, or `None` to leave them in the accent. At 16 colours it
    /// takes the text's own slot, the ANSI white. The accent keeps the rule, the key hints, the
    /// `▶` marker and the frame the keys are in either way; with an emphasis colour every other
    /// frame is drawn faint, so the accent marks where the keys are.
    pub emphasis: Option<Rgb>,
    /// The stops the tone curve's colours run through from its lowest frequency to its highest,
    /// spread evenly along it, or `None` for the hue circle's red to violet.
    pub curve: Option<&'static [Rgb]>,
    /// The three backgrounds the palette paints: the whole frame, a pane's inside and border,
    /// and a raised box.
    pub greys: [Rgb; 3],
}

/// Which of a palette's three background greys a cell takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The whole frame.
    Ground,
    /// A pane's inside and border.
    Pane,
    /// A box over the screen: the menu, the Settings box, help, the questions, the editors and
    /// the toasts.
    Raised,
}

/// The ANSI slots amber and ice accent a DAC, an amplifier and a preamplifier in at 16 colours.
const SLOTS: [Color; 3] = [Color::Yellow, Color::Cyan, Color::Red];

/// Amber on every device: warm gold for DACs, copper for amplifiers, a pale honey for
/// preamplifiers.
const AMBER: Preset = Preset {
    name: "amber",
    dac: (0xe0, 0xa0, 0x30),
    amp: (0xb8, 0x6a, 0x2a),
    preamp: (0xf6, 0xcc, 0x7a),
    good: GOOD,
    warn: WARN,
    alert: ALERT,
    text: TEXT,
    dim: DIM,
    faint: FAINT,
    // The default palette's own stops, which the configuration file's template also reads
    // from here rather than keeping a second copy of the same four colours.
    spectrum: &svolder_ipc::config::DEFAULT_SPECTRUM,
    rest: 2,
    slots: SLOTS,
    emphasis: None,
    curve: None,
    greys: [(0x07, 0x06, 0x05), (0x0b, 0x0a, 0x08), (0x10, 0x0e, 0x0b)],
};

/// Ice on every device: teal for DACs, a steel blue for amplifiers, a pale frost for
/// preamplifiers.
const ICE: Preset = Preset {
    name: "ice",
    dac: (0x5f, 0xc9, 0xc9),
    amp: (0x1f, 0x6a, 0xa8),
    preamp: (0xa8, 0xe6, 0xf0),
    good: GOOD,
    warn: WARN,
    alert: ALERT,
    text: TEXT,
    dim: DIM,
    faint: FAINT,
    spectrum: &[
        (0x12, 0x3a, 0x7a),
        (0x2a, 0x8a, 0xd0),
        (0x5f, 0xc9, 0xc9),
        (0xdf, 0xf6, 0xff),
    ],
    rest: 2,
    slots: SLOTS,
    emphasis: None,
    curve: None,
    greys: [(0x05, 0x06, 0x07), (0x08, 0x0b, 0x0d), (0x0c, 0x10, 0x15)],
};

/// A Schiit-styled terminal: off-white text, bars and selection on the near-black a
/// terminal draws, labels in a dim grey, and Schiit red — scarlet for DACs, crimson for
/// amplifiers, salmon for preamplifiers — only on the rule, the key hints, the `▶` marker and
/// the frame the keys are in; green for good states and amber for warnings, and nothing blue
/// or cyan anywhere, the tone curve included. The alert colour brightens away from the scarlet
/// so a fault still parts from the accent it sits beside.
const RED: Preset = Preset {
    name: "red",
    dac: (0xe0, 0x34, 0x1c),
    amp: (0xb0, 0x1c, 0x3c),
    preamp: (0xff, 0xa8, 0x90),
    good: (0x6f, 0xbf, 0x73),
    warn: (0xd9, 0xa4, 0x41),
    alert: (0xff, 0x5a, 0x4a),
    text: TEXT,
    dim: (0x7a, 0x7a, 0x7a),
    faint: FAINT,
    spectrum: &[(0x6a, 0x12, 0x08), (0xe0, 0x34, 0x1c), (0xff, 0x8a, 0x5a)],
    rest: 1,
    slots: [Color::Red, Color::Red, Color::LightRed],
    emphasis: Some(TEXT),
    // Dark red at 20 Hz through the scarlet and the warning amber to the off-white at 20 kHz:
    // the dark red is as dark as a line of Braille dots can go and still read on a black
    // terminal.
    curve: Some(&[
        (0xa0, 0x24, 0x14),
        (0xe0, 0x34, 0x1c),
        (0xd9, 0xa4, 0x41),
        TEXT,
    ]),
    greys: [(0x0c, 0x0c, 0x0c), (0x14, 0x14, 0x14), (0x1e, 0x1e, 0x1e)],
};

/// Synthwave: an orchid pink for DACs, cyan for amplifiers and violet for preamplifiers, three
/// hues of their own rather than shades of one, on purple-black greys, with the semantic colours
/// lifted to match and each kept a hue away from every accent. The glow cools to a deep violet
/// and heats through a pale pink to a pink-white, and the tone curve runs a sunset: violet, hot
/// pink, orange, gold. At 16 colours the families take the bright magenta, the cyan and the
/// magenta, none of them a semantic colour's slot.
pub(crate) const NEON: Preset = Preset {
    name: "neon",
    dac: (0xff, 0x51, 0xfd),
    amp: (0x2d, 0xe2, 0xe6),
    preamp: (0xa3, 0x6b, 0xff),
    good: (0x6c, 0xff, 0x7a),
    warn: (0xff, 0xd8, 0x4a),
    alert: (0xff, 0x4a, 0x2e),
    text: TEXT,
    dim: DIM,
    faint: FAINT,
    spectrum: &[
        (0x6a, 0x3c, 0xff),
        (0xff, 0x51, 0xfd),
        (0xff, 0xb0, 0xec),
        (0xff, 0xf3, 0xfb),
    ],
    rest: 1,
    slots: [Color::LightMagenta, Color::Cyan, Color::Magenta],
    emphasis: None,
    curve: Some(&[
        (0x7b, 0x2f, 0xff),
        (0xff, 0x3e, 0xa5),
        (0xff, 0x8a, 0x4c),
        (0xff, 0xd3, 0x6b),
    ]),
    greys: [(0x0a, 0x06, 0x12), (0x10, 0x0a, 0x1c), (0x17, 0x10, 0x26)],
};

/// Every palette that ships, in the order the config's own error lists them.
const PRESETS: [&Preset; 4] = [&AMBER, &ICE, &RED, &NEON];

/// The preset `name` names, or `None` for a name none of them carries.
pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.into_iter().find(|preset| preset.name == name)
}

/// What every palette that ships is called, in the order the Settings box steps through
/// them.
pub fn palettes() -> [&'static str; 4] {
    PRESETS.map(|preset| preset.name)
}

impl Preset {
    /// What `family` accents with in this palette.
    pub fn family(&self, family: Family) -> Rgb {
        match family {
            Family::Dac => self.dac,
            Family::Amp => self.amp,
            Family::Preamp => self.preamp,
        }
    }
}

/// An accent a `[tui]` key names, resolved once: a named colour draws as the same ratatui
/// colour at every depth, while a `#rrggbb` value is reduced per depth when it is drawn flat
/// and interpolated in RGB when it is ramped.
#[derive(Clone, Copy)]
enum Accent {
    Named(Color, Rgb),
    Hex(Rgb),
}

impl Accent {
    /// The accent `text` names, or `None` for a colour this cannot read, which then overrides
    /// nothing: `Config::parse` rejects anything that is neither one of the 16 ANSI names nor
    /// a `#rrggbb` value before it ever reaches here.
    fn parse(text: &str) -> Option<Accent> {
        if let Some(rgb) = palette::hex(text) {
            return Some(Accent::Hex(rgb));
        }
        palette::named(text).map(|(color, rgb)| Accent::Named(color, rgb))
    }

    /// The RGB behind this accent, for the ramps that blend it.
    fn rgb(self) -> Rgb {
        match self {
            Accent::Named(_, rgb) | Accent::Hex(rgb) => rgb,
        }
    }
}

/// The styles the screens draw with, resolved once from the `[tui]` config, `--no-color`, and
/// the terminal's colour depth, and pointed at a device family for as long as that device is
/// the one on the screen.
pub struct Theme {
    /// The colour depth to draw at; never [`ColorDepth::Auto`].
    depth: ColorDepth,
    /// Whether every role drops its colour.
    monochrome: bool,
    /// The palette the colours come from.
    preset: &'static Preset,
    /// What the config accents each family with instead of the preset, in [`Theme::slot`]
    /// order.
    overrides: [Option<Accent>; 3],
    /// The family whose accent the screen is drawing with.
    family: Family,
    /// The stops the glow travels: the preset's own, or the ones `[tui.spectrum]` names.
    stops: Vec<Rgb>,
    /// Which of [`Theme::stops`] the accent rests on.
    rest: usize,
    /// How far along those stops the decoration is drawing, `-1.0..=1.0`; 0 is the accent the
    /// current family rests at.
    glow: f32,
    /// Whether the palette paints its backgrounds, from `theme_background` under `[tui]`.
    theme_background: bool,
}

impl Theme {
    /// A theme in the 16 ANSI colours on the default palette, or without colour at all when
    /// `monochrome`: what a case draws through when the depth is not what it is
    /// about. Real code builds a [`Theme`] from [`Theme::from_config`], so this is for tests.
    #[cfg(test)]
    pub fn new(monochrome: bool) -> Theme {
        Theme::from_config(&TuiConfig::default(), monochrome, ColorDepth::Ansi16)
    }

    /// The theme `config`, `no_color`, and `detected` resolve to: `config.colors` overrides
    /// `detected` unless it is [`ColorDepth::Auto`], and `no_color || config.monochrome` wins
    /// over both, dropping every colour (though [`Theme::depth`] still reports the depth that
    /// was resolved, which the effects gate reads regardless of monochrome).
    ///
    /// A single `accent` overrides all three family accents; `[tui.accents]` overrides them
    /// one at a time, and `[tui.spectrum]` `stops` takes the place of the palette's own
    /// stops, resting on whichever of them comes nearest the accent a DAC is drawn in. The
    /// theme starts on the family [`Family::default`] names, until [`Theme::set_family`] says
    /// which device is on the screen.
    ///
    /// An unknown palette name draws as `amber`: `Config::parse` refuses such a name before it
    /// gets here, and the theme is total so a frame can always be drawn.
    pub fn from_config(config: &TuiConfig, no_color: bool, detected: ColorDepth) -> Theme {
        let depth = match config.colors {
            ColorDepth::Auto => detected,
            forced => forced,
        };
        let preset = preset(&config.palette).unwrap_or(&AMBER);
        let overrides = match config.accent.as_deref().and_then(Accent::parse) {
            Some(every) => [Some(every); 3],
            None => [
                config.accents.dac.as_deref(),
                config.accents.amp.as_deref(),
                config.accents.preamp.as_deref(),
            ]
            .map(|named| named.and_then(Accent::parse)),
        };
        let named: Vec<Rgb> = config
            .spectrum
            .iter()
            .filter_map(|stop| palette::hex(stop))
            .collect();
        let (stops, rest) = match named.is_empty() {
            true => (preset.spectrum.to_vec(), preset.rest),
            false => {
                let dac = overrides[0].map_or(preset.dac, Accent::rgb);
                let rest = resting_stop(&named, dac);
                (named, rest)
            }
        };
        Theme {
            depth,
            monochrome: no_color || config.monochrome,
            preset,
            overrides,
            family: Family::default(),
            stops,
            rest,
            glow: 0.0,
            theme_background: config.theme_background,
        }
    }

    /// `rgb` as the colour this theme's depth draws it in.
    pub fn quantised(&self, rgb: Rgb) -> Color {
        quantise(rgb, self.depth)
    }

    /// `rgb` as the grey of its own luma, darkened by the ratio the palette's dim grey bears to
    /// its text, at this theme's depth: what a picture under a box that dims the screen keeps of
    /// its shading, as dark beside the dimmed text as its colours were beside the text. At 256
    /// colours the grey ramp's 24 steps are used as well as the cube's greys.
    pub fn dimmed(&self, rgb: Rgb) -> Color {
        let scale = luma(self.preset.dim) / luma(self.preset.text);
        let level = (luma(rgb) * scale).round().clamp(0.0, 255.0) as u8;
        match self.depth {
            ColorDepth::Ansi256 => palette::nearest_grey_256(level),
            depth => quantise((level, level, level), depth),
        }
    }

    /// The current family's accent when `[tui] accent` or `[tui.accents]` overrides the
    /// preset's own, else `None`.
    pub fn accent_override(&self) -> Option<Rgb> {
        self.overrides[self.slot()].map(Accent::rgb)
    }

    /// Draw the accented roles in `family`'s own accent from here on.
    pub fn set_family(&mut self, family: Family) {
        self.family = family;
    }

    /// Carry the decoration `t` of the way along the palette's stops: what the meters move as
    /// the music does.
    ///
    /// `t` runs `-1.0..=1.0` and is clamped to it: 0 is the colour the current family rests
    /// at, -1 the cool end of the stops and 1 the hot end. Only the decoration travels — the
    /// rule, the key hints and, on a palette with no emphasis colour, an unfocused pane's title.
    /// The accent every value-bearing role is drawn in stays where it rests, as does the
    /// emphasis; the semantic colours a fault, a warning and a good state are drawn in stay
    /// exactly where the preset put them, and so does the reverse video a fault takes when the
    /// palette draws it near the accent.
    pub fn set_glow(&mut self, t: f32) {
        self.glow = t.clamp(-1.0, 1.0);
    }

    /// The RGB the current family accents with: the colour it rests at, which is the config's
    /// override for its family, else the preset's own.
    pub fn accent_rgb(&self) -> Rgb {
        self.rest_rgb()
    }

    /// The RGB the glow is at: the resting accent itself at 0, the cool end of the stops at
    /// -1, the hot end at 1, and a blend of the two stops either side of it in between.
    ///
    /// The stop the spectrum rests on is drawn as the accent rather than as the colour the
    /// palette put there, so the glow leaves from whatever the family on the screen is
    /// accented with — an amplifier's copper as readily as a DAC's gold — and a glow of nothing
    /// is that accent exactly.
    pub fn glow_rgb(&self) -> Rgb {
        self.glow_rgb_at(self.glow)
    }

    /// The RGB the spectrum is at `t` of the way along it, `-1.0..=1.0` and clamped to it the
    /// way [`Theme::set_glow`] clamps the glow itself.
    fn glow_rgb_at(&self, t: f32) -> Rgb {
        let t = t.clamp(-1.0, 1.0);
        let Some(last) = self.stops.len().checked_sub(1) else {
            return self.rest_rgb();
        };
        let stop = |i: usize| match i == self.rest {
            true => self.rest_rgb(),
            false => self.stops[i],
        };
        let from = self.rest as f32;
        let to = match t < 0.0 {
            true => 0.0,
            false => last as f32,
        };
        let along = from + (to - from) * t.abs();
        let i = (along.floor() as usize).min(last);
        let j = (along.ceil() as usize).min(last);
        lerp(stop(i), stop(j), f64::from(along - along.floor()))
    }

    /// What the current family accents with at rest.
    fn rest_rgb(&self) -> Rgb {
        self.slot_rgb(self.slot())
    }

    /// What one family slot accents with: the config's override for it, else the preset's own.
    fn slot_rgb(&self, slot: usize) -> Rgb {
        match self.overrides[slot] {
            Some(accent) => accent.rgb(),
            None => [self.preset.dac, self.preset.amp, self.preset.preamp][slot],
        }
    }

    /// How that accent draws flat: a named override as its own ratatui colour at every depth,
    /// the preset's own accent as the slot the preset names for its family at 16 colours, and
    /// the reduction of [`Theme::accent_rgb`] otherwise.
    pub fn accent(&self) -> Color {
        match self.overrides[self.slot()] {
            Some(Accent::Named(color, _)) => color,
            None if matches!(self.depth, ColorDepth::Ansi16 | ColorDepth::Auto) => {
                self.preset.slots[self.slot()]
            }
            _ => quantise(self.accent_rgb(), self.depth),
        }
    }

    /// What the bars, the selected row and the pane titles draw in: the preset's emphasis
    /// colour where it names one — the text's own white at 16 colours — and the accent where it
    /// does not. An accent the config overrides moves only the accent's own roles, never the
    /// emphasis.
    pub fn emphasis(&self) -> Color {
        match self.preset.emphasis {
            Some(rgb) => self.ink(rgb, Color::Gray),
            None => self.accent(),
        }
    }

    /// Whether the preset names an emphasis colour of its own, and so draws the frames the keys
    /// are not in faint rather than as a palette without one draws them.
    pub fn emphasised(&self) -> bool {
        self.preset.emphasis.is_some()
    }

    /// How the glow draws flat: the accent itself wherever the glow rests on it, and at 16
    /// colours, where the slots are too far apart to show a colour on its way between two of
    /// them; the reduction of [`Theme::glow_rgb`] anywhere else.
    pub fn glow(&self) -> Color {
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => self.accent(),
            _ if self.glow == 0.0 => self.accent(),
            depth => quantise(self.glow_rgb(), depth),
        }
    }

    /// The spectrum at `position` carried [`GLOW_LIFT`] of the way to white: the same colour,
    /// brighter. `position` runs `-1.0..=1.0`, clamped as [`Theme::set_glow`] clamps the glow (0
    /// the resting accent, 1 the spectrum's hot end).
    ///
    /// A pane's contour is drawn in the glow itself, so a light running round it parts from what
    /// it runs over by being brighter, not by being another colour, which keeps the palette's
    /// colour on screen while the light passes. `position` is the caller's own rather than the
    /// screen's current glow, so an animation following the level (the device's meters while
    /// streaming, else the host sink capture) is coloured by loudness independent of whether the
    /// whole screen's decoration is travelling too. At 16 colours there is no brighter accent to
    /// lift into, so this answers the accent exactly as [`Theme::glow`] does — the light is told
    /// apart by weight there instead; this codebase's runner never hits that branch (its layer
    /// needs true colour) but it is exercised directly by this module's tests.
    pub fn glow_lift_at(&self, position: f64) -> Color {
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => self.accent(),
            depth => quantise(
                lerp(self.glow_rgb_at(position as f32), WHITE, GLOW_LIFT),
                depth,
            ),
        }
    }

    /// The colour `t` of the way along the palette's spectrum, `-1.0..=1.0` as the glow runs
    /// it — the cool end at -1, the resting accent at 0, the hot end at 1 — or `None` where the
    /// theme has no colour to draw a spread of them in: 16 colours, whose slots are too far
    /// apart for a colour between two of them, and monochrome.
    pub fn along_spectrum(&self, t: f32) -> Option<Color> {
        match self.depth {
            _ if self.monochrome => None,
            ColorDepth::Ansi16 | ColorDepth::Auto => None,
            depth => Some(quantise(self.glow_rgb_at(t), depth)),
        }
    }

    /// Where the current family's override sits in [`Theme::overrides`].
    fn slot(&self) -> usize {
        match self.family {
            Family::Dac => 0,
            Family::Amp => 1,
            Family::Preamp => 2,
        }
    }

    /// The colour depth this theme draws at. Never [`ColorDepth::Auto`], even when monochrome.
    pub fn depth(&self) -> ColorDepth {
        self.depth
    }

    /// Whether every role draws with no colour at all (selection by reverse video only).
    pub fn monochrome(&self) -> bool {
        self.monochrome
    }

    /// The background `surface` is painted in: the palette's grey at this depth (at 256 colours
    /// an entry of the cube and the grey ramp together, the three kept apart and in order by
    /// [`palette::surfaces_256`]) while `theme_background` is on, or `None` at 16 colours, in
    /// monochrome and with the switch off, where the terminal's own background shows.
    pub fn surface(&self, surface: Surface) -> Option<Color> {
        if self.monochrome || !self.theme_background {
            return None;
        }
        let at = match surface {
            Surface::Ground => 0,
            Surface::Pane => 1,
            Surface::Raised => 2,
        };
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => None,
            ColorDepth::Ansi256 => Some(palette::surfaces_256(self.preset.greys)[at]),
            depth => Some(quantise(self.preset.greys[at], depth)),
        }
    }

    /// The style one role draws with.
    ///
    /// Monochrome keeps what a role says about weight and drops what it says about colour: a
    /// stale row still dims, a header and a select's value name still stand out, and the
    /// focused row is marked by reverse video, which is the only mark left to give it.
    pub fn style(&self, role: Role) -> Style {
        let style = self.coloured(role);
        match self.monochrome {
            true => Style::default().add_modifier(style.add_modifier),
            false => style,
        }
    }

    /// What a role draws with before monochrome has its say: the colour it carries, and the
    /// attributes that go with it.
    fn coloured(&self, role: Role) -> Style {
        match role {
            Role::Header | Role::SelectName => Style::default().add_modifier(Modifier::BOLD),
            Role::Text => Style::default().fg(self.ink(self.preset.text, Color::Gray)),
            Role::CurveSum => {
                let rgb = self.preset.emphasis.unwrap_or(self.preset.text);
                Style::default().fg(self.ink(rgb, Color::Gray))
            }
            Role::Good | Role::Connected | Role::ToggleOn | Role::MeterOk | Role::RateFast => {
                Style::default().fg(self.ink(self.preset.good, Color::Green))
            }
            Role::Warn | Role::MeterHot | Role::Unknown | Role::Notice => {
                Style::default().fg(self.ink(self.preset.warn, Color::Yellow))
            }
            // At 16 colours the warn slot is also the accent slot of a DAC, which every other
            // band's point draws in: white is the one colour no other role takes, so the
            // focused band still parts from the curve it sits on.
            Role::CurveSelected => match self.depth {
                ColorDepth::Ansi16 | ColorDepth::Auto => Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                depth => Style::default().fg(quantise(self.preset.warn, depth)),
            },
            // Monochrome has no colour for the warning, so the fill goes dim rather than reading
            // as a heavy fill under the curve.
            Role::CurveGap => {
                let style = Style::default().fg(self.ink(self.preset.warn, Color::Yellow));
                match self.monochrome {
                    true => style.add_modifier(Modifier::DIM),
                    false => style,
                }
            }
            Role::Alert | Role::MeterClip => {
                Style::default().fg(self.ink(self.preset.alert, Color::Red))
            }
            Role::Fault => {
                let style = Style::default().fg(self.ink(self.preset.alert, Color::Red));
                match self.alert_is_near_the_accent() {
                    true => style.add_modifier(Modifier::REVERSED),
                    false => style,
                }
            }
            Role::Dim | Role::Disconnected | Role::Stale | Role::Axis | Role::CurveTarget => {
                self.dim_style()
            }
            Role::Faint => Style::default().fg(self.ink(self.preset.faint, Color::DarkGray)),
            Role::Rule | Role::KeyHint => Style::default().fg(self.glow()),
            Role::Cursor | Role::CurvePoint | Role::ModeArrow => Style::default().fg(self.accent()),
            Role::Bar => Style::default().fg(self.emphasis()),
            Role::Selected => Style::default()
                .fg(self.emphasis())
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            // A title sits on its pane's contour. In the glow it travels along the spectrum with
            // the rule and the key hints, and a colour of its own keeps it off the shade the
            // contour breathes in; the emphasis, where the preset names one, stands still.
            Role::Title => {
                let colour = match self.preset.emphasis {
                    Some(_) => self.emphasis(),
                    None => self.glow(),
                };
                Style::default().fg(colour).add_modifier(Modifier::BOLD)
            }
            Role::Badge => Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED),
            Role::KeyDesc => self.dim_style(),
        }
    }

    /// Whether the accent the screen draws in comes near enough to the preset's alert colour
    /// that a fault drawn in alert alone would read as just one more accented row.
    ///
    /// At 16 colours "near" means the two slots' xterm colours are within [`ALERT_NEAR`] of each
    /// other (e.g. an amber preamplifier's accent and the fault red, or a `red` preamplifier's
    /// bright red beside it); above 16 colours it means the RGB distance is within it (true only
    /// of the `red` preset's DAC accent against its own alert colour).
    ///
    /// Always weighed against the resting accent, never the glow's current position: how a fault
    /// is drawn is a property of the palette, not something that should flicker reverse video on
    /// and off as the glow travels.
    fn alert_is_near_the_accent(&self) -> bool {
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => {
                let alert = palette::rgb_of(self.ink(self.preset.alert, Color::Red));
                palette::distance(palette::rgb_of(self.accent()), alert) < ALERT_NEAR
            }
            _ => palette::distance(self.rest_rgb(), self.preset.alert) < ALERT_NEAR,
        }
    }

    /// The glyph a filled cell of the volume bar's clipping-risk zone is drawn with: the medium
    /// shade `▓` where the accent is near the alert red, so the zone reads as a pattern beside
    /// the bar's solid `█` rather than as more of its accented cells, and `█` otherwise. In
    /// monochrome, where no colour tells the zone from the bar, it is always `▓`.
    pub fn zone_fill(&self) -> &'static str {
        match self.monochrome || self.alert_is_near_the_accent() {
            true => "▓",
            false => "█",
        }
    }

    /// How one of the preset's colours draws at this theme's depth: `slot`, the ANSI colour it
    /// means, at 16 colours — where a nearest-RGB reduction would land a muted colour on a
    /// grey rather than on the colour it stands for — and the reduction of `rgb` above that.
    fn ink(&self, rgb: Rgb, slot: Color) -> Color {
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => slot,
            depth => quantise(rgb, depth),
        }
    }

    /// The accent's colour at `t` along a ramp (0 at its first cell): the accent itself at 16
    /// colours, and a ramp from a darker accent to the full accent at 256 and true colour.
    pub fn accent_shade(&self, t: f64) -> Color {
        if self.depth == ColorDepth::Ansi16 {
            return self.accent();
        }
        let accent = self.accent_rgb();
        let dark = scale(accent, SHADE_DARK);
        quantise(lerp(dark, accent, t.clamp(0.0, 1.0)), self.depth)
    }

    /// A bar's colour at `t` (its filled cell's position over the bar's width, 0 at the first
    /// cell): [`Theme::accent_shade`]'s ramp, or the emphasis itself, end to end, where the
    /// preset names one — a bar of one colour, so a quiet meter's few cells read as brightly as
    /// a loud one's.
    pub fn bar_shade(&self, t: f64) -> Color {
        match self.preset.emphasis {
            Some(_) => self.emphasis(),
            None => self.accent_shade(t),
        }
    }

    /// What a bar's empty cells, and a track's ground, draw in on the focused row, so the bar
    /// reads as part of that row: the dark end of [`Theme::accent_shade`]'s ramp (or the
    /// emphasis darkened as far, where the preset names one) at 256 and true colour; the accent
    /// or emphasis with the dim attribute at 16 colours (no darker slot exists there); no colour
    /// at all in monochrome, where the glyphs alone tell filled from empty.
    pub fn bar_dim(&self) -> Style {
        if self.monochrome {
            return Style::default();
        }
        let colour = match (self.depth, self.preset.emphasis) {
            (ColorDepth::Ansi16 | ColorDepth::Auto, _) => {
                return Style::default()
                    .fg(self.emphasis())
                    .add_modifier(Modifier::DIM);
            }
            (depth, Some(rgb)) => quantise(scale(rgb, SHADE_DARK), depth),
            (_, None) => self.accent_shade(0.0),
        };
        Style::default().fg(colour)
    }

    /// What a level bar's empty cells past the step a boosting tone curve can clip from are
    /// drawn in: the alert colour dimmed — as far as [`Theme::accent_shade`]'s ramp darkens, at
    /// 256 and true colour, the alert slot dimmed at 16 colours — and no colour at all in
    /// monochrome, where the glyphs alone tell filled from empty.
    pub fn alert_ground(&self) -> Style {
        if self.monochrome {
            return Style::default();
        }
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => {
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
            }
            depth => Style::default().fg(quantise(scale(self.preset.alert, SHADE_DARK), depth)),
        }
    }

    /// A breathing contour's colour at `t`: [`Theme::accent_shade`]'s ramp taken over the
    /// colour the glow is at rather than the resting accent, so a decoration the meters have
    /// carried along the spectrum is shaded in that colour and not the one it left.
    ///
    /// With the glow at rest this is [`Theme::accent_shade`] exactly, and at 16 colours it is
    /// the one flat colour that shade is there too.
    pub fn glow_shade(&self, t: f64) -> Color {
        if self.depth == ColorDepth::Ansi16 {
            return self.glow();
        }
        let glow = self.glow_rgb();
        let dark = scale(glow, SHADE_DARK);
        quantise(lerp(dark, glow, t.clamp(0.0, 1.0)), self.depth)
    }

    /// A meter or curve's colour at `t` along `stops`, which spread evenly over `stops`:
    /// linear RGB interpolation at true colour, the nearest 256-colour cube index at 256, and
    /// the nearest stop by position at 16 (there is no room to blend, so a caller that wants a
    /// meter's three fixed bands at 16 colours, rather than this generic even split, draws
    /// them itself — see `widgets::meters::meter_lines`).
    pub fn gradient(&self, t: f64, stops: &[Color]) -> Color {
        let t = t.clamp(0.0, 1.0);
        match self.depth {
            ColorDepth::Ansi16 | ColorDepth::Auto => nearest_stop(t, stops),
            _ => quantise(interpolate(t, stops), self.depth),
        }
    }

    /// The tone curve's colour `along` the way from its lowest frequency (0) to its highest (1),
    /// carried `drift` degrees round by the rainbow scroll, muted or not, or `None` where this
    /// theme has no spectrum to draw in at all (see [`Theme::spectrum`]).
    ///
    /// On a preset with no curve stops of its own, the curve is [`SPECTRUM_ARC`] of the hue
    /// circle (red at the lowest frequency to violet at the highest), and drift turns the whole
    /// arc round the circle. On one with its own stops, the curve runs through those instead,
    /// and drift slides it along them and back once each way per turn, so it never jumps from
    /// one end to the other. Muted desaturates either one toward its own grey and a little
    /// darker ([`mute`]).
    pub fn curve_colour(&self, along: f32, drift: f32, muted: bool) -> Option<Color> {
        let Some(stops) = self.preset.curve else {
            let degrees = drift + SPECTRUM_ARC * along;
            return match muted {
                true => self.muted_spectrum(degrees),
                false => self.spectrum(degrees),
            };
        };
        if self.monochrome || matches!(self.depth, ColorDepth::Ansi16 | ColorDepth::Auto) {
            return None;
        }
        let place = f64::from(fold(along + drift / (HUE_CIRCLE / 2.0)));
        let rgb = blend(stops, place);
        let rgb = match muted {
            true => mute(rgb),
            false => rgb,
        };
        Some(quantise(rgb, self.depth))
    }

    /// The colour `degrees` round the hue circle draws in, or `None` where this theme has no
    /// spectrum to draw in at all.
    ///
    /// A spectrum needs more colours than the 16 ANSI slots hold — a hue lands on one of six
    /// of them, so a curve drawn this way would read as six blocks rather than as a run — and
    /// monochrome has none, so both answer `None` and the caller keeps the role it would have
    /// drawn in. At 256 colours the hue comes through the cube, and at true colour exactly.
    fn spectrum(&self, degrees: f32) -> Option<Color> {
        match self.depth {
            _ if self.monochrome => None,
            ColorDepth::Ansi16 | ColorDepth::Auto => None,
            depth => Some(quantise(
                hsl(degrees, SPECTRUM_SATURATION, SPECTRUM_LIGHTNESS),
                depth,
            )),
        }
    }

    /// [`Theme::spectrum`], muted: the same hue `degrees` round the circle names, desaturated
    /// toward its own luma grey and a little darker ([`mute`]), for a curve the bands are not
    /// applying rather than shaping — "less saturated" rather than the flat dim grey a fixed
    /// colour would draw. `None` wherever `spectrum` has none to draw in either.
    fn muted_spectrum(&self, degrees: f32) -> Option<Color> {
        match self.depth {
            _ if self.monochrome => None,
            ColorDepth::Ansi16 | ColorDepth::Auto => None,
            depth => Some(quantise(
                mute(hsl(degrees, SPECTRUM_SATURATION, SPECTRUM_LIGHTNESS)),
                depth,
            )),
        }
    }

    /// [`Dim`](Role::Dim) and the roles that hold a reading back: a plain dim attribute at 16
    /// colours and wherever the theme is monochrome, which has no colour to dim with, and the
    /// preset's own grey (drawn instead of dimming, which many terminals render no differently
    /// from the role's own colour) at 256 and true colour.
    fn dim_style(&self) -> Style {
        match self.depth {
            _ if self.monochrome => Style::default().add_modifier(Modifier::DIM),
            ColorDepth::Ansi16 | ColorDepth::Auto => Style::default().add_modifier(Modifier::DIM),
            depth => Style::default().fg(quantise(self.preset.dim, depth)),
        }
    }
}

/// Which of `stops` a spectrum rests on: the one nearest `accent` (the colour a DAC is drawn in —
/// the config's override for that family where it names one, else the palette's own). A
/// user-named spectrum rests on whichever of its colours is closest to the screen's accent.
fn resting_stop(stops: &[Rgb], accent: Rgb) -> usize {
    stops
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            palette::distance(**a, accent).total_cmp(&palette::distance(**b, accent))
        })
        .map_or(0, |(i, _)| i)
}

/// The colour hue `degrees` round the circle is at `saturation` and `lightness`, both
/// `0.0..=1.0`, by the usual HSL conversion.
fn hsl(degrees: f32, saturation: f32, lightness: f32) -> Rgb {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    // Six 60° sectors, each ramping one channel between the two the other channels hold.
    let sector = degrees.rem_euclid(HUE_CIRCLE) / 60.0;
    let ramp = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, ramp, 0.0),
        1 => (ramp, chroma, 0.0),
        2 => (0.0, chroma, ramp),
        3 => (0.0, ramp, chroma),
        4 => (ramp, 0.0, chroma),
        _ => (chroma, 0.0, ramp),
    };
    let floor = lightness - chroma / 2.0;
    let channel = |v: f32| ((v + floor) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(r), channel(g), channel(b))
}

/// Rec. 709 luma of `rgb`, on its encoded values: how bright it reads, 0 to 255.
fn luma(rgb: Rgb) -> f32 {
    0.2126 * f32::from(rgb.0) + 0.7152 * f32::from(rgb.1) + 0.0722 * f32::from(rgb.2)
}

/// `rgb` blended toward its own luma grey and a little darker, for a muted spectrum colour: hue
/// kept (blended against its own grey, never a fixed one), saturation cut to about a third
/// (`c' = L + (c - L) × `[`MUTE_SATURATION`]), then darkened by [`MUTE_DARKEN`], where `L` is
/// the channels' luma, `0.2126 r + 0.7152 g + 0.0722 b`.
fn mute(rgb: Rgb) -> Rgb {
    let luma = luma(rgb);
    let channel = |c: u8| {
        ((luma + (f32::from(c) - luma) * MUTE_SATURATION) * MUTE_DARKEN)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (channel(rgb.0), channel(rgb.1), channel(rgb.2))
}

/// `rgb` at `depth`: exact at true colour, the nearest 256-colour cube index at 256, and the
/// nearest of the 16 ANSI colours (by RGB distance against the xterm table) at 16.
fn quantise(rgb: Rgb, depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColorDepth::Ansi256 => palette::nearest_cube(rgb),
        ColorDepth::Ansi16 | ColorDepth::Auto => palette::nearest_ansi16(rgb),
    }
}

/// `stops`, sampled at `t` by linear interpolation between the two stops either side of it (`t`
/// spreads `stops` evenly over `0.0..=1.0`).
fn interpolate(t: f64, stops: &[Color]) -> Rgb {
    let stops: Vec<Rgb> = stops.iter().map(|stop| palette::rgb_of(*stop)).collect();
    blend(&stops, t)
}

/// [`interpolate`] over colours already in RGB.
fn blend(stops: &[Rgb], t: f64) -> Rgb {
    match stops {
        [] => (0, 0, 0),
        [only] => *only,
        _ => {
            let segments = stops.len() - 1;
            let scaled = t.clamp(0.0, 1.0) * segments as f64;
            let i = (scaled.floor() as usize).min(segments - 1);
            let local = scaled - i as f64;
            lerp(stops[i], stops[i + 1], local)
        }
    }
}

/// `t` folded into `0.0..=1.0` the way a light runs along a track and back: rising from 0 to 1,
/// falling back to 0 by 2, and again from there, so a place that keeps moving never jumps.
fn fold(t: f32) -> f32 {
    let t = t.rem_euclid(2.0);
    match t > 1.0 {
        true => 2.0 - t,
        false => t,
    }
}

/// The stop nearest `t`, for a colour depth with no room to blend: `stops` spread evenly over
/// `0.0..=1.0`, so the nearest of `n` stops to `t` is stop `round(t * (n - 1))`. Two stops
/// switch at their midpoint, `0.5`; a caller after a different split (the meter's fixed
/// bands, say) draws it itself rather than asking a gradient for one.
fn nearest_stop(t: f64, stops: &[Color]) -> Color {
    match stops {
        [] => Color::Reset,
        [only] => *only,
        _ => {
            let idx = (t * (stops.len() - 1) as f64).round() as usize;
            stops[idx.min(stops.len() - 1)]
        }
    }
}

/// `a` scaled by `factor` (clamped to a byte), channel by channel.
fn scale(rgb: Rgb, factor: f64) -> Rgb {
    let channel = |c: u8| (f64::from(c) * factor).round().clamp(0.0, 255.0) as u8;
    (channel(rgb.0), channel(rgb.1), channel(rgb.2))
}

/// The RGB `t` of the way from `a` to `b`, channel by channel.
fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let channel = |from: u8, to: u8| {
        (f64::from(from) + (f64::from(to) - f64::from(from)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

/// Whether `rgb` reads as blue or cyan: a hue from 170° to 260° round the circle at an HSL
/// saturation above a quarter, which the `red` palette never draws in.
#[cfg(test)]
pub(crate) fn bluish(rgb: Rgb) -> bool {
    let (r, g, b) = (
        f32::from(rgb.0) / 255.0,
        f32::from(rgb.1) / 255.0,
        f32::from(rgb.2) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    if chroma == 0.0 {
        return false;
    }
    let sector = match max {
        m if m == r => ((g - b) / chroma).rem_euclid(6.0),
        m if m == g => (b - r) / chroma + 2.0,
        _ => (r - g) / chroma + 4.0,
    };
    let lightness = (max + min) / 2.0;
    let saturation = chroma / (1.0 - (2.0 * lightness - 1.0).abs());
    (170.0..=260.0).contains(&(sector * 60.0)) && saturation > 0.25
}

#[cfg(test)]
mod tests;
