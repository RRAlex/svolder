// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[[preset]]` blocks: finding a user's preset by name or band count, and the writers
//! that add, update, rename and remove one.

use std::path::Path;

use svolder_profiles::tone::apo::SourceFilter;
use svolder_profiles::tone::preset::{Preset, PresetBand, checked_name, new_name, same_name};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use super::edit::{blocks_mut, detached, edit_file, hand_on, prefix_of, push_block, set_key};
use super::{Config, ConfigError};

impl Config {
    /// The preset named `name`, matching case and the blanks round it aside
    /// ([`same_name`]), or `None`.
    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|preset| same_name(&preset.name, name))
    }

    /// The user's presets that fit a tone control with `bands` bands, in file order, and every
    /// imported preset, which fits any tone control that has bands.
    ///
    /// The built-ins are not among them: a caller with a device's
    /// [`EqSpec`](svolder_profiles::EqSpec) puts [`svolder_profiles::tone::preset::builtins`]
    /// first; one with no device names the built-ins a band count would offer with
    /// [`svolder_profiles::tone::preset::builtin_names`].
    pub fn user_presets_for(&self, bands: usize) -> impl Iterator<Item = &Preset> {
        self.presets
            .iter()
            .filter(move |preset| match preset.is_imported() {
                true => bands > 0,
                false => usize::from(preset.bands) == bands,
            })
    }

    /// The presets that fit `profile`'s tone control, each with whether it is a built-in: the
    /// built-ins first, then the user's own in file order. Empty for a profile with no tone
    /// control.
    pub fn fitting_presets(&self, profile: &svolder_profiles::Profile) -> Vec<(Preset, bool)> {
        profile
            .eq
            .as_ref()
            .map(|eq| svolder_profiles::tone::preset::fitting(eq, &self.presets))
            .unwrap_or_default()
    }

    /// Append `preset` to the file at `path` as a new `[[preset]]` block.
    ///
    /// The name is written without the blanks round it, and a name [`new_name`] refuses is
    /// refused here with its words: another preset's, a built-in's or `custom` in any case, so
    /// the file never grows a preset a selector cannot tell apart from another, or one that
    /// shadows a built-in the tone screen already offers.
    pub fn add_preset(path: &Path, preset: &Preset) -> Result<(), ConfigError> {
        let preset = &named_as_written(preset)?;
        edit_file(path, |doc| {
            checked_new(doc, &preset.name, None)?;
            let mut table = Table::new();
            write_preset(&mut table, preset);
            push_block(doc, PRESET, table)
        })
    }

    /// Write `preset` over the `[[preset]]` block named `name` in the file at `path`.
    ///
    /// Each of the five keys is set where it already sits, so the comments around the block
    /// survive; the `band` array is always rewritten whole, as an inline array of inline
    /// tables, one band per line.
    ///
    /// A name no block carries is refused, and so is renaming it onto a name [`new_name`]
    /// refuses; renaming it to the name it has, or to another spelling of it, is no collision.
    /// `name` is looked up as [`Config::preset`] looks it up, and `preset`'s own name is written
    /// and refused as [`Config::add_preset`] writes and refuses it.
    pub fn update_preset(path: &Path, name: &str, preset: &Preset) -> Result<(), ConfigError> {
        let preset = &named_as_written(preset)?;
        edit_file(path, |doc| {
            let at = preset_at(doc, name)?;
            let own = named_preset(doc, name).map(|(_, own)| own);
            checked_new(doc, &preset.name, own.as_deref())?;
            let table = presets_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| ConfigError::Parse(format!("preset {name:?} went missing")))?;
            write_preset(table, preset);
            Ok(())
        })
    }

    /// Rename the `[[preset]]` block named `old` to `new` in the file at `path`, leaving its
    /// curve untouched.
    ///
    /// A block named `old` is required, and `new` is written without the blanks round it and
    /// refused as [`new_name`] refuses it, as [`Config::update_preset`]'s rename is.
    pub fn rename_preset(path: &Path, old: &str, new: &str) -> Result<(), ConfigError> {
        let new = written_name(new)?;
        edit_file(path, |doc| {
            let at = preset_at(doc, old)?;
            let own = named_preset(doc, old).map(|(_, own)| own);
            checked_new(doc, new, own.as_deref())?;
            let table = presets_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| ConfigError::Parse(format!("preset {old:?} went missing")))?;
            let mut nothing = String::new();
            set_key(table, &mut nothing, "name", Value::from(new));
            Ok(())
        })
    }

    /// Take the `[[preset]]` block named `name` out of the file at `path`.
    ///
    /// The comments written right above the block go with it, exactly as [`Config::remove_device`]
    /// hands a device's own comments on; comments a blank line sets apart from it are the
    /// file's, and are handed to whatever the document prints next. A name no block carries is
    /// refused.
    pub fn remove_preset(path: &Path, name: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let at = preset_at(doc, name)?;
            let taken = presets_mut(doc)?.remove(at);
            let carried = detached(prefix_of(&taken));
            if !carried.is_empty() {
                hand_on(doc, PRESET, at, carried)?;
            }
            Ok(())
        })
    }
}

/// The key the `[[preset]]` blocks sit under.
const PRESET: &str = "preset";

/// The `[[preset]]` block of `doc` that carries `name` — where it sits and its name as the
/// loaded configuration spells it — matching case and the blanks round either name aside, the
/// way [`Config::preset`] finds one, so a block the file spells with blanks round its name is
/// found by the name every front end shows.
fn named_preset(doc: &DocumentMut, name: &str) -> Option<(usize, String)> {
    doc.as_table()
        .get(PRESET)?
        .as_array_of_tables()?
        .iter()
        .enumerate()
        .find_map(|(at, table)| {
            let carried = table.get("name")?.as_str()?;
            same_name(carried, name).then(|| (at, carried.trim().to_string()))
        })
}

