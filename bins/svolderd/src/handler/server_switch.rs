// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch of the web UI and the MCP server as a request: `Request::ServerSwitch`, and the
//! switch of the web UI's guest read view, `Request::GuestSwitch`.
//!
//! Placement rule: everything of these requests lives here. [`Daemon`] routes each to it in one
//! line of `handle_from`.
//!
//! The write goes through [`Daemon::edit_then_reload`]: the core's writer of the server's table
//! ([`Config::update_web`], [`Config::update_mcp`]) sets `enabled` in the file the daemon
//! reads, under the file's lock and against the file as it is then, and writes nothing when the
//! server already stands as asked (the environment's default counted: a server
//! `SVOLDER_WEB_ENABLED` turned on is turned off by writing `enabled = false`). The reload that
//! follows starts or stops the server, so a listener that was never bound is bound there and
//! nothing waits for a restart; a port that cannot be bound, or a token that cannot be had, is
//! not a refusal of the switch: the key is written and the answer's state says `failed`, as
//! `version` does.
//!
//! The answer is where both servers stand once the reload has run. A reload that outlasts the
//! write's wait goes on after the answer, which then still shows the servers as they were. Every
//! refusal reaches the socket through [`ConfigWriteError::answer`].
//!
//! The guest switch writes `[web] guest` (`"read"` or `"none"`) the same way, with
//! [`Config::update_web`], and answers the same. The web server on a section that differs only in
//! `guest` is stopped and started again by the reload, on the same address, so the mode applies
//! at once; a web UI that is off just has the key written.

use svolder_ipc::{Config, Response, Server, WebGuest};

use super::{ConfigEdit, ConfigWriteError, Daemon};

/// What a refused write calls itself: `Saving the server switch is off: …`.
const WHAT: &str = "the server switch";

impl Daemon {
    /// Answer one `server_switch` request.
    pub(super) async fn server_switch(&self, server: Server, enabled: bool) -> Response {
        let edit: ConfigEdit = match server {
            Server::Web => {
                Box::new(move |path| Config::update_web(path, |web| web.enabled = enabled))
            }
            Server::Mcp => {
                Box::new(move |path| Config::update_mcp(path, |mcp| mcp.enabled = enabled))
            }
        };
        match self.write_switch(edit).await {
            Ok(()) => Response::ServerSwitch {
                web: self.web.status(),
                mcp: self.mcp.status(),
            },
            Err(why) => why.answer(),
        }
    }

    /// Answer one `guest_switch` request: `guest = "read"` for `read`, `"none"` otherwise.
    pub(super) async fn guest_switch(&self, read: bool) -> Response {
        let guest = match read {
            true => WebGuest::Read,
            false => WebGuest::None,
        };
        let edit: ConfigEdit =
            Box::new(move |path| Config::update_web(path, |web| web.guest = guest));
        match self.write_switch(edit).await {
            Ok(()) => Response::ServerSwitch {
                web: self.web.status(),
                mcp: self.mcp.status(),
            },
            Err(why) => why.answer(),
        }
    }

    /// Make `edit` and reload, once the file is known to be writable; a daemon that is stopping
    /// refuses in `edit_then_reload`, as it does every configuration write.
    async fn write_switch(&self, edit: ConfigEdit) -> Result<(), ConfigWriteError> {
        self.config_for_write(WHAT).await?;
        self.edit_then_reload(edit).await
    }
}

#[cfg(test)]
mod tests;
