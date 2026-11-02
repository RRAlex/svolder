// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's view models: what each part of a device's screen shows, worked out from its
//! row, its profile, its readings and the writer's climb, in the page's wording — the core's text
//! ([`svolder_profiles::units`]) with a typographic minus. Pure: the handlers render these
//! through the templates, the stream sends their changes, and the tests read them directly.
//!
//! - `controls`: every control in the page's terms, by its kind.
//! - `device_presets`: a device's own presets: its slots and its built-in settings.
//! - `dial`: the volume's knob.
//! - `facts`: the Device fold-out: the device's facts and every reading it holds.
//! - `host`: the Now playing card: the player's track and the path the sound takes to the device.
//! - `meters`: an amp's meters and its power.
//! - `strip`: the strip along the bottom of the page: what the device reported, a bar a second.
//! - `tone`: the tone control: its mode, its curve, the preset and the band table.

mod controls;
mod device_presets;
mod dial;
mod facts;
mod host;
mod meters;
mod strip;
mod tone;

pub(crate) use controls::{Choice, ControlView, Slider};
pub(crate) use device_presets::DevicePresets;
pub(crate) use dial::Knob;
pub(crate) use facts::Facts;
pub(crate) use host::{Playing, playing_of};
pub(crate) use meters::{Meters, PowerView};
pub(crate) use strip::{Past, Strip, last_second, strip};
pub(crate) use tone::{Bands, Presets, Tone};

use std::collections::BTreeMap;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use svolder_ipc::DeviceInfo;
use svolder_ipc::host::HostView;
use svolder_profiles::{
    Control, ControlKind, Convert, Family, Profile, db_tables, roles, tone::response,
};
use svolder_proto::{FunctionId, Message, Value};

use crate::writer::ClimbView;

/// What a viewer may do on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Access {
    /// Whether the viewer may change a device: signed in, with `[web] writes` on.
    pub(crate) writes: bool,
    /// Whether the viewer is a guest.
    pub(crate) guest: bool,
}

/// One device as the page reads it.
pub(crate) struct Snapshot<'a> {
    /// Its row: name, link, wait, refusal, memory.
    pub(crate) info: &'a DeviceInfo,
    /// Its profile, once the device has been seen.
    pub(crate) profile: Option<&'a Profile>,
    /// Every reading it has confirmed.
    pub(crate) readings: &'a [Message],
    /// When the daemon last heard each reading, in unix milliseconds.
    pub(crate) heard_at: &'a BTreeMap<FunctionId, u64>,
    /// The daemon's clock when the snapshot was taken, in unix milliseconds.
    pub(crate) now_ms: u64,
    /// The writer's climb, while one is on.
    pub(crate) climb: Option<&'a ClimbView>,
    /// `[web] max_volume_db`.
    pub(crate) cap_db: Option<f64>,
    /// What the viewer may do.
    pub(crate) access: Access,
    /// The presets that fit its tone control, each with whether it is a built-in.
    pub(crate) presets: &'a [(svolder_profiles::tone::preset::Preset, bool)],
    /// Whether the daemon may write its configuration, or why not.
    pub(crate) writable: &'a Result<(), String>,
    /// The computer's player and route as the daemon last said.
    pub(crate) host: &'a HostView,
}

impl Snapshot<'_> {
    /// Whether the device has never connected: no profile, or one an advertisement named while
    /// no link was ever up (a connected device, or one that was, has readings; a refusal is a
    /// link that was up).
    pub(crate) fn unseen(&self) -> bool {
        self.profile.is_none()
            || (!self.info.connected && !self.info.refused && self.readings.is_empty())
    }

    /// Function `id`'s reading.
    pub(crate) fn message(&self, id: FunctionId) -> Option<&Message> {
        self.readings.iter().find(|m| m.id == id)
    }

    /// Function `id`'s raw reading: an integer as itself, a bool as 0 or 1.
    pub(crate) fn raw(&self, id: FunctionId) -> Option<i64> {
        let value = &self.message(id)?.value;
        value.as_i64().or_else(|| match value {
            Value::Bool(on) => Some(i64::from(*on)),
            _ => None,
        })
    }

    /// Whether the control `key` reads on, `None` before it has reported.
    pub(crate) fn reads_on(&self, profile: &Profile, key: &str) -> Option<bool> {
        let control = profile.control_by_key(key)?;
        Some(control.reads_true(&self.message(control.id)?.value))
    }
}

/// The glow's level when nothing sets it: a device with no level to show.
pub(crate) const REST_GLOW: f64 = 0.06;

