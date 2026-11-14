// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's preset library as a request: `Request::PresetWrite`.
//!
//! Each op goes through the same [`Daemon`] writers the web page's save, rename and delete use
//! (`save_preset`, `rename_preset`, `delete_preset`), so a name is judged by the core's one rule
//! and refused in the same words on every surface. A refusal reaches the socket through
//! [`ConfigWriteError::answer`].

use svolder_ipc::{PresetOp, Response};
use svolder_profiles::tone::preset;

use super::config_write::{imported_overwrite, no_such_preset};
use super::{ConfigWriteError, Daemon};

impl Daemon {
    /// Answer one `preset_write` request.
    pub(super) async fn preset_write(&self, op: PresetOp) -> Response {
        let written = match op {
            PresetOp::Save { preset, force } => self.save_over(*preset, force).await,
            PresetOp::Update { preset } => self.update_preset(*preset).await,
            PresetOp::Rename { from, to } => self.rename_preset(&from, &to).await,
            PresetOp::Delete { name } => self.delete_preset(&name).await,
        };
        match written {
            Ok(name) => Response::PresetWritten { name },
            Err(why) => why.answer(),
        }
    }

    /// Save `saved` under its own name; with `force`, over the user's preset of that name, which
    /// a curve read off a device never replaces when that preset is imported.
    async fn save_over(
        &self,
        saved: preset::Preset,
        force: bool,
    ) -> Result<String, ConfigWriteError> {
        if force && !saved.is_imported() {
            let held = self.config_for_write("presets").await?;
            if let Some(words) = imported_overwrite(&held.presets, &saved.name) {
                return Err(ConfigWriteError::Builtin(words));
            }
        }
        let name = saved.name.clone();
        self.save_preset(&name, saved, force).await
    }

    /// Write `saved` over the user's preset of its name, as [`Daemon::save_over`] does with
    /// `force`, once that preset is known to be there.
    async fn update_preset(&self, saved: preset::Preset) -> Result<String, ConfigWriteError> {
        let held = self.config_for_write("presets").await?;
        if held.preset(&saved.name).is_none() {
            return Err(no_such_preset(&saved.name));
        }
        self.save_over(saved, true).await
    }
}

#[cfg(test)]
mod tests;
