// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[tui]` table: its on-disk shapes, [`tui`] that checks them, and the checks on its keys.

use super::super::{
    ColorDepth, ConfigError, CoverArt, EffectsConfig, FamilyAccents, GraphSpan, IntroEffect,
    LinkSpan, Transition, TuiConfig,
};
use super::is_named;

/// The on-disk shape of the `[tui]` table, before validation.
#[derive(serde::Deserialize)]
#[serde(default)]
pub(super) struct RawTui {
    accent: Option<String>,
    palette: String,
    accents: RawAccents,
    spectrum: RawSpectrum,
    colors: ColorDepth,
    monochrome: bool,
    theme_background: bool,
    mouse: bool,
    tooltips: bool,
    intro: bool,
    intro_sound: bool,
    transition: String,
    intro_effect: String,
    title_meter: bool,
    confirm_quit: bool,
    host_audio: bool,
    visualizer: bool,
    cover_art: String,
    graphs: bool,
    graph_span: String,
    link_span: String,
    vu_reference: i64,
    device_split: f64,
    effects: RawEffects,
}

impl Default for RawTui {
    /// The same defaults [`TuiConfig::default`] names, read from it directly rather than
    /// repeated here: no accent override, auto colour depth, not monochrome, the trace
    /// between screens, and mouse, intro, title meter, the quit question, host audio and the
    /// visualizer all on. `accents` and `spectrum` have no on-disk default of their own
    /// beyond "empty", and `effects` collapses every enabled animation to the one switch a
    /// file would write, so the three keep their own literals.
    fn default() -> Self {
        let TuiConfig {
            accent,
            palette,
            accents: _,
            spectrum: _,
            colors,
            monochrome,
            theme_background,
            mouse,
            tooltips,
            intro,
            intro_sound,
            transition,
            intro_effect,
            title_meter,
            confirm_quit,
            host_audio,
            visualizer,
            cover_art,
            graphs,
            graph_span,
            link_span,
            vu_reference,
            device_split,
            effects: _,
        } = TuiConfig::default();
        RawTui {
            accent,
            palette,
            accents: RawAccents::default(),
            spectrum: RawSpectrum::default(),
            colors,
            monochrome,
            theme_background,
            mouse,
            tooltips,
            intro,
            intro_sound,
            transition: transition.name().into(),
            intro_effect: intro_effect.name().into(),
            title_meter,
            confirm_quit,
            host_audio,
            visualizer,
            cover_art: cover_art.name().into(),
            graphs,
            graph_span: graph_span.name().into(),
            link_span: link_span.name().into(),
            vu_reference: i64::from(vu_reference),
            device_split,
            effects: RawEffects::Switch(true),
        }
    }
}

/// The on-disk shape of `effects` under `[tui]`: the one switch every older file writes, or
/// the table that names the animations one at a time.
enum RawEffects {
    /// `effects = true|false`: every animation at once.
    Switch(bool),
    /// `effects = { eased = false, ... }`: the defaults with these keys overridden.
    Table(RawEffectsTable),
}

/// The keys an `effects` table takes, in the order the error message lists them.
pub(in super::super) const EFFECT_KEYS: [&str; 12] = [
    "enabled",
    "eased",
    "glow",
    "shimmer",
    "breathing",
    "frames",
    "fault_presence",
    "peak_hold",
    "hue_drift",
    "spectrum_drift",
    "list_fade",
    "mascot",
];

impl<'de> serde::Deserialize<'de> for RawEffects {
    /// A bool, or a table whose keys are all among [`EFFECT_KEYS`] and whose values are all
    /// bools; anything else is an error naming the key or the shape.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RawEffects;

            fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                out.write_str("tui.effects to be true, false or a table")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<RawEffects, E> {
                Ok(RawEffects::Switch(value))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<RawEffects, A::Error> {
                use serde::de::Error;

                let mut table = RawEffectsTable::default();
                while let Some(key) = map.next_key::<String>()? {
                    let slot = match key.as_str() {
                        "enabled" => &mut table.enabled,
                        "eased" => &mut table.eased,
                        "glow" => &mut table.glow,
                        "shimmer" => &mut table.shimmer,
                        "breathing" => &mut table.breathing,
                        "frames" => &mut table.frames,
                        "fault_presence" => &mut table.fault_presence,
                        "peak_hold" => &mut table.peak_hold,
                        "hue_drift" => &mut table.hue_drift,
                        "spectrum_drift" => &mut table.spectrum_drift,
                        "list_fade" => &mut table.list_fade,
                        "mascot" => &mut table.mascot,
                        _ => {
                            return Err(A::Error::custom(format!(
                                "tui.effects: unknown key {key:?}; the keys are {}",
                                EFFECT_KEYS.join(", ")
                            )));
                        }
                    };
                    *slot = map.next_value::<bool>().map_err(|_| {
                        A::Error::custom(format!("tui.effects.{key} must be true or false"))
                    })?;
                }
                Ok(RawEffects::Table(table))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// The on-disk shape of an `effects` table, every key of it optional.
struct RawEffectsTable {
    enabled: bool,
    eased: bool,
    glow: bool,
    shimmer: bool,
    breathing: bool,
    frames: bool,
    fault_presence: bool,
    peak_hold: bool,
    hue_drift: bool,
    spectrum_drift: bool,
    list_fade: bool,
    mascot: bool,
}

impl Default for RawEffectsTable {
    /// The defaults a table starts from, which are [`EffectsConfig::default`]'s own.
    fn default() -> Self {
        RawEffectsTable::from(EffectsConfig::default())
    }
}

impl From<EffectsConfig> for RawEffectsTable {
    fn from(effects: EffectsConfig) -> Self {
        RawEffectsTable {
            enabled: effects.enabled,
            eased: effects.eased,
            glow: effects.glow,
            shimmer: effects.shimmer,
            breathing: effects.breathing,
            frames: effects.frames,
            fault_presence: effects.fault_presence,
            peak_hold: effects.peak_hold,
            hue_drift: effects.hue_drift,
            spectrum_drift: effects.spectrum_drift,
            list_fade: effects.list_fade,
            mascot: effects.mascot,
        }
    }
}

impl From<RawEffects> for EffectsConfig {
    fn from(raw: RawEffects) -> Self {
        match raw {
            RawEffects::Switch(true) => EffectsConfig::default(),
            RawEffects::Switch(false) => EffectsConfig::off(),
            RawEffects::Table(table) => EffectsConfig {
                enabled: table.enabled,
                eased: table.eased,
                glow: table.glow,
                shimmer: table.shimmer,
                breathing: table.breathing,
                frames: table.frames,
                fault_presence: table.fault_presence,
                peak_hold: table.peak_hold,
                hue_drift: table.hue_drift,
                spectrum_drift: table.spectrum_drift,
                list_fade: table.list_fade,
                mascot: table.mascot,
            },
        }
    }
}

/// The on-disk shape of the `[tui.accents]` table, before validation.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RawAccents {
    dac: Option<String>,
    amp: Option<String>,
    preamp: Option<String>,
}

/// The on-disk shape of the `[tui.spectrum]` table, before validation.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct RawSpectrum {
    stops: Vec<String>,
}

/// The `[tui]` table, with `[tui.accents]`, `[tui.spectrum]` and `effects`.
pub(super) fn tui(raw: RawTui) -> Result<TuiConfig, ConfigError> {
    let accent = raw.accent.filter(|value| is_named(value));
    let accents = FamilyAccents {
        dac: raw.accents.dac.filter(|value| is_named(value)),
        amp: raw.accents.amp.filter(|value| is_named(value)),
        preamp: raw.accents.preamp.filter(|value| is_named(value)),
    };
    validate_palette(&raw.palette)?;
    let transition = parse_transition(&raw.transition)?;
    let intro_effect = parse_intro_effect(&raw.intro_effect)?;
    let cover_art = parse_cover_art(&raw.cover_art)?;
    let graph_span = parse_graph_span(&raw.graph_span)?;
    let link_span = parse_link_span(&raw.link_span)?;
    validate_split(raw.device_split)?;
    for accent in [&accent, &accents.dac, &accents.amp, &accents.preamp]
        .into_iter()
        .flatten()
    {
        validate_accent(accent)?;
    }
    for stop in &raw.spectrum.stops {
        validate_stop(stop)?;
    }
    let RawTui {
        accent: _,
        palette,
        accents: _,
        spectrum,
        colors,
        monochrome,
        theme_background,
        mouse,
        tooltips,
        intro,
        intro_sound,
        transition: _,
        intro_effect: _,
        title_meter,
        confirm_quit,
        host_audio,
        visualizer,
        cover_art: _,
        graphs,
        graph_span: _,
        link_span: _,
        vu_reference,
        device_split,
        effects,
    } = raw;
    Ok(TuiConfig {
        accent,
        palette,
        accents,
        spectrum: spectrum.stops,
        colors,
        monochrome,
        theme_background,
        mouse,
        tooltips,
        intro,
        intro_sound,
        transition,
        intro_effect,
        title_meter,
        confirm_quit,
        host_audio,
        visualizer,
        cover_art,
        graphs,
        graph_span,
        link_span,
        vu_reference: super::super::tui_settings::held_reference(vu_reference),
        device_split,
        effects: effects.into(),
    })
}

/// The palette presets `svolder tui` ships, in the order the error lists them.
pub(in super::super) const PALETTES: [&str; 4] = ["amber", "ice", "red", "neon"];

/// The palette a config that names none draws from.
pub(in super::super) const DEFAULT_PALETTE: &str = "amber";

/// A `[tui]` `palette` names one of the presets that ship.
fn validate_palette(palette: &str) -> Result<(), ConfigError> {
    if PALETTES.contains(&palette) {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "tui.palette must be one of {}",
        PALETTES.join(", ")
    )))
}

