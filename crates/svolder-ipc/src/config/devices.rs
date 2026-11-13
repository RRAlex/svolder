// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writers of the `[[device]]` blocks and of the two top-level keys that name a device
//! and an adapter: `default_device` and `adapter`.

use std::path::Path;

use svolder_profiles::tone::preset::{NAME_MAX, NameError, checked_name};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use super::edit::{
    blocks_mut, clear_key_at, detached, edit_file, hand_on, prefix_of, push_block, set_key,
    set_key_at,
};
use super::template::UNSET;
use super::{Config, ConfigError, DeviceConfig, missing, vanished};

impl Config {
    /// The configured device whose unit id is `unit`, spelled however its block spells it.
    pub fn device_for_unit(&self, unit: &svolder_proto::UnitId) -> Option<&DeviceConfig> {
        Self::device_for_unit_in(&self.devices, unit)
    }

    /// [`Config::device_for_unit`] over `devices`; what [`Config::parse`] checks each block
    /// against, so a file never holds one unit twice.
    pub(super) fn device_for_unit_in<'a>(
        devices: &'a [DeviceConfig],
        unit: &svolder_proto::UnitId,
    ) -> Option<&'a DeviceConfig> {
        devices
            .iter()
            .find(|device| device.udid.parse::<svolder_proto::UnitId>().as_ref() == Ok(unit))
    }

    /// Append `device` to the file at `path` as a new `[[device]]` block.
    ///
    /// Only the four keys a device is configured by are written, and `profile` and
    /// `passphrase` only where they say something: a device's input names are left to a hand
    /// edit. A name another device already carries is refused, so the file never grows two
    /// blocks a selector cannot tell apart, and so is a unit id another device already carries
    /// (the writer's strict parse names both devices): a second block for one unit would be a second
    /// actor on the same device. A name that is none (blank, too long, holding a control
    /// character) is refused, and one that is a name is written without the blanks round it.
    pub fn add_device(path: &Path, device: &DeviceConfig) -> Result<(), ConfigError> {
        let device = &named_as_written(device)?;
        edit_file(path, |doc| {
            refuse_a_unit_twice(doc)?;
            if let Some((_, clash)) = named(doc, &device.name) {
                return Err(taken(&clash));
            }
            let mut table = Table::new();
            write_device(&mut table, device);
            push_block(doc, DEVICE, table)?;
            Config::parse_strict(&doc.to_string()).map(drop)
        })
    }

    /// Take the `[[device]]` block named `name` out of the file at `path`.
    ///
    /// Comments written right above the block go with it, as a comment about a device should;
    /// comments a blank line sets apart from it belong to the file, not the device, and are
    /// handed to whatever the document prints next. Everything else in the file stays where it
    /// was. A name no block carries is refused.
    ///
    /// A `default_device` naming this very device is cleared in the same edit, back to the
    /// commented line the template shows it at — otherwise the file would name a default it no
    /// longer configures, which [`Config::parse`] refuses to load.
    pub fn remove_device(path: &Path, name: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| remove_in(doc, name))
    }

    /// [`Config::update_device`] on the device `selector` names, with what `patch` makes of the
    /// block as the file holds it now. The device is found, patched and written under the
    /// file's one lock, so a second writer between the find and the write waits and builds on
    /// this one's change; a caller that read the file first could write an older block back.
    ///
    /// The selector is a configured name or a whole unit id, neither case-sensitive; one that
    /// names nothing is refused as missing, and what `patch` refuses is handed back with nothing
    /// written.
    pub fn patch_device(
        path: &Path,
        selector: &str,
        patch: impl FnOnce(&DeviceConfig) -> Result<DeviceConfig, ConfigError>,
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let current = selected(doc, selector)?;
            let next = patch(&current)?;
            update_in(doc, &current.name, &next)
        })
    }

    /// [`Config::remove_device`] on the device `selector` names ([`Config::patch_device`]'s
    /// selector), found and removed under the file's one lock.
    pub fn remove_selected_device(path: &Path, selector: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let current = selected(doc, selector)?;
            remove_in(doc, &current.name)
        })
    }

    /// Make the device `selector` names ([`Config::patch_device`]'s selector) the default,
    /// under the name the file spells it with. The device is found and the key written under
    /// the file's one lock, so the file is never left naming a default a writer has just
    /// removed; a selector that names nothing is refused as missing.
    pub fn default_to_device(path: &Path, selector: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let current = selected(doc, selector)?;
            set_key_at(
                doc,
                &[],
                "default_device",
                Value::from(current.name.as_str()),
            )
        })
    }

    /// Write `device` over the `[[device]]` block named `name` in the file at `path`.
    ///
    /// Each of the four keys is set where it already sits, so the comments around it survive,
    /// and a `profile` or `passphrase` the new device says nothing about is taken out. Any
    /// other key the block carries — its `[device.inputs]` table — is left alone.
    ///
    /// A name no block carries is refused, and so is a rename onto a name another block
    /// already carries (the file never grows two blocks a selector cannot tell apart);
    /// renaming a block to the name it has, or to another spelling of it, is no collision.
    /// A rename onto a name that is none is refused as [`Config::add_device`] refuses it; a block
    /// keeps the name it has, whatever that is.
    ///
    /// The edit is judged by the file it leaves: one that leaves a unit on two devices is
    /// refused, and one that takes a device off a unit it shared is accepted. A file that
    /// carried a duplicate and still does is refused with the writers' words for it; an edit
    /// that makes the duplicate is refused by the strict parse.
    ///
    /// A `default_device` naming this very device is renamed with it, in the same edit —
    /// otherwise the file would name a default it no longer configures, which
    /// [`Config::parse`] refuses to load.
    pub fn update_device(
        path: &Path,
        name: &str,
        device: &DeviceConfig,
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| update_in(doc, name, device))
    }

    /// Set the `default_device` key of the file at `path`, or take it out for `None`.
    ///
    /// A file that carries the template's commented `default_device` line has the live key
    /// written where that line stood, under the comment that explains it, exactly as
    /// [`Config::set_adapter`] writes the adapter.
    ///
    /// The name is not checked here: a `default_device` naming nothing configured parses, and
    /// [`Config::warnings`] reports it.
    pub fn set_default_device(path: &Path, name: Option<&str>) -> Result<(), ConfigError> {
        edit_file(path, |doc| match name {
            Some(name) => set_key_at(doc, &[], "default_device", Value::from(name)),
            None => clear_key_at(doc, &[], "default_device", UNSET),
        })
    }

    /// Set the `adapter` key of the file at `path`, or take it out for `None`.
    ///
    /// A file that carries the template's commented `adapter` line has the live key written
    /// where that line stood, under the comment that explains it.
    pub fn set_adapter(path: &Path, adapter: Option<&str>) -> Result<(), ConfigError> {
        edit_file(path, |doc| match adapter {
            Some(adapter) => set_key_at(doc, &[], "adapter", Value::from(adapter)),
            None => clear_key_at(doc, &[], "adapter", UNSET),
        })
    }
}

