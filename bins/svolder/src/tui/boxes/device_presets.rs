// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The list of the presets a device keeps itself, in a box over the device screen: the slots the
//! device recalls and saves, then the built-in settings its profile names.
//!
//! Nothing here writes: a recall, a save or a built-in answers with the [`Intent`]s the loop
//! sends. A slot is never marked as the one on, since the device does not say which it holds; a
//! built-in is marked while the bands read exactly its steps.

use crossterm::event::KeyCode;
use svolder_profiles::device_presets;
use svolder_profiles::{Action, BuiltinSetting, Profile, roles};
use svolder_proto::FunctionId;

use crate::tui::app::{App, Intent, stepped};

/// What the header over the built-ins says.
pub const BUILT_IN: &str = "Built in";

/// The device's presets list, while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceList {
    /// The focused row, as an index into [`rows`].
    pub row: usize,
    /// What the box is showing: the list, or the question a save asks.
    pub step: Step,
}

/// What the box is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The rows themselves, to move through and act on.
    Choosing,
    /// A `[y/N]` question.
    Asking(Question),
}

/// What the box asks before it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Whether to write the bands over the device slot of this number.
    Save(u8),
}

/// One row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A device slot, by number.
    Slot(u8),
    /// The header over the built-ins, which takes no focus.
    Header,
    /// A built-in setting, by its place in the profile's table.
    Setting(usize),
}

impl Row {
    /// What the row says: the slot's name, the header's words or the setting's name.
    pub fn label(&self, profile: &Profile) -> String {
        match *self {
            Row::Slot(slot) => device_presets::slot_name(slot),
            Row::Header => BUILT_IN.to_string(),
            Row::Setting(at) => settings(profile)
                .get(at)
                .map(|setting| setting.name.clone())
                .unwrap_or_default(),
        }
    }

    /// Whether the focus can rest on the row.
    fn focusable(&self) -> bool {
        !matches!(self, Row::Header)
    }
}

/// The built-in settings the profile names, in its order.
fn settings(profile: &Profile) -> &[BuiltinSetting] {
    profile
        .builtin_settings
        .as_ref()
        .map_or(&[], |table| table.settings.as_slice())
}

/// The rows of the list: a row for each slot of the recall action, then the header and a row
/// for each built-in setting.
pub fn rows(profile: &Profile) -> Vec<Row> {
    let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
    let mut rows: Vec<Row> = (1..=slots).map(Row::Slot).collect();
    let count = settings(profile).len();
    if count > 0 {
        rows.push(Row::Header);
        rows.extend((0..count).map(Row::Setting));
    }
    rows
}

/// What `Enter` on a slot says it did.
fn recalled(slot: u8) -> String {
    format!("Recalled preset {slot}")
}

/// What `y` on the save question says it did.
fn saved(slot: u8) -> String {
    format!("Saved to preset {slot}")
}

/// What `Enter` on a built-in says it did.
fn applied(name: &str) -> String {
    format!("{name} on")
}

/// What `Enter` on a built-in the bands already read says.
fn already_on(name: &str) -> String {
    format!("{name} is already on")
}

/// What `u` on a built-in is answered with.
fn not_a_slot(name: &str) -> String {
    format!("{name} is built in; u saves on a device slot")
}

/// The raw step `id` reads as the list sees it: nothing while a recall sent from here has not
/// been answered by the function ([`Readings::unreported`](crate::tui::app::Readings)), since the
/// reading is from before it.
fn reads(app: &App, id: FunctionId) -> Option<i64> {
    match app.readings.unreported.contains(&id) {
        true => None,
        false => app.raw_of(id),
    }
}

/// The raw step `id` is on its way to, for a plan: the step a write asked for last while the
/// device has not answered it, and otherwise what the list sees it read.
fn planned(app: &App, id: FunctionId) -> Option<i64> {
    match app.readings.pending.get(&id) {
        Some(pending) => Some(i64::from(pending.step)),
        None => reads(app, id),
    }
}

/// The place in the profile's table of the built-in setting the bands read, if any: none while a
/// band is unread, which a recall sent from here leaves them until each reports.
pub fn on(app: &App) -> Option<usize> {
    let found = device_presets::matching(&app.profile, |id| reads(app, id))?;
    settings(&app.profile)
        .iter()
        .position(|setting| std::ptr::eq(setting, found))
}

/// Open the list, with the built-in the bands read focused where one is on and the first slot
/// otherwise; nothing on a device with no presets of its own.
pub(in crate::tui) fn open(app: &mut App) -> Vec<Intent> {
    if !device_presets::offered(&app.profile) {
        return Vec::new();
    }
    let rows = rows(&app.profile);
    let row = on(app)
        .and_then(|at| rows.iter().position(|row| *row == Row::Setting(at)))
        .or_else(|| rows.iter().position(Row::focusable))
        .unwrap_or(0);
    app.presets.device_list = Some(DeviceList {
        row,
        step: Step::Choosing,
    });
    Vec::new()
}

/// What a key does while the list is open: the list's own keys, or the answer to the question a
/// save asks, and every other key swallowed — the box is answered, and what it covers is not
/// reached through it.
pub(in crate::tui) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(list) = app.presets.device_list.as_ref() else {
        return Vec::new();
    };
    match list.step.clone() {
        Step::Choosing => choose(app, code),
        Step::Asking(question) => answer(app, code, &question),
    }
}

