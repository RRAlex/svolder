// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[tui]` table as the configuration holds it: its fields and the enums they take.

use super::*;

/// How `svolder tui` looks and behaves.
///
/// Every field mirrors a key under `[tui]` in `config.toml`, with the defaults shown there.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TuiConfig {
    /// One accent colour for every device family, overriding the palette's three: one of the
    /// 16 ANSI colour names (its `bright-` form too) or a `#rrggbb` value. `None` when the
    /// file names none, leaving the palette's own accents in place and the key out of
    /// anything written from this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    /// The palette preset the screens draw from: `amber`, `ice`, `red` or `neon`.
    pub palette: String,
    /// Accent overrides for one device family at a time, which `accent` overrides in turn.
    pub accents: FamilyAccents,
    /// The stops the glow travels, cool end to hot end, from `stops` under `[tui.spectrum]`,
    /// each a `#rrggbb` value. Empty when the file names none, leaving the palette's own stops
    /// in place and the key out of anything written from this.
    ///
    /// Serialises as the table the file carries it in — `"spectrum": { "stops": [...] }` —
    /// rather than the bare array it is held as, so a rendered configuration keeps the file's
    /// shape, the way `accents` does.
    #[serde(skip_serializing_if = "Vec::is_empty", serialize_with = "stops_table")]
    pub spectrum: Vec<String>,
    /// The colour depth to render at.
    pub colors: ColorDepth,
    /// Force monochrome rendering (selection by reverse video only), regardless of `colors`.
    pub monochrome: bool,
    /// Whether each palette paints its own dark greys under the screen, its panes and its boxes,
    /// at true and 256 colours; off leaves the terminal's own background showing.
    pub theme_background: bool,
    /// Whether mouse capture starts on.
    pub mouse: bool,
    /// Whether a pointer resting on a target shows a one-line tip.
    pub tooltips: bool,
    /// Whether the wordmark intro plays while the device opens.
    pub intro: bool,
    /// Whether a short sound plays with the wordmark intro, through PipeWire's `pw-play`.
    pub intro_sound: bool,
    /// Which effect builds the wordmark intro.
    pub intro_effect: IntroEffect,
    /// How one screen gives way to the next.
    pub transition: Transition,
    /// Whether the terminal's title carries one bar per channel of the device's meters while
    /// it streams them, else of what the host plays while its capture runs.
    pub title_meter: bool,
    /// Whether `q` asks before `svolder tui` quits.
    pub confirm_quit: bool,
    /// Whether `svolder tui` reaches the host's own audio: what its media player plays, the
    /// volume of its default sink and the player's transport. `SVOLDER_HOST=off` in the
    /// environment turns it off for one process whatever this says.
    pub host_audio: bool,
    /// Whether the device screen draws a spectrum of what the host's sink plays, while host
    /// audio is on.
    pub visualizer: bool,
    /// Whether the Now playing row shows the track's cover art, and drawn how the terminal
    /// allows.
    pub cover_art: CoverArt,
    /// Whether the screens draw their graphs: the Telemetry and level history, and the
    /// connection strip. Off draws every screen without them.
    pub graphs: bool,
    /// How much history the Telemetry and level graphs span.
    pub graph_span: GraphSpan,
    /// How much history the connection strip spans.
    pub link_span: LinkSpan,
    /// The share of the device screen's width the Controls pane takes of the two panes' room,
    /// from `device_split` under `[tui]`: `0.5` splits it evenly. The screen holds each pane to at
    /// least 40 columns, so a value that leaves either less is read as the nearest split that
    /// does not, and a terminal too narrow for two panes of 40 has no split to move.
    pub device_split: f64,
    /// Which of the animations run: `effects = true|false` for all of them at once, or an
    /// `effects = { ... }` table naming them one at a time.
    pub effects: EffectsConfig,
}

/// How `svolder tui` draws one screen giving way to the next, from `transition` under `[tui]`.
///
/// Every one of them but [`Transition::None`] is part of the animation layer, so `effects`
/// switched off plays none of them whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    /// The new screen is simply there.
    None,
    /// A pane's frame traces itself once round when it first appears, its content sweeping in
    /// behind it.
    #[default]
    Trace,
    /// The new screen shows through a field of shade glyphs that thins to nothing.
    Noise,
    /// The old screen's columns slide down off the bottom, revealing the new one beneath.
    Melt,
}

impl Transition {
    /// Every transition, in the order the configuration error and the Settings box list
    /// them.
    pub const ALL: [Transition; 4] = [
        Transition::None,
        Transition::Trace,
        Transition::Noise,
        Transition::Melt,
    ];