/// The key the `[[device]]` blocks sit under.
const DEVICE: &str = "device";

/// The `[[device]]` block of `doc` that carries `name` — where it sits and how the file spells
/// it — matching case aside, the same way every other device selector matches a name.
fn named(doc: &DocumentMut, name: &str) -> Option<(usize, String)> {
    doc.as_table()
        .get(DEVICE)?
        .as_array_of_tables()?
        .iter()
        .enumerate()
        .find_map(|(at, table)| {
            let carried = table.get("name")?.as_str()?;
            carried
                .eq_ignore_ascii_case(name)
                .then(|| (at, carried.to_string()))
        })
}

/// Whether the file's `default_device` names the device called `name`, whichever way the two are
/// spelled: a selector matches a device by name without regard to case, and so does this.
fn defaults_to(doc: &DocumentMut, name: &str) -> bool {
    doc.as_table()
        .get("default_device")
        .and_then(Item::as_str)
        .is_some_and(|default| default.eq_ignore_ascii_case(name))
}

/// The device `selector` names in `doc`: by configured name, else by whole unit id, neither
/// case-sensitive; or the error that it names none.
fn selected(doc: &DocumentMut, selector: &str) -> Result<DeviceConfig, ConfigError> {
    let devices = Config::parse(&doc.to_string())?.devices;
    let unit = selector.parse::<svolder_proto::UnitId>().ok();
    devices
        .iter()
        .find(|device| device.name.eq_ignore_ascii_case(selector))
        .or_else(|| unit.and_then(|unit| Config::device_for_unit_in(&devices, &unit)))
        .cloned()
        .ok_or_else(|| missing("device", selector))
}

/// Take the block named `name` out of `doc`, a default naming it with it
/// ([`Config::remove_device`]).
fn remove_in(doc: &mut DocumentMut, name: &str) -> Result<(), ConfigError> {
    let at = device_at(doc, name)?;
    if defaults_to(doc, name) {
        clear_key_at(doc, &[], "default_device", UNSET)?;
    }
    let taken = devices_mut(doc)?.remove(at);
    let carried = detached(prefix_of(&taken));
    if !carried.is_empty() {
        hand_on(doc, DEVICE, at, carried)?;
    }
    Ok(())
}

