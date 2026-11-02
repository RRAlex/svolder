// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device's own presets: `preset list` and `preset apply`, and the slots `set recall` and
//! `set save` send.
//!
//! These are not the tone control's presets of `eq list` and `eq load`: a slot lives on the
//! device and is recalled or saved by a command with no value, and a built-in setting is a row
//! of raw steps from the device's profile. [`svolder_profiles::device_presets`] plans the
//! writes and finds the setting a device is on; this module reaches the device, asks the
//! question a save needs and says what happened.

use std::io::IsTerminal;
use std::process::ExitCode;

use svolder_ipc::{Config, ValueObject};
use svolder_profiles::device_presets;
use svolder_profiles::{Action, Profile, ProfileError, roles};

use crate::backend::{Backend, Failure, Globals};
use crate::recovery::Kind;

use super::eq::{current_raw, partial_failure_line, write_count};
use super::presets::configured_profile;
use super::{ask, nothing_changed, open, opened};

/// What `eq load` and `eq list` add on a device that has no tone control but has presets of its
/// own: the verb that lists them.
pub(super) const POINTER: &str = "svolder preset list lists its presets";

/// What a text `preset list` ends with: the two verbs that use what it listed.
const CLOSING: &str =
    "svolder set recall N recalls a slot; svolder preset apply NAME applies a built-in";

/// One row of `preset list`: a slot or a built-in setting, and for a setting whether the device
/// reads that setting's bands.
#[derive(serde::Serialize)]
struct Row {
    name: String,
    /// The slot's number, for a slot.
    slot: Option<u8>,
    built_in: bool,
    /// Whether the device is on this setting: `None` for a slot, which the device never says,
    /// and for every row while there are no readings, or a band is unread, to judge by.
    on: Option<bool>,
}

/// List the device's presets: its slots, then its built-in settings, one text line or JSON
/// object each.
///
/// The profile is the one the configured device names, read without connecting, as `eq list`
/// reads it; with a daemon that holds the device it is the daemon's, and the setting the bands
/// read is marked `on`. A device with neither slots nor settings is said to have none.
pub async fn preset_list(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    if let Err(failure) = crate::backend::known_transport(g.transport) {
        return Ok(failure.report());
    }
    let config = Config::load().map_err(|e| e.to_string())?;
    crate::backend::print_warnings(&config);
    if crate::backend::asks_a_daemon(g.transport)
        && crate::backend::launch::probe(g, &config).await.is_some()
        && let Ok(mut backend) = opened(g, true).await
    {
        let outcome = match backend.state().await {
            Ok(values) => print_list(backend.profile(), Some(&values), json),
            Err(failure) => Ok(failure.report()),
        };
        backend.close().await;
        return outcome;
    }
    match configured_profile(g, &config, "its presets are not listed") {
        Some(profile) => print_list(&profile, None, json),
        None => Ok(Failure::of(
            Kind::Usage,
            "no profile is known for the device, so its presets cannot be listed; connect to it \
             once (svolder status) or name its profile in the configuration",
        )
        .report()),
    }
}

