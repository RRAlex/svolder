// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset list's keys for the computer: `Enter` on the computer's side puts the focused
//! preset on the computer, and `Enter` on the device's side, on the preset that is on the
//! computer, asks before it moves it to the DAC.
//!
//! The same preset is never left on both, as `svolder host eq apply` and `svolder eq load` keep
//! it: putting the preset that is on the DAC on the computer asks, and flattens the DAC when the
//! answer is yes; loading the one on the computer asks, and takes it off the computer first.
//! Joined to a daemon the computer's recall is a request to it by the preset's name: the daemon
//! says if a connected device plays it too, and the question is asked then; with no daemon the
//! loop's own controller takes the correction, and the DAC is looked at here.

use svolder_ipc::HostEqOp;
use svolder_profiles::tone::preset::{self, BothSides, Preset, host_correction};

use crate::tui::app::{App, Intent};

use super::{Entry, Question, Step, entries, load, on};

/// `Enter` on the computer's side: put `entry` on the computer. A preset with no curve for the
/// computer is refused, saying why; one the DAC plays too asks first.
pub(super) fn recall(app: &mut App, entry: &Entry) -> Vec<Intent> {
    if let Err(why) = host_correction(&entry.preset) {
        app.warn(why);
        return Vec::new();
    }
    if let Some(why) = app.host.computer.unavailable().map(str::to_string) {
        app.warn(why);
        return Vec::new();
    }
    if app.host.daemon_owns() {
        app.presets.preset_list = None;
        return vec![by_name(app, &entry.preset.name, false)];
    }
    let on_dac = app.profile.eq.clone().and_then(|eq| {
        let all = entries(app, &eq);
        on(app, &eq, &all)
    });
    if preset::both_sides(&entry.preset, on_dac.as_deref()) != BothSides::Free {
        ask(app, Question::MoveToComputer(entry.preset.name.clone()));
        return Vec::new();
    }
    app.presets.preset_list = None;
    apply(app, &entry.preset)
}

/// The daemon's request to put the preset `name` on the computer, `confirm` once the question
/// has been answered yes.
fn by_name(app: &App, name: &str, confirm: bool) -> Intent {
    Intent::eq(HostEqOp::Preset {
        name: name.to_string(),
        device: Some(app.session.device_name.clone()),
        confirm,
    })
}

/// Put `preset` on the computer.
fn apply(app: &mut App, preset: &Preset) -> Vec<Intent> {
    match host_correction(preset) {
        Ok(correction) => vec![Intent::eq(HostEqOp::Apply {
            name: preset.name.clone(),
            origin: preset
                .source
                .as_ref()
                .and_then(|source| source.origin.clone()),
            correction,
            sink: None,
        })],
        Err(why) => {
            app.warn(why);
            Vec::new()
        }
    }
}

/// Ask `question` in the box.
pub(super) fn ask(app: &mut App, question: Question) {
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Asking(question);
    }
}

/// The daemon says a connected device plays the preset `name` too: the list opens on it with the
/// question, whether the list was closed since the request or not.
pub(crate) fn asked(app: &mut App, name: &str) {
    let row = app
        .preset_spec()
        .and_then(|eq| {
            entries(app, &eq)
                .iter()
                .position(|entry| preset::same_name(&entry.preset.name, name))
        })
        .unwrap_or(0);
    app.presets.preset_list = Some(super::PresetList {
        row,
        step: Step::Asking(Question::MoveToComputer(name.to_string())),
    });
}

/// `y` to the DAC's move to the computer: flatten the DAC, then apply; joined, the daemon
/// flattens the devices that play it, on a request that says yes.
pub(super) fn move_to_computer(app: &mut App, name: &str) -> Vec<Intent> {
    if app.host.daemon_owns() {
        app.presets.preset_list = None;
        return vec![by_name(app, name, true)];
    }
    let Some(eq) = app.preset_spec() else {
        return Vec::new();
    };
    let Some(entry) = entries(app, &eq)
        .into_iter()
        .find(|entry| entry.preset.name == name)
    else {
        return Vec::new();
    };
    // A preset with no curve for the computer is refused before the DAC is touched.
    if let Err(why) = host_correction(&entry.preset) {
        app.warn(why);
        return Vec::new();
    }
    app.presets.preset_list = None;
    let mut intents = app.flatten_dac();
    app.presets.computer_flattened = (!intents.is_empty()).then(|| name.to_string());
    intents.extend(apply(app, &entry.preset));
    intents
}

/// `y` to the computer's move to the DAC: take it off the computer first; the DAC's load goes
/// out once that has succeeded ([`moved`]), so a refused removal leaves the preset where it was.
pub(super) fn move_to_dac(app: &mut App, name: &str) -> Vec<Intent> {
    app.presets.preset_list = None;
    vec![Intent::MoveToDac(name.to_string())]
}

/// The computer EQ is off: load the preset `name` onto the DAC.
pub(crate) fn moved(app: &mut App, name: &str) -> Vec<Intent> {
    let Some(eq) = app.profile.eq.clone() else {
        return Vec::new();
    };
    match entries(app, &eq)
        .into_iter()
        .find(|entry| entry.preset.name == name)
    {
        Some(entry) => load(app, &eq, &entry.preset),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
