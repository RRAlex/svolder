// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Building a [`PanelView`] from what the daemon tells a client: the device's readings and
//! profile, the `devices` row, the host view and the scenes.

use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::{
    ControlKind, EqSpec, Family, Profile, device_presets, roles, safety, units,
};

use super::{
    MINUS, PANEL_VERSION, PanelActions, PanelChoice, PanelChoices, PanelComputerEq, PanelDevice,
    PanelHop, PanelLabelTone, PanelPath, PanelScenes, PanelSegment, PanelSignal, PanelTone,
    PanelView, PanelVolume, number_text,
};
use crate::host::SetupSummary;
use crate::host::{
    DacFacts, DacStage, HostView, PathLine, PathRow, Tone, khz, path_line, path_row, setup_check,
    sink_is_dac,
};
use crate::socket::protocol::{ScenesView, ValueObject};
use crate::socket::signal::Signal;

/// The link to the device as the daemon's `devices` list gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelLink {
    /// The device's configured name.
    pub name: String,
    /// Whether the link is up.
    pub connected: bool,
    /// The last signal strength the daemon heard.
    pub signal: Option<Signal>,
}

/// What a [`PanelView`] is built from.
#[derive(Debug, Clone, Copy)]
pub struct PanelInput<'a> {
    /// The device's profile; `None` when no device is chosen or its profile is not known.
    pub profile: Option<&'a Profile>,
    /// The device's link; `None` when no device is chosen.
    pub link: Option<&'a PanelLink>,
    /// The device's readings as `state` gave them, kept up to date by the stream.
    pub readings: &'a [ValueObject],
    /// The host view; `None` while the daemon has not sent one.
    pub host: Option<&'a HostView>,
    /// The scenes; `None` from a daemon that has none.
    pub scenes: Option<&'a ScenesView>,
    /// The saved tone-control presets of the configuration.
    pub presets: &'a [Preset],
    /// The time to judge a signal's age by, in unix milliseconds.
    pub now_ms: u64,
}

/// The arguments of the panel's own actions; they never name a device or a global flag.
pub(super) fn actions() -> PanelActions {
    let run = |args: &[&str]| args.iter().map(|arg| (*arg).to_string()).collect();
    PanelActions {
        volume_up: run(&["set", "volume", "up"]),
        volume_down: run(&["set", "volume", "down"]),
        mute_on: run(&["set", "mute", "on"]),
        mute_off: run(&["set", "mute", "off"]),
        volume_set: run(&["set", "volume", "{db}dB"]),
        eq_on: run(&["host", "eq", "on"]),
        eq_off: run(&["host", "eq", "off"]),
    }
}