    /// What `transition` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            Transition::None => "none",
            Transition::Trace => "trace",
            Transition::Noise => "noise",
            Transition::Melt => "melt",
        }
    }

    /// The transition `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<Transition> {
        Transition::ALL
            .into_iter()
            .find(|transition| transition.name() == word)
    }
}

/// Which effect builds the wordmark as `svolder tui` starts, from `intro_effect` under `[tui]`.
///
/// Every one of them but [`IntroEffect::Fade`] is a demo effect. Like the fade, a demo effect
/// is drawn only where the animation layer runs; elsewhere the mark is drawn static whatever
/// this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntroEffect {
    /// One of the demo effects, picked afresh each launch; never the fade.
    #[default]
    Random,
    /// The mark fades in and holds.
    Fade,
    /// Sparks thrown in from the edges build the mark.
    Particles,
}

impl IntroEffect {
    /// Every effect, in the order the configuration error and the Settings box list them.
    pub const ALL: [IntroEffect; 3] = [
        IntroEffect::Random,
        IntroEffect::Fade,
        IntroEffect::Particles,
    ];

    /// What `intro_effect` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            IntroEffect::Random => "random",
            IntroEffect::Fade => "fade",
            IntroEffect::Particles => "particles",
        }
    }

    /// The effect `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<IntroEffect> {
        IntroEffect::ALL
            .into_iter()
            .find(|effect| effect.name() == word)
    }
}

/// Whether `svolder tui` draws the cover art of the track playing, from `cover_art` under
/// `[tui]`. There is no way to force a graphics protocol: the terminal is asked what it draws,
/// and one that does not answer gets half-block cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverArt {
    /// Draw the art as the terminal allows: the kitty graphics protocol, else sixel, else
    /// half-block cells.
    #[default]
    Auto,
    /// Draw no art, and ask the terminal nothing about graphics.
    Off,
}

impl CoverArt {
    /// Every setting, in the order the configuration error and the Settings box list them.
    pub const ALL: [CoverArt; 2] = [CoverArt::Auto, CoverArt::Off];

    /// What `cover_art` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            CoverArt::Auto => "auto",
            CoverArt::Off => "off",
        }
    }

    /// The setting `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<CoverArt> {
        CoverArt::ALL.into_iter().find(|art| art.name() == word)
    }
}

/// A span of history for the Telemetry and level graphs, from `graph_span` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum GraphSpan {
    /// 1 minute.
    #[serde(rename = "1m")]
    M1,
    /// 2 minutes.
    #[serde(rename = "2m")]
    M2,
    /// 3 minutes.
    #[serde(rename = "3m")]
    M3,
    /// 5 minutes.
    #[default]
    #[serde(rename = "5m")]
    M5,
    /// 10 minutes.
    #[serde(rename = "10m")]
    M10,
    /// 15 minutes.
    #[serde(rename = "15m")]
    M15,
    /// 30 minutes.
    #[serde(rename = "30m")]
    M30,
    /// 60 minutes.
    #[serde(rename = "60m")]
    M60,
}

impl GraphSpan {
    /// Every span, shortest first, in the order the configuration error and the Settings box
    /// list them.
    pub const ALL: [GraphSpan; 8] = [
        GraphSpan::M1,
        GraphSpan::M2,
        GraphSpan::M3,
        GraphSpan::M5,
        GraphSpan::M10,
        GraphSpan::M15,
        GraphSpan::M30,
        GraphSpan::M60,
    ];

    /// What `graph_span` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            GraphSpan::M1 => "1m",
            GraphSpan::M2 => "2m",
            GraphSpan::M3 => "3m",
            GraphSpan::M5 => "5m",
            GraphSpan::M10 => "10m",
            GraphSpan::M15 => "15m",
            GraphSpan::M30 => "30m",
            GraphSpan::M60 => "60m",
        }
    }

    /// Every name, joined as prose: `1m, 2m or 3m`.
    pub fn list() -> String {
        let names: Vec<&str> = GraphSpan::ALL.iter().map(|span| span.name()).collect();
        match names.split_last() {
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        }
    }

    /// The span `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<GraphSpan> {
        GraphSpan::ALL.into_iter().find(|span| span.name() == word)
    }

    /// How long the span is.
    pub fn duration(self) -> std::time::Duration {
        let minutes = match self {
            GraphSpan::M1 => 1,
            GraphSpan::M2 => 2,
            GraphSpan::M3 => 3,
            GraphSpan::M5 => 5,
            GraphSpan::M10 => 10,
            GraphSpan::M15 => 15,
            GraphSpan::M30 => 30,
            GraphSpan::M60 => 60,
        };
        std::time::Duration::from_secs(minutes * 60)
    }
}