/// A `[tui]` `transition` names one of the four ways a screen gives way to the next.
fn parse_transition(word: &str) -> Result<Transition, ConfigError> {
    Transition::named(word).ok_or_else(|| {
        let words: Vec<&str> = Transition::ALL.iter().map(|t| t.name()).collect();
        ConfigError::Parse(format!(
            "tui.transition must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `cover_art` names one of the two ways the cover art is shown.
fn parse_cover_art(word: &str) -> Result<CoverArt, ConfigError> {
    CoverArt::named(word).ok_or_else(|| {
        let words: Vec<&str> = CoverArt::ALL.iter().map(|art| art.name()).collect();
        ConfigError::Parse(format!("tui.cover_art must be one of {}", words.join(", ")))
    })
}

/// A `[tui]` `intro_effect` names one of the effects that build the wordmark.
fn parse_intro_effect(word: &str) -> Result<IntroEffect, ConfigError> {
    IntroEffect::named(word).ok_or_else(|| {
        let words: Vec<&str> = IntroEffect::ALL.iter().map(|e| e.name()).collect();
        ConfigError::Parse(format!(
            "tui.intro_effect must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `graph_span` names one of the fixed spans of the Telemetry and level graphs.
fn parse_graph_span(word: &str) -> Result<GraphSpan, ConfigError> {
    GraphSpan::named(word).ok_or_else(|| {
        let words: Vec<&str> = GraphSpan::ALL.iter().map(|span| span.name()).collect();
        ConfigError::Parse(format!(
            "tui.graph_span must be one of {}",
            words.join(", ")
        ))
    })
}

/// A `[tui]` `device_split` is a share: a number from 0 to 1.
fn validate_split(split: f64) -> Result<(), ConfigError> {
    if (0.0..=1.0).contains(&split) {
        return Ok(());
    }
    Err(ConfigError::Parse(
        "tui.device_split must be a number from 0 to 1".into(),
    ))
}

/// A `[tui]` `link_span` names one of the fixed spans of the connection strip.
fn parse_link_span(word: &str) -> Result<LinkSpan, ConfigError> {
    LinkSpan::named(word).ok_or_else(|| {
        let words: Vec<&str> = LinkSpan::ALL.iter().map(|span| span.name()).collect();
        ConfigError::Parse(format!("tui.link_span must be one of {}", words.join(", ")))
    })
}

/// The 16 ANSI colour names, lowercase, without a `bright-` prefix.
const ANSI_COLOUR_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// A `[tui]` `accent` is one of the 16 ANSI colour names (case-insensitive, with an optional
/// `bright-` prefix) or a `#rrggbb` value.
fn validate_accent(accent: &str) -> Result<(), ConfigError> {
    let lower = accent.to_ascii_lowercase();
    let name = lower.strip_prefix("bright-").unwrap_or(&lower);
    if ANSI_COLOUR_NAMES.contains(&name) {
        return Ok(());
    }
    if let Some(hex) = accent.strip_prefix('#')
        && hex.len() == 6
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "accent {accent:?} is not an ANSI colour name or #rrggbb"
    )))
}

/// A `[tui.spectrum]` stop is a `#rrggbb` value: the stops are blended into one another, and
/// an ANSI colour name is a terminal slot rather than a colour there is anything to blend
/// from.
fn validate_stop(stop: &str) -> Result<(), ConfigError> {
    if let Some(hex) = stop.strip_prefix('#')
        && hex.len() == 6
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(());
    }
    Err(ConfigError::Parse(format!(
        "tui.spectrum stop {stop:?} is not a #rrggbb colour"
    )))
}
