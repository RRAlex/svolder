// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset list's keys for the computer EQ: `h` puts the focused preset on the computer, or
//! takes it off when it is the one there, and `Enter` on the preset that is on the computer asks
//! before it moves it to the DAC.
//!
//! The same preset is never left on both, as `svolder host eq apply` and `svolder eq load` keep
//! it: putting the preset that is on the DAC on the computer asks, and flattens the DAC when the
//! answer is yes; loading the one on the computer asks, and takes it off the computer first. The
//! work itself is the loop's, off the UI thread, and its answer comes back as a
//! [`Done`](super::Done).

use svolder_host::eq::HostEqState;
use svolder_profiles::tone::preset::{self, Preset};

use crate::commands::host_eq::{host_correction, lowered_note};
use crate::recovery::Failure;
use crate::tui::app::{App, Intent};

use super::{Entry, Question, Step, entries, load, on};

/// What `h` on a preset that is still being changed on the computer is answered with.
const BUSY: &str = "the computer EQ is still changing";

/// What `h` on a built-in is answered with: its bands are shaped on a device's tone control.
fn built_in(name: &str) -> String {
    format!("{name} is built in: save a curve as a new preset to put it on the computer")
}

/// Whether the preset `name` is the one on the computer.
pub(super) fn on_computer(app: &App, name: &str) -> bool {
    app.presets
        .computer
        .as_deref()
        .is_some_and(|there| preset::same_name(there, name))
}

/// `h` on `entry`: take it off the computer when it is there, ask first when it is on the DAC,
/// else put it on the computer. A built-in is refused, and so is a computer where the EQ cannot
/// run, saying why.
pub(super) fn key(app: &mut App, entry: &Entry) -> Vec<Intent> {
    let name = entry.preset.name.clone();
    if entry.builtin {
        app.warn(built_in(&name));
        return Vec::new();
    }
    if app.presets.computer_busy {
        app.warn(BUSY);
        return Vec::new();
    }
    if on_computer(app, &name) {
        app.presets.computer_busy = true;
        return vec![Intent::HostRemove { then_load: None }];
    }
    if let Some(why) = app.presets.computer_off.clone() {
        app.warn(why);
        return Vec::new();
    }
    let on_dac = app.profile.eq.as_ref().is_some_and(|eq| {
        let all = entries(app, eq);
        on(app, eq, &all).is_some_and(|there| preset::same_name(&there, &name))
    });
    if on_dac {
        ask(app, Question::MoveToComputer(name));
        return Vec::new();
    }
    apply(app, &entry.preset)
}

/// `Enter` or `Space` on the device screen's Host EQ row: take the computer EQ off while it is
/// on; when off, put the last one back, built from the filters its state kept, and with none to
/// put back, say how to make one. A computer where the EQ cannot run says why, and so does one
/// still changing.
pub(crate) fn toggle(app: &mut App) -> Vec<Intent> {
    if let Some(why) = app.presets.computer_off.clone() {
        app.warn(why);
        return Vec::new();
    }
    if app.presets.computer_busy {
        app.warn(BUSY);
        return Vec::new();
    }
    let Some(state) = app.presets.host_eq.clone() else {
        app.notify(crate::tui::host::HOST_EQ_HINT);
        return Vec::new();
    };
    app.presets.computer_busy = true;
    match state.on {
        true => vec![Intent::HostRemove { then_load: None }],
        false => vec![Intent::HostApply {
            correction: state.correction(),
            name: state.name,
            origin: state.origin,
        }],
    }
}

/// Put `preset` on the computer.
fn apply(app: &mut App, preset: &Preset) -> Vec<Intent> {
    let correction = match host_correction(preset) {
        Ok(correction) => correction,
        Err(why) => {
            app.warn(why);
            return Vec::new();
        }
    };
    app.presets.computer_busy = true;
    vec![Intent::HostApply {
        name: preset.name.clone(),
        origin: preset
            .source
            .as_ref()
            .and_then(|source| source.origin.clone()),
        correction,
    }]
}

/// Ask `question` in the box.
pub(super) fn ask(app: &mut App, question: Question) {
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.step = Step::Asking(question);
    }
}

/// `y` to the DAC's move to the computer: flatten the DAC, then apply.
pub(super) fn move_to_computer(app: &mut App, name: &str) -> Vec<Intent> {
    let Some(eq) = app.profile.eq.clone() else {
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
    let mut intents = app.flatten();
    app.presets.computer_flattened = !intents.is_empty();
    intents.extend(apply(app, &entry.preset));
    intents
}

/// `y` to the computer's move to the DAC: take it off the computer, then load it.
pub(super) fn move_to_dac(app: &mut App, name: &str) -> Vec<Intent> {
    app.presets.computer_busy = true;
    vec![Intent::HostRemove {
        then_load: Some(name.to_string()),
    }]
}

/// The computer EQ was applied, or could not be.
pub(crate) fn applied(
    app: &mut App,
    name: &str,
    stated: f64,
    result: Result<HostEqState, Failure>,
) -> Vec<Intent> {
    app.presets.computer_busy = false;
    let flattened = std::mem::take(&mut app.presets.computer_flattened);
    match result {
        Ok(state) => {
            let lowered = lowered_note(stated, state.preamp_db);
            app.take_host_eq(Some(state));
            match lowered {
                Some(note) => app.notify(format!("{name} is on the computer; {note}")),
                None => app.notify(format!("{name} is on the computer")),
            }
        }
        Err(mut failure) => {
            if flattened {
                failure.next = format!(
                    "The DAC was flattened; put the curve back with Enter on {name} in the \
                     presets box, or `svolder eq load {name}`. {}",
                    failure.next
                );
            }
            app.notify_failure(&failure);
        }
    }
    Vec::new()
}

/// The computer EQ was removed, or could not be; with `then_load`, the preset of that name is
/// loaded onto the DAC once it is.
pub(crate) fn removed(
    app: &mut App,
    then_load: Option<String>,
    result: Result<(), Failure>,
) -> Vec<Intent> {
    app.presets.computer_busy = false;
    if let Err(failure) = result {
        app.notify_failure(&failure);
        return Vec::new();
    }
    app.presets.computer = None;
    if let Some(state) = app.presets.host_eq.as_mut() {
        state.on = false;
    }
    let Some(name) = then_load else {
        app.notify("computer EQ removed");
        return Vec::new();
    };
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
