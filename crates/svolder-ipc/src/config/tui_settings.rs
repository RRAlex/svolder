// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plain keys of the `[tui]` table, one entry each.
//!
//! An entry holds everything the key needs besides its field: its name in the file, what the
//! configuration template says of it, and how the Settings box labels and explains it. The
//! writer ([`Config::update_tui`](super::Config::update_tui)) and the template
//! ([`Config::template`](super::Config::template)) walk [`TUI_SETTINGS`] in the template's order,
//! so a key written here is written, commented and read back the same way everywhere. `effects`,
//! `accents` and `spectrum` nest, have shapes of their own and keep their own writers.
//!
//! # Adding a plain `[tui]` key
//!
//! 1. `config.rs`: the field of [`TuiConfig`] and its default.
//! 2. `config/parse.rs`: the field of the raw table and its default and conversion lines, which
//!    use field-init shorthand, so the compiler rejects a key that is routed nowhere.
//! 3. This file: a `pub static` entry and its place in [`TUI_SETTINGS`], where the template shows
//!    it.
//! 4. `settings.rs` in the TUI: the row's place in `CATEGORIES`.
//!
//! The table tests in `tui_settings/tests.rs` walk [`TUI_SETTINGS`] and fail for a key that skips
//! any part of that path. The compiler and the tests also ask for the re-export list in
//! `config.rs`, the order list in `the_table_lists_the_keys_in_template_order`, and `rows.golden`
//! when the key gets a Settings row.

use std::borrow::Cow;
use std::fmt;

use super::parse::PALETTES;
use super::{ColorDepth, GraphSpan, IntroEffect, LinkSpan, Transition, TuiConfig};

/// One plain key of the `[tui]` table: how it is written, what the template says about it, and
/// how the Settings box names and explains it.
pub struct TuiSetting {
    /// The key as the file spells it.
    pub key: &'static str,
    /// What the Settings box calls the key's row.
    pub label: &'static str,
    /// What the Settings box says of the key's row: what it does and the values it takes.
    pub help: fn() -> Cow<'static, str>,
    /// The comment line the configuration template puts above the key.
    pub template: fn() -> Cow<'static, str>,
    /// What the key holds, and how to read and change it.
    pub kind: TuiSettingKind,
}

/// What a [`TuiSetting`] holds.
pub enum TuiSettingKind {
    /// An on or off switch.
    Bool {
        /// The switch in `tui`.
        get: fn(&TuiConfig) -> bool,
        /// Set the switch in `tui`.
        set: fn(&mut TuiConfig, bool),
    },
    /// One word of a fixed list.
    Word {
        /// The word in `tui`.
        get: fn(&TuiConfig) -> &str,
        /// Set the word in `tui`; `false`, with `tui` untouched, for a word outside the list.
        set: fn(&mut TuiConfig, &str) -> bool,
        /// The words, in the order the Settings box steps through them.
        words: fn() -> Vec<&'static str>,
    },
    /// `accent`: a colour or nothing. Nothing writes the template's commented line back. A change
    /// of the accent does not go through [`TuiSetting::apply`].
    Accent,
}

/// The value of a [`TuiSetting`] to set, by the kind of key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiValue {
    /// For a [`TuiSettingKind::Bool`].
    Bool(bool),
    /// For a [`TuiSettingKind::Word`].
    Word(&'static str),
}

impl TuiSetting {
    /// Set the key in `tui` to `value`.
    ///
    /// A value of the wrong kind, or for `accent`, is a bug in the caller: it panics in a debug
    /// build and changes nothing in a release build. A word outside the key's list changes
    /// nothing either.
    pub fn apply(&self, tui: &mut TuiConfig, value: TuiValue) {
        match (&self.kind, value) {
            (TuiSettingKind::Bool { set, .. }, TuiValue::Bool(on)) => set(tui, on),
            (TuiSettingKind::Word { set, .. }, TuiValue::Word(word)) => {
                set(tui, word);
            }
            _ => debug_assert!(false, "{} does not take {value:?}", self.key),
        }
    }