/// Which `[[preset]]` block of `doc` is named `name`, or the error that names none is.
fn preset_at(doc: &DocumentMut, name: &str) -> Result<usize, ConfigError> {
    named_preset(doc, name)
        .map(|(at, _)| at)
        .ok_or_else(|| ConfigError::Parse(format!("no preset named {name:?} is configured")))
}

/// What a preset writer refuses a name another block already carries with.
pub(super) fn preset_taken(name: &str) -> ConfigError {
    ConfigError::Parse(format!("a preset named {name:?} is already configured"))
}

/// The names of the `[[preset]]` blocks of `doc`, without the blanks round them.
fn preset_names(doc: &DocumentMut) -> Vec<String> {
    doc.as_table()
        .get(PRESET)
        .and_then(Item::as_array_of_tables)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| Some(block.get("name")?.as_str()?.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Refuse `name` for a `[[preset]]` block of `doc` where [`new_name`] refuses it, against the
/// document just read; `renaming` is the block's own name, which it may take again.
fn checked_new(doc: &DocumentMut, name: &str, renaming: Option<&str>) -> Result<(), ConfigError> {
    new_name(name, preset_names(doc), renaming)
        .map(|_| ())
        .map_err(|why| ConfigError::Parse(why.to_string()))
}

/// `text` as a writer puts a preset's name in the file, without the blanks round it, or the
/// error saying why it is no name ([`checked_name`]): a writer refuses it itself rather than
/// leave it to the parse of the whole file, whose error is about the file.
fn written_name(text: &str) -> Result<&str, ConfigError> {
    checked_name(text).map_err(|why| ConfigError::Parse(why.to_string()))
}

/// `preset` with its name as a writer puts it in the file ([`written_name`]).
fn named_as_written(preset: &Preset) -> Result<Preset, ConfigError> {
    Ok(Preset {
        name: written_name(&preset.name)?.to_string(),
        ..preset.clone()
    })
}

/// The `[[preset]]` array of `doc`, for a writer that has already found a block in it.
fn presets_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, ConfigError> {
    blocks_mut(doc, PRESET)
}

/// Write the keys a preset is configured by into `table`: five for a preset of bands, `name`,
/// `origin`, `preamp` and `filter` for an imported one, and none of the other kind's.
///
/// `name`, `bands`, `mode` and `loudness` are set where they already sit, keeping the comments
/// around them, by [`set_key`]. `band` is always rewritten whole, as the array
/// [`band_array`] builds: an inline array of inline tables, one per line, exactly what a load
/// reads back — nothing is gained by editing it entry by entry. `filter` is written the same
/// way by [`filter_array`].
fn write_preset(table: &mut Table, preset: &Preset) {
    let mut nothing = String::new();
    set_key(
        table,
        &mut nothing,
        "name",
        Value::from(preset.name.as_str()),
    );
    if let Some(imported) = &preset.source {
        for key in ["bands", "mode", "loudness", "band"] {
            table.remove(key);
        }
        match &imported.origin {
            Some(origin) => {
                set_key(table, &mut nothing, "origin", Value::from(origin.as_str()));
            }
            None => {
                table.remove("origin");
            }
        }
        set_key(
            table,
            &mut nothing,
            "preamp",
            Value::from(imported.correction.preamp_db),
        );
        set_key(
            table,
            &mut nothing,
            "filter",
            filter_array(&imported.correction.filters),
        );
        return;
    }
    for key in ["origin", "preamp", "filter"] {
        table.remove(key);
    }
    set_key(
        table,
        &mut nothing,
        "bands",
        Value::from(i64::from(preset.bands)),
    );
    set_key(table, &mut nothing, "mode", Value::from(preset.mode.name()));
    set_key(
        table,
        &mut nothing,
        "loudness",
        Value::from(i64::from(preset.loudness)),
    );
    set_key(table, &mut nothing, "band", band_array(&preset.band));
}

/// `band` as svolder writes it: an inline array of inline tables, one band per line, with a
/// trailing comma on the last one and the closing bracket on a line of its own.
fn band_array(band: &[PresetBand]) -> Value {
    let mut array = toml_edit::Array::new();
    for b in band {
        let mut inline = toml_edit::InlineTable::new();
        inline.insert("f", Value::from(b.f));
        inline.insert("g", Value::from(b.g));
        inline.insert("q", Value::from(b.q));
        let mut value = Value::InlineTable(inline);
        value.decor_mut().set_prefix("\n  ");
        array.push_formatted(value);
    }
    array.set_trailing_comma(true);
    array.set_trailing("\n");
    Value::Array(array)
}

/// `filter` as svolder writes it, laid out as [`band_array`] lays out `band`: `type` in upper
/// case, then `f`, `g` and `q`.
fn filter_array(filters: &[SourceFilter]) -> Value {
    let mut array = toml_edit::Array::new();
    for filter in filters {
        let mut inline = toml_edit::InlineTable::new();
        inline.insert("type", Value::from(filter.kind.code()));
        inline.insert("f", Value::from(filter.f));
        inline.insert("g", Value::from(filter.g));
        inline.insert("q", Value::from(filter.q));
        let mut value = Value::InlineTable(inline);
        value.decor_mut().set_prefix("\n  ");
        array.push_formatted(value);
    }
    array.set_trailing_comma(true);
    array.set_trailing("\n");
    Value::Array(array)
}