/// A device's whole screen.
pub(crate) struct Screen {
    /// Its configured name.
    pub(crate) name: String,
    /// Its name as one URL path segment.
    pub(crate) url: String,
    /// Its name in hex: what an element id that names the device is made of, since a percent
    /// sign is no part of a CSS identifier.
    pub(crate) dom: String,
    /// Its family's class: `dac`, `amp` or `preamp`.
    pub(crate) family: &'static str,
    /// Its family's word under its name: `DAC`, `amp`, `preamp`.
    pub(crate) family_word: &'static str,
    /// Whether it lays out as an amp: meters and power first (an amp, or a device with meters
    /// and no volume).
    pub(crate) amp: bool,
    /// Whether it is an amp with only its meters to show (no power button, no controls): the
    /// page centres them.
    pub(crate) lone: bool,
    /// The link pill.
    pub(crate) link: LinkView,
    /// The card above the view: a lost link, a fault, a refusal, a device not seen yet.
    pub(crate) card: Option<Card>,
    /// The volume's knob.
    pub(crate) knob: Option<Knob>,
    /// The Controls section.
    pub(crate) controls: Vec<ControlView>,
    /// An amp's meters.
    pub(crate) meters: Option<Meters>,
    /// The power button and, on an amp, its switches and readouts.
    pub(crate) power: Option<PowerView>,
    /// The tone control.
    pub(crate) tone: Option<Tone>,
    /// The presets.
    pub(crate) presets: Option<Presets>,
    /// The device's own presets: its slots and its built-in settings.
    pub(crate) device_presets: Option<DevicePresets>,
    /// The band table.
    pub(crate) bands: Option<Bands>,
    /// Every band's steps with the core's words, as JSON for the page's drag: sent once per
    /// screen, where the tone part is drawn.
    pub(crate) tone_steps: Option<String>,
    /// The Now playing card, when the daemon watches the computer's player.
    pub(crate) playing: Option<Playing>,
    /// The Device fold-out below the rest.
    pub(crate) facts: Facts,
    /// The glow's level, 0 to 1, to two decimals.
    pub(crate) glow: String,
    /// Whether every control is drawn inert: a guest, or writes off.
    pub(crate) inert: bool,
    /// What the screen is made of, as a token: the card that stands above it, its layout and
    /// which parts it has. A page whose token differs from the device's is drawn anew.
    pub(crate) shape: String,
}

/// The link pill.
pub(crate) struct LinkView {
    /// `connected`, `connecting…` or `not connected`.
    pub(crate) word: &'static str,
    /// Its class: `on`, `wait` or `off`.
    pub(crate) state: &'static str,
}

/// A card above the view.
#[derive(Debug)]
pub(crate) struct Card {
    /// Its tint: `warn` (a lost link), `fault`, or `dim`.
    pub(crate) kind: &'static str,
    /// Its title.
    pub(crate) title: String,
    /// Its line under the title; a command in it is in backticks ([`spans`]). Empty on a fault's
    /// card, which has a line for each fault instead.
    pub(crate) detail: String,
    /// The active faults, one line each, in the profile's order.
    pub(crate) faults: Vec<Fault>,
    /// The whole seconds until the next connect attempt, for the countdown bar.
    pub(crate) retry_in: Option<u64>,
    /// Whether it offers Try now.
    pub(crate) try_now: bool,
}

/// One active fault: what it is and what the profile advises.
#[derive(Debug)]
pub(crate) struct Fault {
    /// The profile's label for it.
    pub(crate) label: String,
    /// The profile's advice; empty when it has none.
    pub(crate) advice: String,
}

impl Card {
    /// The detail as text and code.
    pub(crate) fn spans(&self) -> Vec<Span> {
        spans(&self.detail)
    }
}

/// A run of words, or a command.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Span {
    /// Whether it is a command, drawn in the mono face.
    pub(crate) code: bool,
    pub(crate) text: String,
}

/// `text` split at its backticks: the runs between a pair are commands.
pub(crate) fn spans(text: &str) -> Vec<Span> {
    text.split('`')
        .enumerate()
        .filter(|(_, run)| !run.is_empty())
        .map(|(i, run)| Span {
            code: i % 2 == 1,
            text: run.to_string(),
        })
        .collect()
}

/// The card for a daemon whose configuration names no device.
pub(crate) fn no_devices() -> Card {
    Card {
        kind: "dim",
        title: "No devices".to_string(),
        detail: "No devices yet. `svolder setup` adds one.".to_string(),
        faults: Vec::new(),
        retry_in: None,
        try_now: false,
    }
}

