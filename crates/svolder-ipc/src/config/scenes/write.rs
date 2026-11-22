// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writers of the `[[scene]]` blocks: add, update, rename and remove one.

use std::path::Path;

use svolder_host::eq::ComputerPart;
use svolder_profiles::scene::DevicePart;
use svolder_profiles::tone::preset::{CUSTOM, NameRefusal, Preset, checked_name, same_name};
use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use super::super::edit::{
    blocks_mut, detached, edit_file, hand_on, prefix_of, push_block, set_key,
};
use super::super::presets::{band_array, filter_array};
use super::super::{Config, ConfigError, missing, vanished};
use super::Scene;

/// The key the `[[scene]]` blocks sit under.
const SCENE: &str = "scene";

impl Config {
    /// The scene named `name`, matching case and the blanks round it aside, or `None`.
    pub fn scene(&self, name: &str) -> Option<&Scene> {
        self.scenes
            .iter()
            .find(|scene| same_name(&scene.name, name))
    }

    /// Append `scene` to the file at `path` as a new `[[scene]]` block.
    ///
    /// The name is written without the blanks round it, and a name the rules refuse is refused
    /// here in words: another scene's, `custom` in any case, or one that is blank, too long or
    /// holds a control character. A scene may take the name of a preset: the two lists are
    /// apart.
    pub fn add_scene(path: &Path, scene: &Scene) -> Result<(), ConfigError> {
        let scene = &named_as_written(scene)?;
        edit_file(path, |doc| {
            checked_new(doc, &scene.name, None)?;
            let mut table = Table::new();
            write_scene(&mut table, scene);
            push_block(doc, SCENE, table)
        })
    }

    /// Write `scene` over the `[[scene]]` block named `name` in the file at `path`.
    ///
    /// The `name` key is set where it sits, so the comments round the block survive; the
    /// computer's table and the device blocks are written again whole. A name no block carries
    /// is refused, and so is renaming it onto a name the rules refuse; the name it has, in
    /// another spelling, is no collision.
    pub fn update_scene(path: &Path, name: &str, scene: &Scene) -> Result<(), ConfigError> {
        let scene = &named_as_written(scene)?;
        edit_file(path, |doc| {
            let (at, own) = named_scene(doc, name).ok_or_else(|| missing("scene", name))?;
            checked_new(doc, &scene.name, Some(&own))?;
            let table = scenes_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| vanished("scene", name))?;
            write_scene(table, scene);
            Ok(())
        })
    }

    /// Rename the `[[scene]]` block named `old` to `new` in the file at `path`, leaving what it
    /// holds untouched.
    pub fn rename_scene(path: &Path, old: &str, new: &str) -> Result<(), ConfigError> {
        let new = written_name(new)?;
        edit_file(path, |doc| {
            let (at, own) = named_scene(doc, old).ok_or_else(|| missing("scene", old))?;
            checked_new(doc, new, Some(&own))?;
            let table = scenes_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| vanished("scene", old))?;
            let mut nothing = String::new();
            set_key(table, &mut nothing, "name", Value::from(new));
            Ok(())
        })
    }

    /// Take the `[[scene]]` block named `name` out of the file at `path`.
    ///
    /// The comments written right above the block go with it, as a preset's do; comments a
    /// blank line sets apart from it are the file's. A name no block carries is refused.
    pub fn remove_scene(path: &Path, name: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let (at, _) = named_scene(doc, name).ok_or_else(|| missing("scene", name))?;
            let taken = scenes_mut(doc)?.remove(at);
            let carried = detached(prefix_of(&taken));
            if !carried.is_empty() {
                hand_on(doc, SCENE, at, carried)?;
            }
            Ok(())
        })
    }
}

/// What a scene writer refuses a name another scene already carries with.
pub(super) fn scene_taken(name: &str) -> ConfigError {
    ConfigError::Taken {
        kind: "scene",
        name: name.to_string(),
    }
}

/// The `[[scene]]` block of `doc` that carries `name`: where it sits and its name as the file
/// spells it, without the blanks round it.
fn named_scene(doc: &DocumentMut, name: &str) -> Option<(usize, String)> {
    doc.as_table()
        .get(SCENE)?
        .as_array_of_tables()?
        .iter()
        .enumerate()
        .find_map(|(at, table)| {
            let carried = table.get("name")?.as_str()?;
            same_name(carried, name).then(|| (at, carried.trim().to_string()))
        })
}

