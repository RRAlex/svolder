// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writer of the `[autoeq]` table, which the TUI's settings screen saves the refresh
//! setting with.

use std::path::Path;

use toml_edit::{Item, Table, Value};

use super::edit::{edit_file, set_key_at};
use super::{AutoEqConfig, Config, ConfigError, RefreshWeeks};

impl Config {
    /// Change the `[autoeq]` table of the file at `path` with `edit`, and rewrite only the key
    /// that change alters.
    ///
    /// The table `edit` is handed is the one the file holds at this moment, read as
    /// [`Config::parse`] reads it. A key `edit` leaves alone is left alone in the text, and so
    /// is everything outside `[autoeq]`; an edit that changes nothing leaves the file byte for
    /// byte. A key the template shows commented out is written where that line stood, and a
    /// missing file starts from [`Config::template`]. A file that does not parse, or whose
    /// result would not, is left as it was and the error returned.
    pub fn update_autoeq(
        path: &Path,
        edit: impl FnOnce(&mut AutoEqConfig),
    ) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let before = Config::parse(&doc.to_string())?.autoeq;
            let mut after = before;
            edit(&mut after);
            if before == after {
                return Ok(());
            }
            let root = doc.as_table_mut();
            if !root.contains_key("autoeq") {
                root.insert("autoeq", Item::Table(Table::new()));
            }
            let value = match after.refresh_weeks {
                RefreshWeeks::Never => Value::from("never"),
                RefreshWeeks::Weeks(weeks) => Value::from(i64::from(weeks)),
            };
            set_key_at(doc, &["autoeq"], "refresh_weeks", value)
        })
    }
}