/// The card for a device that is listed but cannot be read this moment (a reload under way).
pub(crate) fn unavailable(name: &str) -> Card {
    Card {
        kind: "dim",
        title: format!("{name} is not available"),
        detail: "svolder cannot read it right now; the page updates when it can.".to_string(),
        faults: Vec::new(),
        retry_in: None,
        try_now: false,
    }
}

/// `text` with every `-` that signs a number written `−` (U+2212), the page's minus.
pub(crate) fn minus(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let signs = c == '-'
            && chars
                .peek()
                .is_some_and(|n| n.is_ascii_digit() || *n == '.');
        out.push(if signs { '−' } else { c });
    }
    out
}

/// The characters a path segment keeps as they are.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// `name` as one percent-encoded URL path segment.
pub(crate) fn segment(name: &str) -> String {
    utf8_percent_encode(name, SEGMENT).to_string()
}

/// `name` in hex: what an element id that names the device is made of.
pub(crate) fn dom(name: &str) -> String {
    name.bytes().map(|b| format!("{b:02x}")).collect()
}

/// The link pill of the device `info` describes.
pub(crate) fn link_of(info: &DeviceInfo) -> LinkView {
    match (info.connected, info.retry_in_secs) {
        (true, _) => LinkView {
            word: "connected",
            state: "on",
        },
        (false, Some(_)) => LinkView {
            word: "not connected",
            state: "off",
        },
        (false, None) => LinkView {
            word: "connecting…",
            state: "wait",
        },
    }
}

/// The family of the device `info` describes: a DAC when no profile is known for it.
pub(crate) fn family_of(web: &crate::web::Web, info: &DeviceInfo) -> Family {
    web.fleet.profile(info).map_or(Family::Dac, |p| p.family)
}

/// A family's class and the word under a device's name.
pub(crate) fn family_words(family: Family) -> (&'static str, &'static str) {
    match family {
        Family::Dac => ("dac", "DAC"),
        Family::Amp => ("amp", "amp"),
        Family::Preamp => ("preamp", "preamp"),
    }
}

/// The device's screen.
pub(crate) fn screen(s: &Snapshot) -> Screen {
    let info = s.info;
    let family = s.profile.map_or(Family::Dac, |p| p.family);
    let (family_class, family_word) = family_words(family);
    let mut screen = Screen {
        name: info.name.clone(),
        url: segment(&info.name),
        dom: dom(&info.name),
        family: family_class,
        family_word,
        amp: family == Family::Amp,
        lone: false,
        link: link_of(info),
        card: card(s),
        knob: None,
        controls: Vec::new(),
        meters: None,
        power: None,
        tone: None,
        presets: None,
        device_presets: None,
        bands: None,
        tone_steps: None,
        playing: host::playing(s, family),
        facts: facts::facts(s),
        glow: format!("{REST_GLOW:.2}"),
        inert: !s.access.writes || s.access.guest,
        shape: String::new(),
    };
    let Some(profile) = s.profile.filter(|_| !s.unseen()) else {
        screen.shape = shape_of(&screen, None);
        return screen;
    };
    screen.knob = dial::knob(s, profile);
    screen.meters = meters::meters(s, profile);
    screen.amp |= screen.knob.is_none() && screen.meters.is_some();
    screen.power = meters::power(s, profile, screen.amp);
    screen.controls = controls::section(s, profile, screen.amp);
    screen.lone = screen.amp && screen.power.is_none() && screen.controls.is_empty();
    if let Some(eq) = profile.eq.as_ref() {
        screen.tone = Some(tone::tone(s, eq));
        screen.presets = Some(tone::presets(s, eq));
        screen.bands = Some(tone::bands(s, eq));
        screen.tone_steps = screen.has("tone").then(|| tone::steps_json(eq));
    }
    screen.device_presets = device_presets::device_presets(s, profile);
    let level = match (&screen.knob, &screen.meters) {
        (Some(knob), _) if !knob.muted => knob.fraction(),
        (None, Some(meters)) => meters.louder(),
        _ => None,
    };
    screen.glow = format!("{:.2}", level.unwrap_or(REST_GLOW));
    screen.shape = shape_of(&screen, Some(&profile.name));
    screen
}