    /// The key's switch in `tui`, for a key that holds one.
    pub fn flag(&self, tui: &TuiConfig) -> Option<bool> {
        match &self.kind {
            TuiSettingKind::Bool { get, .. } => Some(get(tui)),
            _ => None,
        }
    }

    /// The key's word in `tui`, for a key that holds one.
    pub fn word<'a>(&self, tui: &'a TuiConfig) -> Option<&'a str> {
        match &self.kind {
            TuiSettingKind::Word { get, .. } => Some(get(tui)),
            _ => None,
        }
    }
}

impl PartialEq for TuiSetting {
    /// Two entries are the same key when the file spells them the same.
    fn eq(&self, other: &TuiSetting) -> bool {
        self.key == other.key
    }
}

impl Eq for TuiSetting {}

impl fmt::Debug for TuiSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TuiSetting({})", self.key)
    }
}

/// A fixed text as a help or template line.
macro_rules! text {
    ($text:expr) => {
        || Cow::Borrowed($text)
    };
}

/// A switch's accessors: reads and writes the field named `$field`.
macro_rules! switch {
    ($field:ident) => {
        TuiSettingKind::Bool {
            get: |tui| tui.$field,
            set: |tui, on| tui.$field = on,
        }
    };
}

/// The colour depths, in the order the Settings box steps through them.
const DEPTHS: [ColorDepth; 4] = [
    ColorDepth::Auto,
    ColorDepth::Ansi16,
    ColorDepth::Ansi256,
    ColorDepth::TrueColor,
];

/// `palette`: which colour preset the screens draw from.
pub static PALETTE: TuiSetting = TuiSetting {
    key: "palette",
    label: "Palette",
    help: text!(
        "The colour preset every screen draws from: amber, ice, red or neon. Each names an \
         accent per device family and the good, warn and alert colours."
    ),
    template: text!("The colour preset the screens draw from: amber, ice, red or neon."),
    kind: TuiSettingKind::Word {
        get: |tui| &tui.palette,
        set: |tui, word| match PALETTES.contains(&word) {
            true => {
                tui.palette = word.to_string();
                true
            }
            false => false,
        },
        words: || PALETTES.to_vec(),
    },
};

/// The words `accent` is described with: it has no row of its own, only one per family.
const ACCENT_TEXT: &str =
    "One accent for every device family at once, as an ANSI colour name or #rrggbb.";

/// `accent`: one accent for every device family at once.
///
/// Its label and help are shown nowhere: the Settings box has one accent row per family.
pub static ACCENT: TuiSetting = TuiSetting {
    key: "accent",
    label: "Accent",
    help: text!(ACCENT_TEXT),
    template: text!(ACCENT_TEXT),
    kind: TuiSettingKind::Accent,
};

/// `colors`: the colour depth the screens draw at.
pub static COLORS: TuiSetting = TuiSetting {
    key: "colors",
    label: "Colours",
    help: text!(
        "The colour depth the screens draw at: auto detects the terminal's own, or force 16, \
         256 or true colour. The animation layer needs true colour."
    ),
    template: text!("The colour depth to render at: auto, 16, 256 or true."),
    kind: TuiSettingKind::Word {
        get: |tui| tui.colors.name(),
        set: |tui, word| match DEPTHS.into_iter().find(|depth| depth.name() == word) {
            Some(depth) => {
                tui.colors = depth;
                true
            }
            None => false,
        },
        words: || DEPTHS.map(ColorDepth::name).to_vec(),
    },
};

/// `monochrome`: whether every role drops its colour.
pub static MONOCHROME: TuiSetting = TuiSetting {
    key: "monochrome",
    label: "Monochrome",
    help: text!(
        "Whether every role drops its colour. The selection is then marked by reverse video \
         alone."
    ),
    template: text!(
        "Whether to draw without colour, marking the selection by reverse video alone."
    ),
    kind: switch!(monochrome),
};