/// A span of history for the connection strip, from `link_span` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum LinkSpan {
    /// 1 hour.
    #[serde(rename = "1h")]
    H1,
    /// 3 hours.
    #[serde(rename = "3h")]
    H3,
    /// 6 hours.
    #[serde(rename = "6h")]
    H6,
    /// 12 hours.
    #[default]
    #[serde(rename = "12h")]
    H12,
    /// 24 hours.
    #[serde(rename = "24h")]
    H24,
}

impl LinkSpan {
    /// Every span, shortest first, in the order the configuration error and the Settings box
    /// list them.
    pub const ALL: [LinkSpan; 5] = [
        LinkSpan::H1,
        LinkSpan::H3,
        LinkSpan::H6,
        LinkSpan::H12,
        LinkSpan::H24,
    ];

    /// What `link_span` under `[tui]` writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            LinkSpan::H1 => "1h",
            LinkSpan::H3 => "3h",
            LinkSpan::H6 => "6h",
            LinkSpan::H12 => "12h",
            LinkSpan::H24 => "24h",
        }
    }

    /// Every name, joined as prose: `1h, 3h or 6h`.
    pub fn list() -> String {
        let names: Vec<&str> = LinkSpan::ALL.iter().map(|span| span.name()).collect();
        match names.split_last() {
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        }
    }

    /// The span `word` names, or `None` for a word that names none of them.
    pub fn named(word: &str) -> Option<LinkSpan> {
        LinkSpan::ALL.into_iter().find(|span| span.name() == word)
    }

    /// How long the span is.
    pub fn duration(self) -> std::time::Duration {
        let hours = match self {
            LinkSpan::H1 => 1,
            LinkSpan::H3 => 3,
            LinkSpan::H6 => 6,
            LinkSpan::H12 => 12,
            LinkSpan::H24 => 24,
        };
        std::time::Duration::from_secs(hours * 3600)
    }
}

/// The `effects` key under `[tui]`: the animation layer as a whole, and one switch per
/// animation.
///
/// `effects = false` turns every one of them off and `effects = true` is these defaults; a
/// table starts from the defaults and overrides only the keys it names, `enabled` among them.
///
/// Written back in the same three shapes it is read in, by this type's own
/// [`Serialize`](serde::Serialize) implementation, so a rendered config keeps the file's shape
/// rather than becoming a table where the file said one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectsConfig {
    /// Whether the animation layer runs at all.
    pub enabled: bool,
    /// Whether a changed value's bar glides to its new length and its row glows.
    pub eased: bool,
    /// Whether the row the cursor left keeps a fading trail.
    pub glow: bool,
    /// Whether a highlight sweeps the row you change, and the selected row while you rest.
    pub shimmer: bool,
    /// Whether panel contours pulse with the level: the amp's meters, else what the host plays.
    pub breathing: bool,
    /// Whether a light runs once round the frame the keys move or arrive to. The key keeps
    /// its name, so a file that switches it off goes on doing so; how a pane arrives on the
    /// screen is [`TuiConfig::transition`]'s.
    pub frames: bool,
    /// Whether a standing fault's row and the header border pulse.
    pub fault_presence: bool,
    /// Whether a marker lingers at a meter's recent peak.
    pub peak_hold: bool,
    /// Whether the decoration drifts along the palette's spectrum, glowing hotter as the level
    /// rises; off by default.
    pub hue_drift: bool,
    /// Whether the tone curve's colours scroll round the hue circle (along the palette's own ramp
    /// on `red` and `neon`) rather than resting red at the lowest frequency and violet at the
    /// highest (or on that ramp); off by default.
    pub spectrum_drift: bool,
    /// Whether the rows of a list dim with their distance from the selected row.
    pub list_fade: bool,
    /// Whether Ratatoskr, the squirrel, may come out; it stays hidden until it is called.
    pub mascot: bool,
}

impl Default for EffectsConfig {
    /// The animation layer on with every animation running, and the hue drift and the
    /// spectrum's own scroll — both modes rather than animations — off.
    fn default() -> Self {
        EffectsConfig {
            enabled: true,
            eased: true,
            glow: true,
            shimmer: true,
            breathing: true,
            frames: true,
            fault_presence: true,
            peak_hold: true,
            hue_drift: false,
            spectrum_drift: false,
            list_fade: true,
            mascot: true,
        }
    }
}

impl serde::Serialize for EffectsConfig {
    /// Write the shape the file writes: `true` for the defaults, `false` for everything off,
    /// and the whole table for anything else, exactly as [`Config::parse`] reads it back.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;

