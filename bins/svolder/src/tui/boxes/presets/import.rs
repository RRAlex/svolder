// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the search box and the file prompt hand over: a correction to keep as a preset. A name
//! no preset carries saves it at once, with the new row focused and not loaded; a name that is
//! taken opens the name prompt prefilled with another, `NAME (source)` for a download and
//! `NAME 2` for a file.

use svolder_autoeq::Entry as Measurement;
use svolder_profiles::tone::apo;
use svolder_profiles::tone::preset::{self, Imported, Preset};

use crate::tui::app::{App, Intent};

use super::{NameLine, PresetList, Step, entries};

/// A correction waiting for the name the user gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// The correction.
    pub imported: Imported,
    /// How many lines of its source the parser left out.
    pub skipped: usize,
}

/// How a name that is taken is made into another.
pub(super) enum Alt<'a> {
    /// `NAME (source)`: the measurement's source in brackets.
    Source(&'a str),
    /// `NAME 2`, the first number that makes the name free.
    Number,
}

/// A correction `imported` under the preset name `name`: saved if the name is free, else asked
/// for again with another name filled in.
pub(super) fn ready(
    app: &mut App,
    name: String,
    imported: Imported,
    skipped: usize,
    alt: Alt<'_>,
) -> Vec<Intent> {
    let name = name.trim().to_string();
    if !taken(app, &name) {
        return save(app, &name, imported, skipped);
    }
    let text = match alt {
        Alt::Source(source) => format!("{name} ({source})"),
        Alt::Number => (2u32..)
            .map(|n| format!("{name} {n}"))
            .find(|candidate| !taken(app, candidate))
            .unwrap_or_else(|| name.clone()),
    };
    let text: String = text.chars().take(preset::NAME_MAX).collect();
    if let Some(list) = app.presets.preset_list.as_mut() {
        let mut line = NameLine::new(None, text);
        line.import = Some(Box::new(Pending { imported, skipped }));
        list.step = Step::Naming(line);
    }
    if let Some(why) = refusal(app, &name) {
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
        imported.correction.preamp_db,
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

/// A download ended: import the file, or say why not. A result for a box that has since been
/// closed, or is downloading another headphone, is dropped.
pub(crate) fn downloaded(
    app: &mut App,
    entry: &Measurement,
    result: Result<String, String>,
) -> Vec<Intent> {
    let Some(PresetList {
        step: Step::Searching(search),
        ..
    }) = app.presets.preset_list.as_mut()
    else {
        return Vec::new();
    };
    if search.downloading.as_deref() != Some(entry.name.as_str()) {
        return Vec::new();
    }
    search.downloading = None;
    let text = match result {
        Ok(text) => text,
        Err(why) => {
            app.warn(why);
            return Vec::new();
        }
    };
    let parsed = match apo::parse(&text) {
        Ok(parsed) => parsed,
        Err(e) => {
            app.warn(format!(
                "the file for {} is not a parametric EQ: {e}",
                entry.name
            ));
            return Vec::new();
        }
    };
    let name = Some(preset::default_name(&entry.name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| entry.name.clone());
    let skipped = parsed.notes.len();
    let imported = Imported {
        origin: Some(entry.origin()),
        correction: parsed.correction,
    };
    ready(app, name, imported, skipped, Alt::Source(&entry.source))
}
