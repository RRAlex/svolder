// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the search box and the file prompt hand over: a correction to keep as a preset.
//!
//! A file's correction is read here, so a name no preset carries saves it at once, with the new
//! row focused and not loaded, and a name that is taken opens the name prompt prefilled
//! `NAME 2`. An AutoEq measurement is the daemon's to download and save: the name is settled
//! first (a taken one opens the prompt prefilled `NAME (source)`), then the daemon is asked to
//! import it, and the list shows the preset when the daemon says it is saved.

use svolder_ipc::{AutoEqSaved, AutoEqTarget, AutoEqVariant};
use svolder_profiles::tone::preset::{self, Imported, Preset};

use crate::tui::app::{App, Intent};

use super::{NameLine, PresetList, Step, entries};

/// A correction waiting for the name the user gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// A file's correction, read and parsed.
    File {
        /// The correction.
        imported: Imported,
        /// How many lines of its source the parser left out.
        skipped: usize,
    },
    /// An AutoEq measurement the daemon downloads once it has its name.
    AutoEq(AutoEqVariant),
}

/// What a preset the daemon saved says when the presets are read again: the line of the toast
/// and the row to focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    /// The preset's name, as the daemon saved it.
    pub name: String,
    /// How many filters the source had.
    pub filters: usize,
    /// How many lines of the source the parser left out.
    pub skipped: usize,
}

/// A file's correction `imported` under the preset name `name`: saved if the name is free, else
/// asked for again with another name filled in.
pub(super) fn ready(
    app: &mut App,
    name: String,
    imported: Imported,
    skipped: usize,
) -> Vec<Intent> {
    let name = name.trim().to_string();
    if !taken(app, &name) {
        return save(app, &name, imported, skipped);
    }
    let text = (2u32..)
        .map(|n| format!("{name} {n}"))
        .find(|candidate| !taken(app, candidate))
        .unwrap_or_else(|| name.clone());
    ask_for_name(app, &name, text, Pending::File { imported, skipped })
}

/// Open the name prompt prefilled with `text`, for the import `pending` whose first name `name`
/// is taken.
fn ask_for_name(app: &mut App, name: &str, text: String, pending: Pending) -> Vec<Intent> {
    let text: String = text.chars().take(preset::NAME_MAX).collect();
    if let Some(list) = app.presets.preset_list.as_mut() {
        let mut line = NameLine::new(None, text);
        line.import = Some(Box::new(pending));
        list.step = Step::Naming(line);
    }
    if let Some(why) = refusal(app, name) {
        app.warn(format!("{why}; pick another name"));
    }
    Vec::new()
}

/// Whether a preset goes by `name` already, or may not: a built-in's, `custom`, or a preset's
/// the user already has. A name refused for its length or a control character is not taken:
/// [`ready`] cuts the name it settles on to [`preset::NAME_MAX`].
fn taken(app: &App, name: &str) -> bool {
    matches!(
        refusal(app, name),
        Some(
            preset::NameRefusal::BuiltIn(_)
                | preset::NameRefusal::Reserved(_)
                | preset::NameRefusal::Taken(_)
        )
    )
}

/// Why `name` cannot be an imported preset's name, if it cannot.
fn refusal(app: &App, name: &str) -> Option<preset::NameRefusal> {
    preset::new_name(
        name,
        app.presets.saved.iter().map(|p| p.name.as_str()),
        None,
    )
    .err()
}

/// The name the name prompt's `Enter` settled on for the import it is for.
pub(super) fn named(app: &mut App, name: &str, pending: Pending) -> Vec<Intent> {
    match pending {
        Pending::File { imported, skipped } => save(app, name, imported, skipped),
        Pending::AutoEq(variant) => {
            if let Some(list) = app.presets.preset_list.as_mut() {
                list.step = Step::Choosing;
            }
            start(app, variant, name)
        }
    }
}

/// Keep `imported` as the preset `name`, which is free: the list is back with the new row
/// focused, its fit worked out now rather than when the row is first drawn, and the import said
/// in a toast. The preset is written, not loaded.
pub(super) fn save(app: &mut App, name: &str, imported: Imported, skipped: usize) -> Vec<Intent> {
    let Some(eq) = app.profile.eq.clone() else {
        return Vec::new();
    };
    let said = crate::commands::device::saved_line(
        name,
        imported.correction.filters.len(),
        Some(imported.correction.preamp_db),
        skipped,
    );
    let preset = Preset::imported(name, imported);
    if let Some(source) = &preset.source {
        preset::fit_for(&eq, source);
    }
    let row = entries(app, &eq).len();
    if let Some(list) = app.presets.preset_list.as_mut() {
        list.row = row;
        list.step = Step::Choosing;
    }
    app.notify(said);
    vec![Intent::SavePreset(preset)]
}

/// `Enter` on a measurement of the search box: import it under the headphone's own name, or
/// ask for another when a preset carries that one.
pub(super) fn autoeq(app: &mut App, variant: AutoEqVariant) -> Vec<Intent> {
    let name = Some(preset::default_name(&variant.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| variant.name.clone());
    let name = name.trim().to_string();
    if !taken(app, &name) {
        return start(app, variant, &name);
    }
    let text = format!("{name} ({})", variant.source);
    ask_for_name(app, &name, text, Pending::AutoEq(variant))
}

/// Ask the daemon to import `variant` as the preset `name`, which is free, for the side the list
/// is for. The search box, when it is the step, says it is importing; the daemon's answer
/// ([`imported`]) does the rest.
fn start(app: &mut App, variant: AutoEqVariant, name: &str) -> Vec<Intent> {
    let target = match app.on_computer() {
        true => AutoEqTarget::Computer,
        false => AutoEqTarget::Device {
            device: app.session.device_name.clone(),
        },
    };
    match app.presets.preset_list.as_mut() {
        Some(PresetList {
            step: Step::Searching(search),
            ..
        }) => {
            search.importing = Some(variant.name.clone());
            search.importing_as = Some(name.to_string());
        }
        _ => app.notify(format!("importing {name:?}…")),
    }
    vec![Intent::AutoEqImport {
        variant: Box::new(variant),
        name: name.to_string(),
        target,
    }]
}

/// The daemon's import ended. A refusal is a toast, with the box still up; a saved preset has the
/// list back with the presets read again, which says it in a toast and focuses its row. The
/// preset is written, not loaded. The answer counts whatever the box is by then: the preset is
/// saved either way.
pub(crate) fn imported(
    app: &mut App,
    name: &str,
    result: Result<AutoEqSaved, String>,
) -> Vec<Intent> {
    let was_importing = match app.presets.preset_list.as_mut() {
        Some(PresetList {
            step: Step::Searching(search),
            ..
        }) if search.importing_as.as_deref() == Some(name) => {
            search.importing_as = None;
            search.importing.take().is_some()
        }
        _ => false,
    };
    match result {
        Err(why) => {
            app.warn(why);
            Vec::new()
        }
        Ok(saved) => {
            if was_importing && let Some(list) = app.presets.preset_list.as_mut() {
                list.step = Step::Choosing;
            }
            app.presets.landed = Some(Landed {
                name: saved.name,
                filters: saved.filters,
                skipped: saved.skipped,
            });
            vec![Intent::ReadPresets]
        }
    }
}