/// Print the rows of `profile`'s presets, marked against `values` when there are any.
fn print_list(
    profile: &Profile,
    values: Option<&[ValueObject]>,
    json: bool,
) -> Result<ExitCode, String> {
    let rows = rows(profile, values)?;
    if json {
        for row in &rows {
            println!("{}", serde_json::to_string(row).map_err(|e| e.to_string())?);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let width = rows.iter().map(|row| row.name.chars().count()).max();
    for row in &rows {
        let mark = match row.built_in {
            true => "built-in",
            false => "slot",
        };
        let on = match row.on {
            Some(true) => "  on",
            _ => "",
        };
        println!("{:<w$}  {mark}{on}", row.name, w = width.unwrap_or(0));
    }
    println!("{CLOSING}");
    Ok(ExitCode::SUCCESS)
}

/// The rows of `profile`'s presets: a row for each slot of the recall action, then for each
/// built-in setting.
fn rows(profile: &Profile, values: Option<&[ValueObject]>) -> Result<Vec<Row>, String> {
    if !device_presets::offered(profile) {
        return Err(no_presets(profile));
    }
    let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
    let mut rows: Vec<Row> = (1..=slots)
        .map(|slot| Row {
            name: device_presets::slot_name(slot),
            slot: Some(slot),
            built_in: false,
            on: None,
        })
        .collect();
    // The device is on a setting only when every band it sets has been read; with a band unread
    // there is nothing to judge by, which is no setting being on and not a "no".
    let on = values.and_then(|values| {
        let raw = current_raw(values);
        let table = profile.builtin_settings.as_ref()?;
        let read = table.keys.iter().all(|key| {
            profile
                .control_by_key(key)
                .is_some_and(|c| raw(c.id).is_some())
        });
        read.then(|| device_presets::matching(profile, raw).map(|setting| setting.name.clone()))
    });
    for setting in profile
        .builtin_settings
        .iter()
        .flat_map(|table| &table.settings)
    {
        rows.push(Row {
            name: setting.name.clone(),
            slot: None,
            built_in: true,
            on: on
                .as_ref()
                .map(|on| on.as_deref() == Some(setting.name.as_str())),
        });
    }
    Ok(rows)
}

/// The words for a device that has no presets of its own.
fn no_presets(profile: &Profile) -> String {
    ProfileError::NoDevicePresets {
        device: profile.display.clone(),
        tone_control: profile.eq.is_some(),
    }
    .to_string()
}

/// Reach the device and put it on the built-in setting `name`: the writes
/// [`device_presets::plan`] plans from what the bands read, each sent as the table says and
/// waited for, in order. A device already on the setting is sent nothing. A direct session is
/// closed before returning, on every path.
pub async fn preset_apply(g: &Globals<'_>, name: &str) -> Result<ExitCode, String> {
    let mut backend = match open(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    let outcome = apply_on(&mut backend, name).await;
    backend.close().await;
    outcome
}

/// [`preset_apply`]'s body, once a backend is open.
async fn apply_on(backend: &mut Backend, name: &str) -> Result<ExitCode, String> {
    let values = match backend.state().await {
        Ok(values) => values,
        Err(failure) => return Ok(failure.report()),
    };
    let plan = device_presets::plan(backend.profile(), name, current_raw(&values))
        .map_err(|e| e.to_string())?;
    if plan.writes.is_empty() {
        eprintln!("svolder: {} is already on", plan.name);
        return Ok(ExitCode::SUCCESS);
    }
    let total = plan.writes.len();
    for (sent, write) in plan.writes.iter().enumerate() {
        let raw = write.raw.to_string();
        if let Err(err) = backend.raw(&write.id.to_string(), "u8", &raw, false).await {
            eprintln!("svolder: {}", partial_failure_line(sent, total));
            return Ok(err.failure().report());
        }
    }
    eprintln!("svolder: {} on ({})", plan.name, write_count(total));
    Ok(ExitCode::SUCCESS)
}

/// Send `action` on the slot `value` names, and say what was sent.
///
/// An action has no echo, so nothing is waited for and no reading is printed: the answer is a
/// line on stderr, and with `json` one object, `{"key":"recall","slot":2,"sent":true}`; a refusal
/// for want of `--yes` is one too, `sent` false with the words as `error`. A slot
/// the action does not have is a usage error before anything is sent. An action that
/// overwrites what the device holds (a save) is asked about first: on a terminal with
/// `[y/N]`, a no leaving the device alone; off a terminal it is refused unless `yes` has
/// already answered.
pub(super) async fn send_action(
    backend: &mut Backend,
    action: &Action,
    value: &str,
    yes: bool,
    json: bool,
) -> Result<ExitCode, String> {
    let terminal = std::io::stdin().is_terminal();
    send_action_asking(backend, action, value, json, (yes, terminal, ask)).await
}

/// [`send_action`] with the answer given as `(yes, terminal, ask)`: whether `--yes` was passed,
/// whether there is a terminal to ask on, and how to ask it.
async fn send_action_asking(
    backend: &mut Backend,
    action: &Action,
    value: &str,
    json: bool,
    (yes, terminal, ask): (bool, bool, impl FnOnce(&str) -> std::io::Result<bool>),
) -> Result<ExitCode, String> {
    let Some(slot) = device_presets::parse_slot(value, action.slots) else {
        return Err(ProfileError::BadSlot {
            key: action.key.clone(),
            slots: action.slots,
        }
        .to_string());
    };
    let words = Words::of(action, slot, &backend.profile().display);
    if action.overwrites {
        match overwrite_answer(&words, yes, terminal, ask) {
            Ok(true) => {}
            Ok(false) => return Ok(nothing_changed()),
            Err(refused) => {
                if json {
                    print_sent(&Sent {
                        key: &action.key,
                        slot,
                        sent: false,
                        error: Some(&refused),
                    })?;
                }
                return Err(refused);
            }
        }
    }
    if let Err(err) = backend.set(&action.key, &slot.to_string(), 1, false).await {
        return Ok(err.failure().report());
    }
    eprintln!("svolder: {}", words.done);
    if json {
        print_sent(&Sent {
            key: &action.key,
            slot,
            sent: true,
            error: None,
        })?;
    }
    Ok(ExitCode::SUCCESS)
}

/// What `set --json` prints for an action: `sent` says whether it went out, and `error` why not
/// when it was refused before.
#[derive(serde::Serialize)]
struct Sent<'a> {
    key: &'a str,
    slot: u8,
    sent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

/// Print `sent` as one JSON line on stdout.
fn print_sent(sent: &Sent) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(sent).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Whether the user said yes to the question of an action that overwrites: `yes` is an answer
/// already, a terminal is asked, and anything else is refused with the flag that answers
/// without one.
fn overwrite_answer(
    words: &Words,
    yes: bool,
    terminal: bool,
    ask: impl FnOnce(&str) -> std::io::Result<bool>,
) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    if !terminal {
        return Err(words.refusal.clone());
    }
    ask(&words.question).map_err(|e| e.to_string())
}

/// What is said about one action sent on one slot.
struct Words {
    /// The question that asks before an overwrite, ending in its `[y/N]`.
    question: String,
    /// What a run that cannot ask says, with the flag that answers.
    refusal: String,
    /// What is said once it is sent.
    done: String,
}

impl Words {
    /// The words for `action` on `slot` of the device called `device`: a recall and a save have
    /// their own, any other action is named by its label.
    fn of(action: &Action, slot: u8, device: &str) -> Words {
        let preset = device_presets::slot_name(slot).to_lowercase();
        match action.key.to_ascii_lowercase().as_str() {
            "recall" => Words {
                question: format!("Recall {preset} on {device}? [y/N]"),
                refusal: format!(
                    "recalling {preset} overwrites what {device} has set; add --yes to recall \
                     without asking"
                ),
                done: format!("recalled {preset}"),
            },
            "save" => Words {
                question: format!("Save the bands to {preset} on {device}? [y/N]"),
                refusal: format!(
                    "saving to {preset} overwrites what {device} holds there; add --yes to save \
                     without asking"
                ),
                done: format!("saved the bands to {preset}"),
            },
            _ => Words {
                question: format!("{} {slot} on {device}? [y/N]", action.label),
                refusal: format!(
                    "{} {slot} overwrites what {device} holds; add --yes to send it without asking",
                    action.key
                ),
                done: format!("sent {} {slot}", action.key),
            },
        }
    }
}

#[cfg(test)]
mod tests;
