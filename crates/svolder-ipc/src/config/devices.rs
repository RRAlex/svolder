// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writers of the `[[device]]` blocks and of the two top-level keys that name a device
//! and an adapter: `default_device` and `adapter`.

use std::path::Path;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use super::edit::{
    blocks_mut, clear_key_at, detached, edit_file, hand_on, prefix_of, push_block, set_key,
    set_key_at,
};
use super::template::UNSET;
use super::{Config, ConfigError, DeviceConfig};

impl Config {
    /// Append `device` to the file at `path` as a new `[[device]]` block.
    ///
    /// Only the four keys a device is configured by are written, and `profile` and
    /// `passphrase` only where they say something: a device's input names are left to a hand
    /// edit. A name another device already carries is refused, so the file never grows two
    /// blocks a selector cannot tell apart.
    pub fn add_device(path: &Path, device: &DeviceConfig) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            if let Some((_, clash)) = named(doc, &device.name) {
                return Err(taken(&clash));
            }
            let mut table = Table::new();
            write_device(&mut table, device);
            push_block(doc, DEVICE, table)
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
        edit_file(path, |doc| {
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
    ///
    /// A `default_device` naming this very device is renamed with it, in the same edit —
    /// otherwise the file would name a default it no longer configures, which
    /// [`Config::parse`] refuses to load.
    pub fn update_device(
        path: &Path,
        name: &str,
        device: &DeviceConfig,
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let at = device_at(doc, name)?;
            if let Some((clash_at, clash)) = named(doc, &device.name)
                && clash_at != at
            {
                return Err(taken(&clash));
            }
            let renamed = defaults_to(doc, name);
            let table = devices_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| ConfigError::Parse(format!("device {name:?} went missing")))?;
            write_device(table, device);
            if renamed {
                set_key_at(
                    doc,
                    &[],
                    "default_device",
                    Value::from(device.name.as_str()),
                )?;
            }
            Ok(())
        })
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

/// Which `[[device]]` block of `doc` is named `name`, or the error that names none is.
fn device_at(doc: &DocumentMut, name: &str) -> Result<usize, ConfigError> {
    named(doc, name)
        .map(|(at, _)| at)
        .ok_or_else(|| ConfigError::Parse(format!("no device named {name:?} is configured")))
}

/// What a writer refuses a name another block already carries with.
fn taken(name: &str) -> ConfigError {
    ConfigError::Parse(format!("a device named {name:?} is already configured"))
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
