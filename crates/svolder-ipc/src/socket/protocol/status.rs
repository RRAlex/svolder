// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The answers that describe one subsystem: readings, recalls, link changes, and the status of
//! the MQTT bridge, the MCP server and the web server.

use serde::{Deserialize, Serialize};

/// The longest span a `readings` request is answered for, in seconds: what the daemon keeps.
pub const READINGS_SPAN_SECS: u64 = 30 * 60;

/// One function's readings in a `readings` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesOut {
    /// the function id, dotted
    pub id: String,
    /// the profile key, None for a function the profile does not name
    pub key: Option<String>,
    /// how the samples in one bin were joined
    pub aggregate: svolder_profiles::history::Aggregate,
    /// `(unix milliseconds, value)`, oldest first; a bool reads 0 or 1
    pub points: Vec<(u64, f64)>,
}

/// One recall in a `readings` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallOut {
    /// when the recall started, in unix milliseconds
    pub at_ms: u64,
    /// the preset's name
    pub name: String,
}

/// One change of a device's link, as the daemon saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkChange {
    /// the daemon's wall clock at the change, in unix milliseconds
    pub at_ms: u64,
    /// the link state from then on
    pub connected: bool,
}

/// Where a daemon's MQTT bridge stands, as `version` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MqttStatus {
    /// No `[mqtt]` section turns the bridge on, so the daemon opens no connection to a broker.
    Off,
    /// Connecting to the broker, or waiting to try it again.
    Connecting {
        /// the broker's host, as the config names it
        host: String,
        /// the broker's port
        port: u16,
        /// whole seconds until the next attempt, while waiting between attempts; absent while
        /// an attempt is under way
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_in_secs: Option<u64>,
    },
    /// Connected to the broker.
    Connected {
        /// the broker's host, as the config names it
        host: String,
        /// the broker's port
        port: u16,
        /// whether the bridge takes commands back
        commands: bool,
    },
    /// Not connected, and the last attempt failed in a way that has words. A client that does
    /// not know this state reads it as `Unknown`.
    Trouble {
        /// the broker's host, as the config names it
        host: String,
        /// the broker's port
        port: u16,
        /// what went wrong
        problem: MqttProblem,
        /// whole seconds until the next attempt, while waiting between attempts
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_in_secs: Option<u64>,
    },
    /// A state this client does not know, from a daemon newer than it: read as this, so the
    /// rest of the reply still decodes.
    #[serde(other)]
    Unknown,
}

/// Why a daemon's MQTT bridge cannot reach its broker, as far as the failed attempt says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MqttProblem {
    /// The broker answered and refused the username and password.
    Refused,
    /// Nothing answered at the address.
    Unreachable,
    /// The port is 8883, the one MQTT over TLS uses, and the broker closed a plain connection.
    Tls,
    /// A problem a newer daemon names that this client does not know.
    #[serde(other)]
    Unknown,
}

impl MqttProblem {
    /// The problem in words a person can act on.
    pub fn words(self) -> &'static str {
        match self {
            MqttProblem::Refused => "the broker refused the login (check username and password)",
            MqttProblem::Unreachable => "not reachable",
            MqttProblem::Tls => "port 8883 is TLS, which svolder does not speak",
            MqttProblem::Unknown => "trouble reaching the broker",
        }
    }
}

/// Where a daemon's MCP or web token comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// The server's token variable (`SVOLDER_MCP_TOKEN` or `SVOLDER_WEB_TOKEN`) in the daemon's
    /// environment.
    Environment,
    /// The file the server's `token_file` (`[mcp]` or `[web]`) names.
    File,
    /// The file svolder generated in the daemon's state home.
    State,
}

/// What a daemon says of the token its MCP or web server uses: where it comes from and its last
/// three characters (none for a short token), which lets a front end tell whether it reads the
/// same token. Never the token itself, and never anywhere but the `version` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenInfo {
    /// the source kind
    pub kind: TokenKind,
    /// the file's path, on the daemon's side, for a file source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// the token's last three characters, or empty for a token under 16 characters
    pub tail: String,
}

