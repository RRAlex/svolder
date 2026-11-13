// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The switch of the web UI and the MCP server as a request: `Request::ServerSwitch`.
//!
//! Placement rule: everything of this request lives here. [`Daemon`] routes
//! `Request::ServerSwitch` to it in one line of `handle_from`.
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

use svolder_ipc::{Config, Response, Server};

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

    /// Make `edit` and reload, once the file is known to be writable; a daemon that is stopping
    /// refuses in `edit_then_reload`, as it does every configuration write.
    async fn write_switch(&self, edit: ConfigEdit) -> Result<(), ConfigWriteError> {
        self.config_for_write(WHAT).await?;
        self.edit_then_reload(edit).await
    }
}

#[cfg(test)]
mod tests;