fn choice(name: &str, args: &[&str]) -> PanelChoice {
    PanelChoice {
        name: name.to_string(),
        run: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

impl PanelView {
    /// The panel for `input`.
    pub fn build(input: &PanelInput<'_>) -> PanelView {
        let device = input.link.zip(input.profile);
        let facts = device.map(|(info, profile)| dac_facts(input, info, profile));
        PanelView {
            v: PANEL_VERSION,
            device: device.map(|(info, profile)| panel_device(input, info, profile)),
            computer_eq: computer_eq(input),
            path: device
                .zip(facts)
                .and_then(|((_, profile), facts)| path(input, profile, &facts)),
            scenes: scenes(input.scenes),
            actions: actions(),
        }
    }
}

/// The raw whole number `id` last read, a boolean as 0 or 1.
fn raw_of(readings: &[ValueObject], id: svolder_proto::FunctionId) -> Option<i64> {
    let id = id.to_string();
    let raw = &readings.iter().find(|v| v.id == id)?.raw;
    raw.as_i64().or_else(|| raw.as_bool().map(i64::from))
}

/// What the device reports of its end of the path; with the link down, the readings held are
/// from before the drop and the device's own stage is not claimed from them.
fn dac_facts(input: &PanelInput<'_>, info: &PanelLink, profile: &Profile) -> DacFacts {
    let mut facts = DacFacts::read(profile, |id| raw_of(input.readings, id));
    if !info.connected {
        facts.stage = DacStage::default();
    }
    facts
}

fn panel_device(input: &PanelInput<'_>, info: &PanelLink, profile: &Profile) -> PanelDevice {
    let raw = |id| raw_of(input.readings, id);
    let facts = dac_facts(input, info, profile);
    PanelDevice {
        name: info.name.clone(),
        present: info.connected,
        rate_hz: facts.rate_hz,
        rate_text: facts.rate_hz.map(khz),
        volume: volume(profile, raw),
        input: input_choices(profile, raw),
        preset: preset_choices(profile, input.presets, raw),
        signal: info.signal.as_ref().map(|signal| PanelSignal {
            dbm: signal.dbm,
            dbm_text: number_text(f64::from(signal.dbm), 0),
            heard_at_ms: signal.heard_at_ms,
            bars: signal.bars(),
            weak: signal.weak(),
            old: signal.old(input.now_ms),
        }),
    }
}

/// The volume the profile has, in dB where it is.
fn volume(
    profile: &Profile,
    raw: impl Fn(svolder_proto::FunctionId) -> Option<i64>,
) -> Option<PanelVolume> {
    let control = roles::volume(profile)?;
    let in_db = safety::in_db(control);
    let at = |raw: i64| {
        units::amount(control, raw)
            .filter(|_| in_db)
            .map(|amount| amount.number().unwrap_or(f64::NEG_INFINITY))
    };
    let db = raw(control.id).and_then(at).filter(|db| db.is_finite());
    // The lowest step of a dB level can be silence: the slider starts at the lowest number.
    let (min_db, max_db) = match control.range {
        Some((low, high)) if in_db => (
            (low..=high)
                .filter_map(at)
                .filter(|v| v.is_finite())
                .reduce(f64::min),
            at(high),
        ),
        _ => (None, None),
    };
    let muted = profile
        .control_by_key("mute")
        .filter(|mute| mute.kind == ControlKind::Toggle)
        .map(|mute| raw(mute.id) == Some(1));
    Some(PanelVolume {
        db,
        min_db,
        max_db,
        muted,
        text: db.map(|db| number_text(db, 1)),
        bar_text: db.map(|db| number_text(db, 0)),
    })
}

/// The input control's values, each selected by `set input NAME`.
fn input_choices(
    profile: &Profile,
    raw: impl Fn(svolder_proto::FunctionId) -> Option<i64>,
) -> Option<PanelChoices> {
    let control = profile.control_by_key("input")?;
    if control.kind != ControlKind::Select || control.values.is_empty() {
        return None;
    }
    let on = raw(control.id)
        .and_then(|raw| u8::try_from(raw).ok())
        .and_then(|raw| control.value_name(raw))
        .map(str::to_string);
    Some(PanelChoices {
        on,
        choices: control
            .values
            .iter()
            .map(|(_, name)| choice(name, &["set", "input", "--", name]))
            .collect(),
    })
}

/// Whether the saved `preset` fits the tone control `eq`.
fn fits(preset: &Preset, eq: &EqSpec) -> bool {
    match preset.is_imported() {
        true => !eq.bands.is_empty(),
        false => eq.bands.len() == usize::from(preset.bands),
    }
}

/// The device's presets as the TUI's preset list for it offers them: the tone control's presets
/// on a device that has a tone control (the built-ins, then the saved ones that fit, loaded by
/// `eq load`), the device's own slots and built-in settings on one that keeps presets itself
/// (`set recall N`, `preset apply NAME`), and nothing on any other.
fn preset_choices(
    profile: &Profile,
    saved: &[Preset],
    raw: impl Fn(svolder_proto::FunctionId) -> Option<i64>,
) -> Option<PanelChoices> {
    if let Some(eq) = &profile.eq {
        let mut listed = preset::builtins(eq);
        listed.extend(saved.iter().filter(|p| fits(p, eq)).cloned());
        let on = listed
            .iter()
            .find(|candidate| preset::matches(eq, &raw, candidate))
            .map(|found| found.name.clone());
        return Some(PanelChoices {
            on,
            choices: listed
                .iter()
                .map(|p| choice(&p.name, &["eq", "load", "--", &p.name]))
                .collect(),
        });
    }
    if !device_presets::offered(profile) {
        return None;
    }
    let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
    let mut choices: Vec<PanelChoice> = (1..=slots)
        .map(|slot| {
            choice(
                &device_presets::slot_name(slot),
                &["set", "recall", &slot.to_string()],
            )
        })
        .collect();
    let on = device_presets::matching(profile, &raw).map(|setting| setting.name.clone());
    if let Some(table) = &profile.builtin_settings {
        choices.extend(
            table
                .settings
                .iter()
                .map(|setting| choice(&setting.name, &["preset", "apply", "--", &setting.name])),
        );
    }
    Some(PanelChoices { on, choices })
}

/// The computer's EQ, when its controls are there: its built-in presets (those of the device's
/// tone control, else of a five-band one) and the saved ones.
fn computer_eq(input: &PanelInput<'_>) -> Option<PanelComputerEq> {
    let eq = input
        .host?
        .eq
        .as_ref()
        .filter(|eq| eq.unavailable.is_none())?;
    let bands = input
        .profile
        .and_then(|profile| profile.eq.as_ref())
        .map_or(5, |spec| spec.bands.len());
    let names = preset::builtin_names(bands)
        .iter()
        .map(|name| (*name).to_string())
        .chain(
            input
                .presets
                .iter()
                .filter(|p| preset::host_correction(p).is_ok())
                .map(|p| p.name.clone()),
        );
    Some(PanelComputerEq {
        on: eq.on,
        preset: eq.preset.clone(),
        choices: names
            .map(|name| choice(&name, &["host", "eq", "preset", "--", &name]))
            .collect(),
    })
}

/// The Path: judged by the one judge, drawn as three segments and the words for what is off.
fn path(input: &PanelInput<'_>, profile: &Profile, facts: &DacFacts) -> Option<PanelPath> {
    let host = input.host.filter(|host| host.available)?;
    if profile.family != Family::Dac {
        return None;
    }
    let route = host.route.as_ref()?;
    let check = route
        .sink
        .as_ref()
        .is_some_and(|sink| sink_is_dac(sink, facts))
        .then(|| setup_check(route));
    let line = path_line(route, facts);
    let row = path_row(&line, check.as_ref());
    let fails = match (
        check.as_ref().map(|check| check.summary),
        row.hops.is_empty(),
    ) {
        (Some(SetupSummary::Changes(fails)), false) => Some(fails),
        _ => None,
    };
    Some(PanelPath {
        end_to_end: line.end_to_end().key().to_string(),
        tone: match row.label.tone {
            Tone::Good => PanelLabelTone::Good,
            Tone::Warn => PanelLabelTone::Warn,
            Tone::Plain | Tone::Dim => PanelLabelTone::Plain,
        },
        label: row.label.words.clone(),
        segments: segments(&line),
        off: off_words(&row, fails),
    })
}

/// The three segments: each hop's tone, or grey for all three when the sound does not go
/// through the path (another sink or input, no route).
fn segments(line: &PathLine) -> Vec<PanelSegment> {
    let hops = [PanelHop::Player, PanelHop::Computer, PanelHop::Dac];
    hops.iter()
        .enumerate()
        .map(|(at, hop)| {
            let tone = match line.hops.get(at) {
                _ if line.hops.len() != 3 => PanelTone::Unknown,
                Some(h) if h.changes => PanelTone::Warn,
                Some(h) if h.unknown => PanelTone::Unknown,
                Some(_) => PanelTone::Good,
                None => PanelTone::Unknown,
            };
            PanelSegment { hop: *hop, tone }
        })
        .collect()
}

/// The words for what is off: each hop's changes grouped under its name (`DAC: volume −24.1 dB
/// · tone control`), groups joined by `; `, then `config N` for a failing config check. Empty
/// for a path untouched end to end.
fn off_words(row: &PathRow, config_fails: Option<u32>) -> String {
    // A path with no hops (another sink, another input, no route) has no changes to name: the
    // row's own short words are what the user is told.
    if row.hops.is_empty() {
        return minus_signs(row.words.as_deref().unwrap_or_default());
    }
    let names = ["Player", "Computer", "DAC"];
    // The row puts `config N` last of its values when the check fails.
    let values = match config_fails {
        Some(_) => &row.values[..row.values.len().saturating_sub(1)],
        None => &row.values[..],
    };
    let mut groups: Vec<String> = Vec::new();
    for (at, name) in names.iter().enumerate() {
        let mut words: Vec<&str> = row
            .hops
            .get(at)
            .map(|hop| {
                hop.words
                    .iter()
                    .filter(|word| word.tone == Tone::Warn)
                    .map(|word| word.text.as_str())
                    .collect()
            })
            .unwrap_or_default();
        if at == 2 {
            words.extend(
                values
                    .iter()
                    .filter(|word| word.tone == Tone::Warn)
                    .map(|word| word.text.as_str()),
            );
        }
        if !words.is_empty() {
            groups.push(format!("{name}: {}", words.join(" \u{b7} ")));
        }
    }
    groups.extend(config_fails.map(|fails| format!("config {fails}")));
    minus_signs(&groups.join("; "))
}

/// `text` with every hyphen that is a number's sign written as the proper minus: one that starts
/// a number, with nothing but a space, an opening bracket or a middle dot before it. A hyphen
/// inside a name (`HDMI-2`) stays as it is.
pub(super) fn minus_signs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(at, c)| {
            let starts_a_number = chars.get(at + 1).is_some_and(char::is_ascii_digit);
            let after_a_word = at > 0 && !matches!(chars[at - 1], ' ' | '(' | '\u{b7}');
            match (c, starts_a_number && !after_a_word) {
                ('-', true) => MINUS,
                _ => *c,
            }
        })
        .collect()
}

fn scenes(view: Option<&ScenesView>) -> PanelScenes {
    let Some(view) = view else {
        return PanelScenes {
            on: None,
            choices: Vec::new(),
        };
    };
    PanelScenes {
        on: view.on.clone().filter(|_| view.known),
        choices: view
            .scenes
            .iter()
            .map(|scene| choice(&scene.name, &["scene", "load", "--", &scene.name]))
            .collect(),
    }
}