/// `theme_background`: whether each palette paints its own greys under the screen.
pub static THEME_BACKGROUND: TuiSetting = TuiSetting {
    key: "theme_background",
    label: "Theme background",
    help: text!(
        "Paints each palette's own dark greys under the screen, its panes and its boxes, so \
         svolder looks the same on any terminal. Off, the terminal's own background shows. \
         Truecolor and 256 colours only."
    ),
    template: text!(
        "Whether each palette paints its own dark greys under the screen, its panes and its boxes."
    ),
    kind: switch!(theme_background),
};

/// `mouse`: whether mouse capture starts on.
pub static MOUSE: TuiSetting = TuiSetting {
    key: "mouse",
    label: "Mouse capture",
    help: text!(
        "Whether the TUI takes the mouse. Capture stops the terminal's own text selection \
         and copy while it is on."
    ),
    template: text!("Whether mouse capture starts on."),
    kind: switch!(mouse),
};

/// `intro`: whether the wordmark intro plays while the device opens.
pub static INTRO: TuiSetting = TuiSetting {
    key: "intro",
    label: "Intro on launch",
    help: text!("Whether the wordmark plays while the device opens. Any key skips it."),
    template: text!("Whether the wordmark intro plays while the device opens."),
    kind: switch!(intro),
};

/// `intro_sound`: whether a short sound plays with the intro.
pub static INTRO_SOUND: TuiSetting = TuiSetting {
    key: "intro_sound",
    label: "Intro sound",
    help: text!(
        "Whether a short sound plays with the wordmark, through PipeWire's pw-play, at the \
         sample rate the device already runs at. Any key that skips the intro stops it."
    ),
    template: text!(
        "Whether a short sound plays with the wordmark intro, through PipeWire's pw-play."
    ),
    kind: switch!(intro_sound),
};

/// `intro_effect`: which effect builds the wordmark.
pub static INTRO_EFFECT: TuiSetting = TuiSetting {
    key: "intro_effect",
    label: "Intro effect",
    help: text!(
        "Which effect builds the wordmark at launch: random picks a demo effect each launch, \
         never the fade; fade is the plain fade-in; particles throws sparks in from the edges. \
         Below truecolor, or with Effects off, the mark is drawn static whatever this says."
    ),
    template: text!("Which effect builds the wordmark intro: random, fade or particles."),
    kind: TuiSettingKind::Word {
        get: |tui| tui.intro_effect.name(),
        set: |tui, word| match IntroEffect::named(word) {
            Some(effect) => {
                tui.intro_effect = effect;
                true
            }
            None => false,
        },
        words: || IntroEffect::ALL.map(IntroEffect::name).to_vec(),
    },
};

/// `transition`: how one screen gives way to the next.
pub static TRANSITION: TuiSetting = TuiSetting {
    key: "transition",
    label: "Transition",
    help: text!(
        "How one screen gives way to the next: none, trace, noise or melt. Every one but \
         none belongs to the animation layer, so Effects off plays none of them."
    ),
    template: text!("How one screen gives way to the next: none, trace, noise or melt."),
    kind: TuiSettingKind::Word {
        get: |tui| tui.transition.name(),
        set: |tui, word| match Transition::named(word) {
            Some(transition) => {
                tui.transition = transition;
                true
            }
            None => false,
        },
        words: || Transition::ALL.map(Transition::name).to_vec(),
    },
};

/// `title_meter`: whether the terminal's title carries the meters as bars.
pub static TITLE_METER: TuiSetting = TuiSetting {
    key: "title_meter",
    label: "Title meter",
    help: text!(
        "Whether the terminal's title carries one bar per channel of the device's meters, \
         else of what the host plays, while they stream."
    ),
    template: text!(
        "Whether the terminal's title carries one bar per channel of the device's meters, else of what the host plays."
    ),
    kind: switch!(title_meter),
};

