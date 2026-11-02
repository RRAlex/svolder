// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writers of the `[mcp]`, `[mqtt]` and `[web]` tables, which the TUI's Connections settings
//! save a change with: only the keys that differ are written, in the shape a hand edit would use.
//!
//! No writer here ever writes the broker's password, the token file or any other key the
//! Connections settings do not show: the keys are named one by one below.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::edit::{edit_file, set_key_at};
use super::{Config, ConfigError, McpConfig, MqttConfig, WebConfig};

impl Config {
    /// Change the `[mcp]` table of the file at `path` with `edit`, and rewrite only the keys of
    /// `enabled`, `listen` and `writes` that change alters.
    ///
    /// The table `edit` is handed is the one the file holds at this moment, read as
    /// [`Config::parse`] reads it, so a key edited by hand since the TUI started stays. Every
    /// key left alone is left alone in the text too, comments and layout included, and so is
    /// everything outside `[mcp]`; an edit that changes nothing leaves the file byte for byte.
    /// A missing file starts from [`Config::template`], and a missing `[mcp]` table is added
    /// at the end of the document when there is something to write. A file that does not
    /// parse, or whose result would not, is left as it was and the error returned.
    pub fn update_mcp(path: &Path, edit: impl FnOnce(&mut McpConfig)) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse(&doc.to_string())?.mcp;
            let mut after = before.clone();
            edit(&mut after);
            if before.enabled != after.enabled {
                set(doc, "mcp", "enabled", Value::from(after.enabled))?;
            }
            if before.listen != after.listen {
                set(doc, "mcp", "listen", Value::from(after.listen.to_string()))?;
            }
            if before.writes != after.writes {
                set(doc, "mcp", "writes", Value::from(after.writes))?;
            }
            Ok(())
        })
    }

    /// Change the `[web]` table of the file at `path` with `edit`, and rewrite only the keys of
    /// `enabled` and `listen` that change alters, as [`Config::update_mcp`] does for `[mcp]`:
    /// the TUI's Web box changes nothing else.
    pub fn update_web(path: &Path, edit: impl FnOnce(&mut WebConfig)) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse(&doc.to_string())?.web;
            let mut after = before.clone();
            edit(&mut after);
            if before.enabled != after.enabled {
                set(doc, "web", "enabled", Value::from(after.enabled))?;
            }
            if before.listen != after.listen {
                set(doc, "web", "listen", Value::from(after.listen.to_string()))?;
            }
            Ok(())
        })
    }

    /// Change the `[mqtt]` table of the file at `path` with `edit`, and rewrite only the keys of
    /// `enabled`, `host`, `port` and `commands` that change alters, as [`Config::update_mcp`]
    /// does for `[mcp]`. The password is never written: no edit of it reaches the file.
    pub fn update_mqtt(path: &Path, edit: impl FnOnce(&mut MqttConfig)) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse(&doc.to_string())?.mqtt;
            let mut after = before.clone();
            edit(&mut after);
            if before.enabled != after.enabled {
                set(doc, "mqtt", "enabled", Value::from(after.enabled))?;
            }
            if before.host != after.host
                && let Some(host) = after.host.as_deref()
            {
                set(doc, "mqtt", "host", Value::from(host))?;
            }
            if before.port != after.port {
                set(doc, "mqtt", "port", Value::from(i64::from(after.port)))?;
            }
            if before.commands != after.commands {
                set(doc, "mqtt", "commands", Value::from(after.commands))?;
            }
            Ok(())
        })
    }
}

/// Set `key` of `[table]` in `doc`, adding the table at the end of the document when the file
/// has none.
fn set(doc: &mut DocumentMut, table: &str, key: &str, value: Value) -> Result<(), ConfigError> {
    let root = doc.as_table_mut();
    if !root.contains_key(table) {
        root.insert(table, Item::Table(Table::new()));
    }
    set_key_at(doc, &[table], key, value)
}
