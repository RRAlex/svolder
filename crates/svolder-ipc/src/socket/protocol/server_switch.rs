// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch of the daemon's two servers as a request: `Request::ServerSwitch`.
//!
//! While a daemon runs it is the one writer of `[web] enabled` and `[mcp] enabled` for the front
//! ends that send this request (`svolder web on|off`, `svolder mcp on|off`): it writes the key
//! in the file it reads and reloads, and the reload starts or stops the server, binding a
//! listener that was never bound, so nothing waits for a restart. A daemon that has the request
//! lists the feature [`FEATURE_SERVER_SWITCH`](super::FEATURE_SERVER_SWITCH).
//!
//! It is answered with [`Response::ServerSwitch`](super::Response::ServerSwitch), where both
//! servers stand after the reload, or with an error whose [`Reason`](super::Reason) is
//! `invalid` (the file would not parse with the key written), `read_only`, `file_busy` or
//! `stopping`.
//!
//! `Request::GuestSwitch { read }` is the same switch for `[web] guest`: `read = true` writes
//! `guest = "read"`, `false` writes `guest = "none"`, and the reload makes the running web
//! server serve the new mode. It is a request of its own, listed as
//! [`FEATURE_GUEST_SWITCH`](super::FEATURE_GUEST_SWITCH), so a daemon without it answers an
//! unknown kind and never reads a `guest` field as part of a server switch. Its answer and
//! refusals are the server switch's; the web state in the answer carries the guest mode.

use serde::{Deserialize, Serialize};

/// One of the daemon's two servers, as a `server_switch` request names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Server {
    /// The web UI, the `[web]` table.
    Web,
    /// The MCP server, the `[mcp]` table.
    Mcp,
}

impl Server {
    /// The server's table in the configuration file, which is also the word of its CLI verb.
    pub fn table(self) -> &'static str {
        match self {
            Server::Web => "web",
            Server::Mcp => "mcp",
        }
    }
}