/// `confirm_quit`: whether `q` asks before the TUI quits.
pub static CONFIRM_QUIT: TuiSetting = TuiSetting {
    key: "confirm_quit",
    label: "Confirm quit",
    help: text!(
        "Whether q asks Quit svolder? [Y/n] before the TUI quits. Off quits on the one q. \
         Ctrl-C quits at once either way."
    ),
    template: text!("Whether q asks before svolder tui quits; Ctrl-C quits at once either way."),
    kind: switch!(confirm_quit),
};

/// `host_audio`: whether the TUI reaches the host's own audio.
pub static HOST_AUDIO: TuiSetting = TuiSetting {
    key: "host_audio",
    label: "Host audio",
    help: text!(
        "Whether the TUI reaches the host's own audio: what plays, the sink's volume and the \
         media player's transport. SVOLDER_HOST=off holds it off for one process whatever \
         this says."
    ),
    template: text!(
        "Whether svolder tui shows what the host plays, with its volume and transport."
    ),
    kind: switch!(host_audio),
};

/// `visualizer`: whether the device screen draws a spectrum of what the host plays.
pub static VISUALIZER: TuiSetting = TuiSetting {
    key: "visualizer",
    label: "Visualizer",
    help: text!(
        "Whether the device screen's Spectrum row draws what the host plays in sixteen \
         bands. It needs host audio on."
    ),
    template: text!("Whether the device screen draws a spectrum of what the host plays."),
    kind: switch!(visualizer),
};

/// `graphs`: whether the screens draw their graphs.
pub static GRAPHS: TuiSetting = TuiSetting {
    key: "graphs",
    label: "Graphs",
    help: text!(
        "Whether the screens draw their graphs: the Telemetry columns, the level history and \
         the connection strip. Off draws every screen without them."
    ),
    template: text!(
        "Whether the screens draw their graphs: Telemetry, the level history and the connection strip."
    ),
    kind: switch!(graphs),
};

/// What `graph_span` does, for its help and its template line.
fn graph_span_text() -> Cow<'static, str> {
    Cow::Owned(format!(
        "How much history the Telemetry and level graphs span: {}.",
        GraphSpan::list()
    ))
}

/// What `link_span` does, for its help and its template line.
fn link_span_text() -> Cow<'static, str> {
    Cow::Owned(format!(
        "How much history the connection strip spans: {}.",
        LinkSpan::list()
    ))
}

/// `graph_span`: how much history the Telemetry and level graphs span.
pub static GRAPH_SPAN: TuiSetting = TuiSetting {
    key: "graph_span",
    label: "Graph span",
    help: graph_span_text,
    template: graph_span_text,
    kind: TuiSettingKind::Word {
        get: |tui| tui.graph_span.name(),
        set: |tui, word| match GraphSpan::named(word) {
            Some(span) => {
                tui.graph_span = span;
                true
            }
            None => false,
        },
        words: || GraphSpan::ALL.map(GraphSpan::name).to_vec(),
    },
};

/// `link_span`: how much history the connection strip spans.
pub static LINK_SPAN: TuiSetting = TuiSetting {
    key: "link_span",
    label: "Link span",
    help: link_span_text,
    template: link_span_text,
    kind: TuiSettingKind::Word {
        get: |tui| tui.link_span.name(),
        set: |tui, word| match LinkSpan::named(word) {
            Some(span) => {
                tui.link_span = span;
                true
            }
            None => false,
        },
        words: || LinkSpan::ALL.map(LinkSpan::name).to_vec(),
    },
};

/// Every plain key of the `[tui]` table, in the order the template lists them and the writer
/// appends them.
pub static TUI_SETTINGS: [&TuiSetting; 17] = [
    &PALETTE,
    &ACCENT,
    &COLORS,
    &MONOCHROME,
    &THEME_BACKGROUND,
    &MOUSE,
    &INTRO,
    &INTRO_EFFECT,
    &INTRO_SOUND,
    &TRANSITION,
    &TITLE_METER,
    &CONFIRM_QUIT,
    &HOST_AUDIO,
    &VISUALIZER,
    &GRAPHS,
    &GRAPH_SPAN,
    &LINK_SPAN,
];

#[cfg(test)]
mod tests;