/// Write `device` over the block named `name` in `doc` ([`Config::update_device`]).
fn update_in(doc: &mut DocumentMut, name: &str, device: &DeviceConfig) -> Result<(), ConfigError> {
    let renamed;
    let device = match device.name.eq_ignore_ascii_case(name) {
        true => device,
        false => {
            renamed = named_as_written(device)?;
            &renamed
        }
    };
    let before = refuse_a_unit_twice(doc);
    let at = device_at(doc, name)?;
    if let Some((clash_at, clash)) = named(doc, &device.name)
        && clash_at != at
    {
        return Err(taken(&clash));
    }
    let renamed = defaults_to(doc, name);
    let table = devices_mut(doc)?
        .get_mut(at)
        .ok_or_else(|| vanished("device", name))?;
    write_device(table, device);
    // On a file that had a unit twice, refuse with the pair the edit leaves, if any.
    if before.is_err() {
        refuse_a_unit_twice(doc)?;
    }
    if renamed {
        set_key_at(
            doc,
            &[],
            "default_device",
            Value::from(device.name.as_str()),
        )?;
    }
    Config::parse_strict(&doc.to_string()).map(drop)
}

/// Which `[[device]]` block of `doc` is named `name`, or the error that names none is.
fn device_at(doc: &DocumentMut, name: &str) -> Result<usize, ConfigError> {
    named(doc, name)
        .map(|(at, _)| at)
        .ok_or_else(|| missing("device", name))
}

/// Refuse a device write on a file that already configures one unit twice (written before the
/// rule), in words that name the file's own fault and its cure rather than the strict parse's
/// bare clash: the write is about another device, and says nothing of the pair.
fn refuse_a_unit_twice(doc: &DocumentMut) -> Result<(), ConfigError> {
    match Config::parse(&doc.to_string())?.dropped_duplicates.first() {
        Some(line) => Err(ConfigError::Parse(format!(
            "this file already configures one unit twice, so no device is added or changed \
             until that is fixed: {line}"
        ))),
        None => Ok(()),
    }
}

/// `device` as a writer puts it in the file: its name without the blanks round it, or the error
/// saying why that is no name.
fn named_as_written(device: &DeviceConfig) -> Result<DeviceConfig, ConfigError> {
    let name = checked_name(&device.name).map_err(|why| ConfigError::Parse(name_words(why)))?;
    Ok(DeviceConfig {
        name: name.to_string(),
        ..device.clone()
    })
}

/// Why the device writers refuse `name`, in their words, or `None` for a name they take: blank,
/// over the name length, or holding a control character. A front end that asks for a name says
/// so before the write.
pub fn device_name_refusal(name: &str) -> Option<String> {
    checked_name(name).err().map(name_words)
}

/// The words a device writer refuses a name with.
fn name_words(why: NameError) -> String {
    match why {
        NameError::Blank => "a device needs a name".to_string(),
        NameError::TooLong(length) => {
            format!("a device's name is at most {NAME_MAX} characters, not {length}")
        }
        NameError::Control => {
            "a device's name cannot hold a control character, such as a newline or a tab"
                .to_string()
        }
    }
}

/// What a writer refuses a name another block already carries with.
fn taken(name: &str) -> ConfigError {
    ConfigError::Taken {
        kind: "device",
        name: name.to_string(),
    }
}

/// The `[[device]]` array of `doc`, for a writer that has already found a block in it.
fn devices_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, ConfigError> {
    blocks_mut(doc, DEVICE)
}

/// Write the four keys a device is configured by into `table`, taking out the two that say
/// nothing.
///
/// Everything else the block carries — its `[device.inputs]` table, the comments around each
/// key — is left where it is by [`set_key`].
fn write_device(table: &mut Table, device: &DeviceConfig) {
    // No run of comments is offered to the setter: the template's commented `[[device]]` is an
    // example of what a block looks like rather than this block's own defaults, and a device
    // is never written over it.
    let mut nothing = String::new();
    set_key(
        table,
        &mut nothing,
        "name",
        Value::from(device.name.as_str()),
    );
    set_key(
        table,
        &mut nothing,
        "udid",
        Value::from(device.udid.as_str()),
    );
    match device.profile.as_deref() {
        Some(profile) => {
            set_key(table, &mut nothing, "profile", Value::from(profile));
        }
        None => {
            table.remove("profile");
        }
    }
    if device.passphrase.is_empty() {
        table.remove("passphrase");
    } else {
        set_key(
            table,
            &mut nothing,
            "passphrase",
            Value::from(device.passphrase.as_str()),
        );
    }
}