/// The token of `screen`'s shape: a hash of its card (kind and title, not the countdown in its
/// words), its profile, its layout and which parts it has.
fn shape_of(screen: &Screen, profile: Option<&str>) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let card = screen.card.as_ref().map(|c| (c.kind, c.title.as_str()));
    let mut hasher = DefaultHasher::new();
    (
        card,
        profile,
        screen.family,
        screen.amp,
        screen.knob.is_some(),
        screen.meters.is_some(),
        screen.power.is_some(),
        screen.tone.is_some(),
        (screen.presets.is_some(), screen.device_presets.is_some()),
        screen.controls.len(),
        screen.inert,
        screen.playing.is_some(),
    )
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl Screen {
    /// Whether the view is drawn dimmed: any card but a fault's, which leaves it in use.
    pub(crate) fn away(&self) -> bool {
        self.card.as_ref().is_some_and(|c| c.kind != "fault")
    }

    /// Whether the columns are inert: a lost link leaves nothing to write to.
    pub(crate) fn locked(&self) -> bool {
        self.card.as_ref().is_some_and(|c| c.kind == "warn")
    }

    /// Whether the screen as it is drawn has an element for the part `part` (by the name the
    /// stream's parts go by) to land on: the amp layout draws no tone control or presets.
    pub(crate) fn has(&self, part: &str) -> bool {
        match part {
            "card" => self.card.is_some(),
            "tone" => !self.amp && self.tone.is_some(),
            "presets" => !self.amp && self.presets.is_some(),
            "device-presets" => !self.amp && self.device_presets.is_some(),
            "power" => self.power.is_some(),
            "playing" => self.playing.is_some(),
            _ => true,
        }
    }
}

/// The card the device's state earns, the most pressing first: never seen, a refused
/// passphrase, a lost link, the active faults (all of them, one line each).
fn card(s: &Snapshot) -> Option<Card> {
    let name = &s.info.name;
    let Some(profile) = s.profile.filter(|_| !s.unseen()) else {
        // No link yet in this daemon's run (a restart included): looking for it, with the
        // wait's countdown and Try now when an attempt is scheduled.
        let wait = s
            .info
            .retry_in_secs
            .map(|secs| format!(" Next attempt in {secs} s."))
            .unwrap_or_default();
        return Some(Card {
            kind: "dim",
            title: format!("Looking for {name}…"),
            detail: format!(
                "svolder has not found it yet: is it on, in range, and free of the phone app?{wait}"
            ),
            faults: Vec::new(),
            retry_in: s.info.retry_in_secs,
            try_now: s.info.retry_in_secs.is_some() && s.access.writes && !s.access.guest,
        });
    };
    if s.info.refused {
        return Some(Card {
            kind: "dim",
            title: format!("{name} refused its passphrase"),
            detail: "Run `svolder doctor` on the computer svolder runs on: it says which code to \
                     set."
                .to_string(),
            faults: Vec::new(),
            retry_in: None,
            try_now: false,
        });
    }
    if !s.info.connected {
        let detail = match s.info.retry_in_secs {
            Some(secs) => format!("Not connected. Next attempt in {secs} s."),
            None => "Not connected. Trying now.".to_string(),
        };
        return Some(Card {
            kind: "warn",
            title: format!("{name} is out of reach"),
            detail,
            faults: Vec::new(),
            retry_in: s.info.retry_in_secs,
            try_now: s.info.retry_in_secs.is_some() && s.access.writes && !s.access.guest,
        });
    }
    let faults: Vec<Fault> = profile
        .controls
        .iter()
        .filter(|c| c.kind == ControlKind::Fault)
        .filter(|c| s.message(c.id).is_some_and(|m| c.reads_true(&m.value)))
        .map(|fault| Fault {
            label: fault.label.clone(),
            advice: fault.advice.clone().unwrap_or_default(),
        })
        .collect();
    let title = match faults.len() {
        0 => return None,
        1 => "Fault".to_string(),
        n => format!("{n} faults"),
    };
    Some(Card {
        kind: "fault",
        title,
        detail: String::new(),
        faults,
        retry_in: None,
        try_now: false,
    })
}

/// The first volume step at which the summed curve's `peak` (dB) could clip, by `control`'s
/// table; `None` where its steps are not a table or none clips.
pub(crate) fn risk_step(control: &Control, peak: f64) -> Option<i64> {
    let Some(Convert::Table(name)) = control.convert.as_ref() else {
        return None;
    };
    let safe = response::threshold_step(peak, db_tables::table(name)?)?;
    i64::try_from(safe).ok().map(|safe| safe + 1)
}

/// The volume and the mute toggle the knob carries, when the profile has a volume.
fn knob_controls(profile: &Profile) -> (Option<&Control>, Option<&Control>) {
    let volume = roles::volume(profile);
    let mute = volume.and_then(|_| profile.controls.iter().find(|c| c.mutes_output));
    (volume, mute)
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
