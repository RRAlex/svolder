// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The change of one `[mqtt]` key as a request: `Request::MqttSet`.
//!
//! While a daemon runs it is the one writer of the file it reads for the front ends that send
//! this request (`svolder mqtt on|off`, `svolder mqtt set …`, `svolder mqtt password`): it writes
//! the one key and reloads, and the reload restarts the bridge on the new section. A daemon that
//! has the request lists the feature [`FEATURE_MQTT_SET`](super::FEATURE_MQTT_SET).
//!
//! It is answered with [`Response::MqttSet`](super::Response::MqttSet), where the bridge stands
//! after the reload, or with an error whose [`Reason`](super::Reason) is `invalid` (the file
//! would not parse with the key written, such as a bridge on with no host), `read_only`,
//! `file_busy` or `stopping`.
//!
//! The password travels in this request and nowhere else: no answer, event or log line carries
//! it, and its `Debug` reads `***`. The request goes over the daemon's own socket, which only its
//! user can open.

use serde::{Deserialize, Serialize};

/// One change of the `[mqtt]` table, as an `mqtt_set` request carries it.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MqttOp {
    /// Turn the bridge on or off.
    Enabled {
        /// on or off
        enabled: bool,
    },
    /// Let the bridge take commands back, or keep it read-only.
    Commands {
        /// whether commands are taken
        commands: bool,
    },
    /// Set the broker's host.
    Host {
        /// the host
        value: String,
    },
    /// Set the broker's port.
    Port {
        /// the port
        value: u16,
    },
    /// Set the broker's username, or take it away with `null`.
    Username {
        /// the username
        value: Option<String>,
    },
    /// Set the broker's password, or take it away with `null`.
    Password {
        /// the password
        value: Option<String>,
    },
}

impl std::fmt::Debug for MqttOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttOp::Enabled { enabled } => {
                f.debug_struct("Enabled").field("enabled", enabled).finish()
            }
            MqttOp::Commands { commands } => f
                .debug_struct("Commands")
                .field("commands", commands)
                .finish(),
            MqttOp::Host { value } => f.debug_struct("Host").field("value", value).finish(),
            MqttOp::Port { value } => f.debug_struct("Port").field("value", value).finish(),
            MqttOp::Username { value } => f.debug_struct("Username").field("value", value).finish(),
            MqttOp::Password { value } => f
                .debug_struct("Password")
                .field("value", &value.as_ref().map(|_| crate::config::REDACTED))
                .finish(),
        }
    }
}