/// The names of the `[[scene]]` blocks of `doc`, without the blanks round them.
fn scene_names(doc: &DocumentMut) -> Vec<String> {
    doc.as_table()
        .get(SCENE)
        .and_then(Item::as_array_of_tables)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| Some(block.get("name")?.as_str()?.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Refuse `name` for a `[[scene]]` block of `doc`, in a scene's words: a name that is blank, too
/// long or holds a control character, `custom` in any case, or one another scene carries. The
/// scenes are a list of their own, so a preset's name, a built-in's included, is no refusal.
/// `renaming` is the block's own name, which it may take again in another spelling. A taken
/// name is answered with the spelling the file already has.
fn checked_new(doc: &DocumentMut, name: &str, renaming: Option<&str>) -> Result<(), ConfigError> {
    let name = written_name(name)?;
    if same_name(name, CUSTOM) {
        return Err(ConfigError::SceneName(NameRefusal::Reserved(
            name.to_string(),
        )));
    }
    let taken = scene_names(doc)
        .into_iter()
        .find(|other| same_name(other, name) && !renaming.is_some_and(|old| same_name(other, old)));
    match taken {
        Some(other) => Err(scene_taken(&other)),
        None => Ok(()),
    }
}

/// `text` as a writer puts a scene's name in the file, or the error saying why it is no name.
fn written_name(text: &str) -> Result<&str, ConfigError> {
    checked_name(text).map_err(|why| ConfigError::SceneName(NameRefusal::Name(why)))
}

/// `scene` with its name as a writer puts it in the file.
fn named_as_written(scene: &Scene) -> Result<Scene, ConfigError> {
    Ok(Scene {
        name: written_name(&scene.name)?.to_string(),
        ..scene.clone()
    })
}

/// The `[[scene]]` array of `doc`, for a writer that has already found a block in it.
fn scenes_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, ConfigError> {
    blocks_mut(doc, SCENE)
}

/// Write the keys of a scene into `table`: `name` where it sits, then the computer's table and
/// the device blocks, written again whole.
fn write_scene(table: &mut Table, scene: &Scene) {
    let mut nothing = String::new();
    set_key(
        table,
        &mut nothing,
        "name",
        Value::from(scene.name.as_str()),
    );
    table.remove("computer");
    table.remove("device");
    if let Some(part) = &scene.computer {
        table.insert("computer", Item::Table(computer_table(part)));
    }
    if !scene.devices.is_empty() {
        let mut devices = ArrayOfTables::new();
        for part in &scene.devices {
            devices.push(device_table(part));
        }
        table.insert("device", Item::ArrayOfTables(devices));
    }
}

/// A sub-table that starts a line of its own after a blank one.
fn sub_table() -> Table {
    let mut table = Table::new();
    table.decor_mut().set_prefix("\n");
    table
}

/// The computer's part as the `[scene.computer]` table.
fn computer_table(part: &ComputerPart) -> Table {
    let mut table = sub_table();
    table.insert("eq_on", Item::Value(Value::from(part.eq_on)));
    table.insert("name", Item::Value(Value::from(part.name.as_str())));
    if let Some(origin) = &part.origin {
        table.insert("origin", Item::Value(Value::from(origin.as_str())));
    }
    table.insert("preamp", Item::Value(Value::from(part.preamp_db)));
    table.insert("filter", Item::Value(filter_array(&part.filters)));
    if let Some(loudness) = &part.loudness {
        table.insert("loudness", Item::Value(inline_of(loudness)));
    }
    table.insert("crossfeed_on", Item::Value(Value::from(part.crossfeed_on)));
    if let Some(crossfeed) = &part.crossfeed {
        table.insert("crossfeed", Item::Value(inline_of(crossfeed)));
    }
    table.insert("speakers_on", Item::Value(Value::from(part.speakers_on)));
    if let Some(set) = &part.speakers {
        table.insert("speakers", Item::Value(Value::from(set.as_str())));
    }
    table
}

/// `value` as an inline table of every key its serde form has, in that order: the settings are
/// plain scalars, which the reader reads back with the same serde, and a key added to them
/// later is written without a change here.
fn inline_of<T: serde::Serialize>(value: &T) -> Value {
    let text = toml::to_string(value).expect("plain settings serialise to TOML");
    let parsed = text
        .parse::<DocumentMut>()
        .expect("what the TOML writer wrote parses");
    let mut inline = InlineTable::new();
    for (key, item) in parsed.iter() {
        if let Item::Value(found) = item {
            let mut found = found.clone();
            found.decor_mut().clear();
            inline.insert(key, found);
        }
    }
    Value::InlineTable(inline)
}

/// One device's part as a `[[scene.device]]` block, its tone as the `[scene.device.tone]` table
/// under it.
fn device_table(part: &DevicePart) -> Table {
    let mut table = Table::new();
    table.insert("udid", Item::Value(Value::from(part.udid.as_str())));
    table.insert("name", Item::Value(Value::from(part.name.as_str())));
    if let Some(bands) = &part.raw_bands {
        table.insert("raw_bands", Item::Value(settings_array(bands)));
    }
    table.insert("controls", Item::Value(settings_array(&part.controls)));
    if let Some(tone) = &part.tone {
        table.insert("tone", Item::Table(tone_table(tone)));
    }
    table
}

/// A device's tone as its table: the name, the mode, the loudness strength and the bands.
fn tone_table(tone: &Preset) -> Table {
    let mut table = sub_table();
    table.insert("name", Item::Value(Value::from(tone.name.as_str())));
    table.insert("mode", Item::Value(Value::from(tone.mode.name())));
    table.insert(
        "loudness",
        Item::Value(Value::from(i64::from(tone.loudness))),
    );
    table.insert("band", Item::Value(band_array(&tone.band)));
    table
}

/// Settings as an array of `{ key, step }` entries, one per line, in the given order.
fn settings_array(settings: &[(String, i64)]) -> Value {
    let mut array = toml_edit::Array::new();
    for (key, step) in settings {
        let mut inline = InlineTable::new();
        inline.insert("key", Value::from(key.as_str()));
        inline.insert("step", Value::from(*step));
        let mut value = Value::InlineTable(inline);
        value.decor_mut().set_prefix("\n  ");
        array.push_formatted(value);
    }
    array.set_trailing_comma(true);
    array.set_trailing("\n");
    Value::Array(array)
}
