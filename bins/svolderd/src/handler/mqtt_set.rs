// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The change of one `[mqtt]` key as a request: `Request::MqttSet`.
//!
//! Placement rule: everything of this request lives here. [`Daemon`] routes it to
//! [`Daemon::mqtt_set`] in one line of `handle_from`.
//!
//! The write goes through [`Daemon::edit_then_reload`], as the server switch's does:
//! [`Config::update_mqtt_noting`] changes the one key in the file the daemon reads, under the
//! file's lock and against the file as it is then, and writes nothing when the key already
//! stands as asked. The reload that follows restarts the bridge on the new section. The answer is where
//! the bridge stands once the reload has run, which for a bridge that has only just started is
//! `connecting`: the broker's verdict comes a moment later, in `version`.
//!
//! The password is written to the file and nowhere else: it is not logged, not echoed and not in
//! the answer.

use svolder_ipc::{Config, MqttOp, Response};

use super::{ConfigEdit, Daemon};

/// What a refused write calls itself: `Saving the MQTT settings is off: …`.
const WHAT: &str = "the MQTT settings";

impl Daemon {
    /// Answer one `mqtt_set` request.
    pub(super) async fn mqtt_set(&self, op: MqttOp) -> Response {
        if let Err(why) = self.config_for_write(WHAT).await {
            return why.answer();
        }
        let tightened = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let said = std::sync::Arc::clone(&tightened);
        let edit: ConfigEdit = Box::new(move |path| {
            let narrowed = Config::update_mqtt_noting(path, |mqtt| match op {
                MqttOp::Enabled { enabled } => mqtt.enabled = enabled,
                MqttOp::Commands { commands } => mqtt.commands = commands,
                MqttOp::Host { value } => mqtt.host = Some(value),
                MqttOp::Port { value } => mqtt.port = value,
                MqttOp::Username { value } => mqtt.username = value,
                MqttOp::Password { value } => mqtt.password = value,
            })?;
            if narrowed {
                said.store(true, std::sync::atomic::Ordering::SeqCst);
                crate::log::info(&format!(
                    "{} was readable by others: a password was written, so it is now 0600",
                    path.display()
                ));
            }
            Ok(())
        });
        match self.edit_then_reload(edit).await {
            Ok(()) => Response::MqttSet {
                status: self.mqtt_status(),
                tightened: tightened.load(std::sync::atomic::Ordering::SeqCst),
            },
            Err(why) => why.answer(),
        }
    }
}

#[cfg(test)]
mod tests;