        if *self == EffectsConfig::default() {
            return serializer.serialize_bool(true);
        }
        if *self == EffectsConfig::off() {
            return serializer.serialize_bool(false);
        }
        let mut table = serializer.serialize_struct("EffectsConfig", 12)?;
        table.serialize_field("enabled", &self.enabled)?;
        table.serialize_field("eased", &self.eased)?;
        table.serialize_field("glow", &self.glow)?;
        table.serialize_field("shimmer", &self.shimmer)?;
        table.serialize_field("breathing", &self.breathing)?;
        table.serialize_field("frames", &self.frames)?;
        table.serialize_field("fault_presence", &self.fault_presence)?;
        table.serialize_field("peak_hold", &self.peak_hold)?;
        table.serialize_field("hue_drift", &self.hue_drift)?;
        table.serialize_field("spectrum_drift", &self.spectrum_drift)?;
        table.serialize_field("list_fade", &self.list_fade)?;
        table.serialize_field("mascot", &self.mascot)?;
        table.end()
    }
}

impl EffectsConfig {
    /// Every switch off, as `effects = false` means.
    pub fn off() -> EffectsConfig {
        EffectsConfig {
            enabled: false,
            eased: false,
            glow: false,
            shimmer: false,
            breathing: false,
            frames: false,
            fault_presence: false,
            peak_hold: false,
            hue_drift: false,
            spectrum_drift: false,
            list_fade: false,
            mascot: false,
        }
    }
}

/// The `[tui.accents]` table: an accent colour per device family, each in the same form as
/// `accent`, and each `None` when the file names none.
///
/// A family that overrides nothing is left out of what this serialises to, the way `accent`
/// is: a rendered configuration names the keys a hand-written file would name and no others.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct FamilyAccents {
    /// The accent a DAC draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dac: Option<String>,
    /// The accent an amplifier draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amp: Option<String>,
    /// The accent a preamplifier draws with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preamp: Option<String>,
}

impl FamilyAccents {
    /// The accent `family` overrides its palette colour with, if any.
    pub fn get(&self, family: Family) -> Option<&str> {
        match family {
            Family::Dac => self.dac.as_deref(),
            Family::Amp => self.amp.as_deref(),
            Family::Preamp => self.preamp.as_deref(),
        }
    }

    /// Set (or, with `None`, clear) the accent of `family`.
    pub fn set(&mut self, family: Family, accent: Option<String>) {
        match family {
            Family::Dac => self.dac = accent,
            Family::Amp => self.amp = accent,
            Family::Preamp => self.preamp = accent,
        }
    }
}

/// The device screen's split when the file names none: the two panes share the width evenly.
const DEFAULT_DEVICE_SPLIT: f64 = 0.5;

impl Default for TuiConfig {
    /// The `amber` palette with no accent overridden and no spectrum of its own, auto colour
    /// depth, not monochrome, the trace between screens, and the theme background, mouse, intro,
    /// title meter, the quit question, host audio, the visualizer and effects all on.
    fn default() -> Self {
        TuiConfig {
            accent: None,
            palette: DEFAULT_PALETTE.into(),
            accents: FamilyAccents::default(),
            spectrum: Vec::new(),
            colors: ColorDepth::Auto,
            monochrome: false,
            theme_background: true,
            mouse: true,
            tooltips: true,
            intro: true,
            intro_sound: false,
            intro_effect: IntroEffect::default(),
            transition: Transition::default(),
            title_meter: true,
            confirm_quit: true,
            host_audio: true,
            visualizer: true,
            cover_art: CoverArt::default(),
            graphs: true,
            graph_span: GraphSpan::default(),
            link_span: LinkSpan::default(),
            device_split: DEFAULT_DEVICE_SPLIT,
            effects: EffectsConfig::default(),
        }
    }
}

/// The colour depth `svolder tui` renders at, from `colors` under `[tui]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorDepth {
    /// Detect the depth from `COLORTERM`/`TERM`.
    #[default]
    Auto,
    /// Force the 16 ANSI colours.
    #[serde(rename = "16")]
    Ansi16,
    /// Force the 256-colour palette.
    #[serde(rename = "256")]
    Ansi256,
    /// Force 24-bit truecolor.
    #[serde(rename = "true")]
    TrueColor,
}

impl ColorDepth {
    /// What `colors` under `[tui]` writes this as, which is the name it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            ColorDepth::Auto => "auto",
            ColorDepth::Ansi16 => "16",
            ColorDepth::Ansi256 => "256",
            ColorDepth::TrueColor => "true",
        }
    }
}

/// Write `stops` as the `[tui.spectrum]` table itself rather than as the array it is held as:
/// `{ "stops": [...] }`, the shape the file spells and the shape a reader of `config show
/// --json` can hand back to the file.
fn stops_table<S: serde::Serializer>(stops: &[String], out: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;

    let mut table = out.serialize_struct("Spectrum", 1)?;
    table.serialize_field("stops", stops)?;
    table.end()
}