/// Where a daemon's MCP server stands, as `version` reports it.
///
/// Nothing here is a secret: the address, how requests prove themselves, whether writes are
/// offered, or why the address could not be bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum McpStatus {
    /// No `[mcp]` section turns the server on, so the daemon opens no port.
    Off,
    /// Serving on `listen`.
    Listening {
        /// the address the server is bound to, with the port the system chose when the config
        /// asked for 0
        listen: std::net::SocketAddr,
        /// how a request proves who sent it
        auth: crate::config::McpAuth,
        /// whether the tools that change a device are offered
        writes: bool,
    },
    /// The server is on in the config but is not serving: the address could not be bound, or
    /// the token could not be had.
    Failed {
        /// the address the config asks for
        listen: std::net::SocketAddr,
        /// why, in words that never carry a token
        error: String,
    },
    /// A state this client does not know, from a daemon newer than it: read as this, so the
    /// rest of the reply still decodes.
    #[serde(other)]
    Unknown,
}

impl McpStatus {
    /// The status as `svolder daemon status` and the daemon's log say it: `mcp: off`;
    /// `mcp: listening on ADDR (token, read-only)`, with `no auth` for `auth = "none"` and
    /// `writes` where the write tools are offered; `mcp: cannot listen on ADDR: WHY`; or
    /// `mcp: unknown` for a state a newer daemon reports that this one does not know.
    pub fn line(&self) -> String {
        match self {
            McpStatus::Off => "mcp: off".to_string(),
            McpStatus::Listening {
                listen,
                auth,
                writes,
            } => format!(
                "mcp: listening on {listen} ({}, {})",
                match auth {
                    crate::config::McpAuth::Token => "token",
                    crate::config::McpAuth::None => "no auth",
                },
                match writes {
                    true => "writes",
                    false => "read-only",
                }
            ),
            McpStatus::Failed { listen, error } => {
                format!("mcp: cannot listen on {listen}: {error}")
            }
            McpStatus::Unknown => "mcp: unknown".to_string(),
        }
    }
}

/// Where a daemon's web UI stands, as `version` reports it. Nothing here is a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WebStatus {
    /// No `[web]` section turns it on, so the daemon opens no port.
    Off,
    /// Serving on `listen`.
    Listening {
        /// the address the server is bound to, with the port the system chose when the config
        /// asked for 0
        listen: std::net::SocketAddr,
        /// how a browser proves who it is
        auth: crate::config::WebAuth,
        /// whether the page may change a device
        writes: bool,
        /// what a visitor without a sign-in gets
        guest: crate::config::WebGuest,
    },
    /// On in the config but not serving: the address could not be bound, or the token could
    /// not be had.
    Failed {
        /// the address the config asks for
        listen: std::net::SocketAddr,
        /// why, in words that never carry a token
        error: String,
    },
    /// A state this client does not know, from a daemon newer than it.
    #[serde(other)]
    Unknown,
}

impl WebStatus {
    /// The status as `svolder daemon status` and the daemon's log say it: `web: off`;
    /// `web: listening on ADDR (token, writes)`, with `no auth` for `auth = "none"`,
    /// `read-only` where writes are off and `, guest read` where guests may look;
    /// `web: cannot listen on ADDR: WHY`; or `web: unknown`.
    pub fn line(&self) -> String {
        match self {
            WebStatus::Off => "web: off".to_string(),
            WebStatus::Listening {
                listen,
                auth,
                writes,
                guest,
            } => format!(
                "web: listening on {listen} ({}, {}{})",
                match auth {
                    crate::config::WebAuth::Token => "token",
                    crate::config::WebAuth::None => "no auth",
                },
                match writes {
                    true => "writes",
                    false => "read-only",
                },
                match guest {
                    crate::config::WebGuest::Read => ", guest read",
                    crate::config::WebGuest::None => "",
                }
            ),
            WebStatus::Failed { listen, error } => {
                format!("web: cannot listen on {listen}: {error}")
            }
            WebStatus::Unknown => "web: unknown".to_string(),
        }
    }
}
