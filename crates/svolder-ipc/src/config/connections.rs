// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writers of the `[mcp]`, `[mqtt]` and `[web]` tables, which the TUI's Connections settings
//! save a change with: only the keys that differ are written, in the shape a hand edit would use.
//!
//! No writer here writes the token file or any other key the Connections settings do not show:
//! the keys are named one by one below. The broker's password is written only by
//! [`Config::update_mqtt_noting`], when an edit changes it, and is never read back out to a client.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use super::edit::{clear_key_at, edit_file, edit_file_private, set_key_at};
use super::{Config, ConfigError, ListenDefaults, McpConfig, MqttConfig, WebConfig, WebGuest};

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
        Self::update_mcp_with(path, &ListenDefaults::from_env(), edit)
    }

    /// [`Config::update_mcp`] with the environment's defaults given rather than read: the table
    /// `edit` is handed has them applied, so a server the environment turned on is turned off
    /// by writing `enabled = false`.
    pub(crate) fn update_mcp_with(
        path: &Path,
        env: &ListenDefaults,
        edit: impl FnOnce(&mut McpConfig),
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse_with_listen_defaults(&doc.to_string(), env)?.mcp;
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
    /// `enabled`, `listen` and `guest` that change alters, as [`Config::update_mcp`] does for
    /// `[mcp]`: the TUI's Web box and the guest switch change nothing else. `guest` is written in
    /// lower case, `"none"` or `"read"`, as the parser reads it.
    pub fn update_web(path: &Path, edit: impl FnOnce(&mut WebConfig)) -> Result<(), ConfigError> {
        Self::update_web_with(path, &ListenDefaults::from_env(), edit)
    }

    /// [`Config::update_web`] with the environment's defaults given rather than read, as
    /// [`Config::update_mcp_with`].
    pub(crate) fn update_web_with(
        path: &Path,
        env: &ListenDefaults,
        edit: impl FnOnce(&mut WebConfig),
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse_with_listen_defaults(&doc.to_string(), env)?.web;
            let mut after = before.clone();
            edit(&mut after);
            if before.enabled != after.enabled {
                set(doc, "web", "enabled", Value::from(after.enabled))?;
            }
            if before.listen != after.listen {
                set(doc, "web", "listen", Value::from(after.listen.to_string()))?;
            }
            if before.guest != after.guest {
                let word = match after.guest {
                    WebGuest::None => "none",
                    WebGuest::Read => "read",
                };
                set(doc, "web", "guest", Value::from(word))?;
            }
            Ok(())
        })
    }

    /// [`Config::update_mqtt_noting`] without the answer.
    #[cfg(test)]
    pub fn update_mqtt(path: &Path, edit: impl FnOnce(&mut MqttConfig)) -> Result<(), ConfigError> {
        Self::update_mqtt_noting(path, edit).map(|_| ())
    }

    /// Change the `[mqtt]` table of the file at `path` with `edit`, and rewrite only the keys of
    /// `enabled`, `host`, `port`, `username`, `password` and `commands` that change alters, as
    /// [`Config::update_mcp`] does for `[mcp]`. A username or password an edit takes away is
    /// cleared back to the template's commented line.
    ///
    /// The file's own mode is kept, as every writer here keeps it, except that a password
    /// written into a group- or world-accessible file narrows it to 0600; a file created here
    /// is 0600. The answer says whether it tightened the file's mode: a password written into a
    /// file that group or others could read leaves it at 0600, and a front end says so once.
    pub fn update_mqtt_noting(
        path: &Path,
        edit: impl FnOnce(&mut MqttConfig),
    ) -> Result<bool, ConfigError> {
        edit_file_private(path, |doc| {
            let before = Config::parse(&doc.to_string())?.mqtt;
            let mut after = before.clone();
            edit(&mut after);
            let mut secret = false;
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
            for (key, was, now) in [
                ("username", &before.username, &after.username),
                ("password", &before.password, &after.password),
            ] {
                if was == now {
                    continue;
                }
                match now {
                    Some(text) => {
                        set(doc, "mqtt", key, Value::from(text.as_str()))?;
                        secret |= key == "password";
                    }
                    None => clear_key_at(doc, &["mqtt"], key, "")?,
                }
            }
            if before.commands != after.commands {
                set(doc, "mqtt", "commands", Value::from(after.commands))?;
            }
            Ok(secret)
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
