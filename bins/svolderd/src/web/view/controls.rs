// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Every control in the page's terms, by its kind (8a): a select with up to five values as a
//! segmented track and with more as a list, a toggle as a switch, a level as a slider, a
//! readout and a fault as a label and its words.

use svolder_profiles::{Control, ControlKind, FlagWords, Profile, describe, units};
use svolder_proto::{Value, ValueType};

use super::{Snapshot, knob_controls, minus};

/// The most values a select shows as a segmented track.
const SEGMENTS_MAX: usize = 5;

/// One control.
#[derive(Debug)]
pub(crate) enum ControlView {
    /// A select of up to five values.
    Segments {
        key: String,
        label: String,
        options: Vec<Choice>,
    },
    /// A select of more.
    List {
        key: String,
        label: String,
        options: Vec<Choice>,
    },
    /// A toggle; `None` before it has reported.
    Switch {
        key: String,
        label: String,
        on: Option<bool>,
        /// What a press posts, as JSON: the opposite of the reading, `true` before one.
        json: String,
    },
    /// A level other than the volume.
    Slider(Slider),
    /// A readout or a fault: its words, and their tint (`good`, `faint` or none).
    Readout {
        key: String,
        label: String,
        text: String,
        tone: &'static str,
    },
}

impl ControlView {
    /// The control's profile key.
    #[cfg(test)]
    pub(crate) fn key(&self) -> &str {
        match self {
            ControlView::Segments { key, .. }
            | ControlView::List { key, .. }
            | ControlView::Switch { key, .. }
            | ControlView::Readout { key, .. } => key,
            ControlView::Slider(slider) => &slider.key,
        }
    }
}

/// One value of a select: the name a write sends, and whether the device reads it.
#[derive(Debug)]
pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) on: bool,
    /// What a press posts, as JSON.
    pub(crate) json: String,
}

/// A level as a slider.
#[derive(Debug)]
pub(crate) struct Slider {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) lo: i64,
    pub(crate) hi: i64,
    pub(crate) raw: Option<i64>,
    /// The centre of a centred level (a balance), which a tap on the value returns to.
    pub(crate) centre: Option<i64>,
    /// The words of the reading, `–` before one.
    pub(crate) text: String,
    /// The words of every step from `lo` to `hi`.
    pub(crate) words: Vec<String>,
    /// The reading's place along the track, as a percentage to two decimals.
    pub(crate) at: Option<String>,
    /// Where the fill starts, as a percentage: the centre of a centred level, else the start.
    pub(crate) from: String,
    /// The labels at its ends: `L` and `R` for a balance.
    pub(crate) ends: Option<(String, String)>,
}

/// What a press of a switch that reads `on` posts, as JSON: the opposite, `true` before a
/// reading.
pub(super) fn opposite(on: Option<bool>) -> String {
    serde_json::json!({ "value": !on.unwrap_or(false) }).to_string()
}

/// The Controls section, in profile order: on an amp its selects and levels (its switches and
/// readouts are the Power section's); elsewhere every control but the knob's, the power button,
/// the meters, the faults (the card lists every active one), a floor a meter's scale reads, and a
/// readout the session answers itself (`auto_off`).
pub(crate) fn section(s: &Snapshot, profile: &Profile, amp: bool) -> Vec<ControlView> {
    let (volume, mute) = knob_controls(profile);
    profile
        .controls
        .iter()
        .filter(|c| Some(c.id) != volume.map(|v| v.id) && Some(c.id) != mute.map(|m| m.id))
        .filter(|c| !(c.kind == ControlKind::Toggle && c.powers_output))
        .filter(|c| !matches!(c.kind, ControlKind::Meter | ControlKind::Fault))
        .filter(|c| !c.auto_off && !profile.sets_a_zero(c))
        .filter(|c| !amp || matches!(c.kind, ControlKind::Select | ControlKind::Level))
        .map(|c| view(s, profile, c))
        .collect()
}

/// `control` in the page's terms.
pub(crate) fn view(s: &Snapshot, profile: &Profile, control: &Control) -> ControlView {
    let key = control.key.clone();
    let label = control.label.clone();
    let raw = s.raw(control.id);
    match control.kind {
        ControlKind::Select => {
            let options: Vec<Choice> = control
                .values
                .iter()
                .map(|(value, name)| Choice {
                    label: name.clone(),
                    on: raw == Some(i64::from(*value)),
                    json: serde_json::json!({ "value": name }).to_string(),
                })
                .collect();
            match options.len() <= SEGMENTS_MAX {
                true => ControlView::Segments {
                    key,
                    label,
                    options,
                },
                false => ControlView::List {
                    key,
                    label,
                    options,
                },
            }
        }
        ControlKind::Toggle => {
            let on = s.message(control.id).map(|m| control.reads_true(&m.value));
            ControlView::Switch {
                key,
                label,
                on,
                json: opposite(on),
            }
        }
        ControlKind::Level => {
            let (lo, hi) = control.range.unwrap_or((0, 0));
            let words =
                |raw: i64| minus(&units::text(control, raw).unwrap_or_else(|| raw.to_string()));
            let texts: Vec<String> = (lo..=hi).map(words).collect();
            let place =
                |at: i64| format!("{:.2}", 100.0 * (at - lo) as f64 / (hi - lo).max(1) as f64);
            ControlView::Slider(Slider {
                key,
                label,
                lo,
                hi,
                raw,
                centre: control.center,
                at: raw.map(place),
                from: control.center.map_or_else(|| "0".to_string(), place),
                text: raw.map_or_else(|| "–".to_string(), words),
                words: texts,
                ends: control.center.map(|_| ("L".to_string(), "R".to_string())),
            })
        }
        ControlKind::Readout | ControlKind::Fault | ControlKind::Meter => {
            let (text, tone) = readout(s, profile, control);
            ControlView::Readout {
                key,
                label,
                text,
                tone,
            }
        }
    }
}

/// A readout's words and tint: a bool as `connected`/`not connected`, `locked`/`unlocked` or
/// `yes`/`no` by its key, its `only_while` gate applied ([`units::shows_true`]); anything else
/// in the profile's words. `–` before it has reported.
fn readout(s: &Snapshot, profile: &Profile, control: &Control) -> (String, &'static str) {
    let Some(m) = s.message(control.id) else {
        return ("–".to_string(), "");
    };
    let is_bool = matches!(m.value, Value::Bool(_)) || control.value_type == ValueType::Bool;
    if !is_bool || control.kind == ControlKind::Fault {
        // A value with a name of its own (a sample rate) reads as the name alone.
        let named = m
            .value
            .as_u64()
            .and_then(|v| u8::try_from(v).ok())
            .and_then(|v| control.value_name(v));
        return match named {
            Some(name) => (minus(name), ""),
            None => (minus(&describe(profile, m).display), ""),
        };
    }
    let on = units::shows_true(profile, control, control.reads_true(&m.value), |gate| {
        s.message(gate.id).map(|g| gate.reads_true(&g.value))
    });
    let words = control.flag_words();
    let tone = match (words, on) {
        (FlagWords::Connected | FlagWords::Locked, true) => "good",
        (FlagWords::Connected, false) => "faint",
        _ => "",
    };
    let word = match on {
        true => words.on(),
        false => words.off(),
    };
    (word.to_string(), tone)
}