/// The list's own keys: `↑↓` move over the rows that take focus, `Enter` recalls the focused slot
/// or applies the focused built-in and closes the list, `u` asks before it saves the bands over
/// the focused slot and refuses a built-in, and `Esc` closes the list.
fn choose(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let rows = rows(&app.profile);
    let Some(list) = app.presets.device_list.as_ref() else {
        return Vec::new();
    };
    let row = list.row.min(rows.len().saturating_sub(1));
    match code {
        KeyCode::Esc => {
            app.presets.device_list = None;
            Vec::new()
        }
        KeyCode::Up => move_to(app, &rows, row, -1),
        KeyCode::Down => move_to(app, &rows, row, 1),
        KeyCode::Enter => match rows.get(row) {
            Some(Row::Slot(slot)) => recall(app, *slot),
            Some(Row::Setting(at)) => apply(app, *at),
            _ => Vec::new(),
        },
        KeyCode::Char('u') => match rows.get(row) {
            Some(Row::Slot(slot)) => {
                if let Some(list) = app.presets.device_list.as_mut() {
                    list.step = Step::Asking(Question::Save(*slot));
                }
                Vec::new()
            }
            Some(row @ Row::Setting(_)) => {
                let name = row.label(&app.profile);
                app.warn(not_a_slot(&name));
                Vec::new()
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Move the focus from `row` one row `delta` on, over the header, and stay where the list ends.
fn move_to(app: &mut App, rows: &[Row], row: usize, delta: isize) -> Vec<Intent> {
    let mut at = row;
    loop {
        let next = stepped(at, delta, rows.len());
        if next == at {
            return Vec::new();
        }
        at = next;
        if rows.get(at).is_some_and(Row::focusable) {
            break;
        }
    }
    if let Some(list) = app.presets.device_list.as_mut() {
        list.row = at;
    }
    Vec::new()
}

/// Recall slot `slot` and close the list. The functions the recall changes are unreported from
/// here until each says what the device now holds.
fn recall(app: &mut App, slot: u8) -> Vec<Intent> {
    app.presets.device_list = None;
    let Some(action) = roles::recall_action(&app.profile) else {
        return Vec::new();
    };
    let intent = action_intent(action, slot);
    let changed: Vec<FunctionId> = action
        .changes
        .iter()
        .filter_map(|key| app.profile.control_by_key(key))
        .map(|control| control.id)
        .collect();
    // What an earlier write asked of these functions is not what the device holds after the
    // recall, so no plan starts from it.
    for id in &changed {
        app.readings.pending.remove(id);
    }
    app.readings.unreported.extend(changed);
    app.notify(recalled(slot));
    vec![intent]
}

/// Take back what [`recall`] assumed when `intent`, a recall, could not be sent: the device holds
/// what it held, so the readings of the functions it changes can be trusted again. Any other
/// intent changes nothing.
pub(in crate::tui) fn unsent(app: &mut App, intent: &Intent) {
    let Intent::Set { key, .. } = intent else {
        return;
    };
    let Some(action) =
        roles::recall_action(&app.profile).filter(|a| a.key.eq_ignore_ascii_case(key))
    else {
        return;
    };
    for key in &action.changes {
        if let Some(control) = app.profile.control_by_key(key) {
            app.readings.unreported.remove(&control.id);
        }
    }
}

/// The write that runs `action` on slot `slot`: a `set` of the action's key to the slot's number.
fn action_intent(action: &Action, slot: u8) -> Intent {
    Intent::Set {
        key: action.key.clone(),
        value: slot.to_string(),
        step: 0,
    }
}

/// Apply the built-in setting at `at` and close the list: the plan's writes, in its order, each
/// one noted as pending so a second press before the device answers plans from it.
fn apply(app: &mut App, at: usize) -> Vec<Intent> {
    app.presets.device_list = None;
    let name = Row::Setting(at).label(&app.profile);
    let plan = match device_presets::plan(&app.profile, &name, |id| planned(app, id)) {
        Ok(plan) => plan,
        Err(e) => {
            app.warn(e.to_string());
            return Vec::new();
        }
    };
    if plan.writes.is_empty() {
        app.notify(already_on(&plan.name));
        return Vec::new();
    }
    let mut intents = Vec::with_capacity(plan.writes.len());
    for write in &plan.writes {
        if let Some(control) = app.profile.control_by_key(&write.key) {
            let id = control.id;
            app.expect_raw(id, write.raw);
        }
        intents.push(Intent::Raw {
            id: write.id.to_string(),
            value_type: "u8".to_string(),
            value: write.raw.to_string(),
        });
    }
    app.notify(applied(&plan.name));
    intents
}

/// What a key does while the save question is open: `y` saves, `n`, `Esc` and `Enter` — the
/// question's default is no — go back to the list with nothing written, and every other key is
/// swallowed.
fn answer(app: &mut App, code: KeyCode, question: &Question) -> Vec<Intent> {
    match code {
        KeyCode::Char('y' | 'Y') => {}
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char('n' | 'N') => {
            back_to_list(app);
            return Vec::new();
        }
        _ => return Vec::new(),
    }
    back_to_list(app);
    let Question::Save(slot) = *question;
    let Some(action) = roles::save_action(&app.profile) else {
        return Vec::new();
    };
    let intent = action_intent(action, slot);
    app.notify(saved(slot));
    vec![intent]
}

/// Go back from the question to the list itself.
fn back_to_list(app: &mut App) {
    if let Some(list) = app.presets.device_list.as_mut() {
        list.step = Step::Choosing;
    }
}

#[cfg(test)]
mod tests;
