// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder web on|off` and `svolder mcp on|off`: turn the daemon's web UI or its MCP server on
//! or off, for a script and for a first run in a container.
//!
//! A daemon that answers makes the switch (`Request::ServerSwitch`): it writes `enabled` in the
//! file it reads and reloads, which starts or stops the server, and the verb prints where the
//! server then stands, the line `svolder daemon status` prints for it. This side writes nothing
//! while a daemon runs, and a daemon too old to have the request is not written around: the
//! verb says to restart it. With no daemon the verb writes the key in this side's own file,
//! and the server follows it when a daemon next starts.
//!
//! No daemon means no socket, or one nothing listens on. A socket that cannot be tried (not
//! this user's to open, a path too long) may have a daemon behind it, so the verb fails naming
//! it and writes nothing.
//!
//! The line goes to stdout and every note to stderr. The verb succeeds only when the server
//! stands as asked: one that cannot listen, and one the daemon's answer still shows as it was
//! (its reload failed or outlasted the write's wait), fail it, so a script sees either.

use std::path::Path;
use std::process::ExitCode;

use svolder_ipc::socket::daemon_version::{VERSION_WAIT, version_on};
use svolder_ipc::{
    Client, Config, FEATURE_SERVER_SWITCH, McpStatus, Request, Response, Server, WebStatus, socket,
};

use crate::backend::Globals;
use crate::recovery::{Failure, Kind};

/// What a switch came to: the line for stdout and the notes for stderr, in order.
#[derive(Debug, PartialEq, Eq)]
struct Done {
    line: String,
    notes: Vec<String>,
}

/// Where a server stands after a switch, as far as the verb judges it.
enum Stands {
    On,
    Off,
    Failed,
}

/// Why a switch did not come about: in words, or because the daemon is older than this svolder
/// (which has its own exit code).
#[derive(Debug)]
enum Failed {
    /// Said as it is, exit 1.
    Words(String),
    /// A daemon without the request: said with the step that follows, exit 7.
    TooOld(Failure),
}

impl From<String> for Failed {
    fn from(words: String) -> Failed {
        Failed::Words(words)
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::Words(words) => f.write_str(words),
            Failed::TooOld(failure) => write!(f, "{failure}"),
        }
    }
}

/// Carry out `web on|off` or `mcp on|off`.
pub(crate) async fn run(
    g: &Globals<'_>,
    server: Server,
    enabled: bool,
) -> Result<ExitCode, String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let socket = socket::path::resolve(g.socket, config.socket.as_deref());
    let done = match switch(&socket, &Config::path(), server, enabled).await {
        Ok(done) => done,
        Err(Failed::TooOld(failure)) => {
            eprintln!("svolder: {failure}");
            return Ok(ExitCode::from(failure.code));
        }
        Err(Failed::Words(words)) => return Err(words),
    };
    println!("{}", done.line);
    for note in done.notes {
        eprintln!("svolder: {note}");
    }
    Ok(ExitCode::SUCCESS)
}

/// What the server is called in a sentence.
fn called(server: Server) -> &'static str {
    match server {
        Server::Web => "the web UI",
        Server::Mcp => "the MCP server",
    }
}

/// `on` or `off`.
fn word(enabled: bool) -> &'static str {
    match enabled {
        true => "on",
        false => "off",
    }
}

/// The switch over a given socket and a given configuration file of this side's own: through
/// the daemon on `socket` when one answers, else written to `own_config`.
async fn switch(
    socket: &Path,
    own_config: &Path,
    server: Server,
    enabled: bool,
) -> Result<Done, Failed> {
    let mut client = match Client::connect_if_any(socket).await {
        Ok(Some(client)) => client,
        Ok(None) => return alone(socket, own_config, server, enabled).map_err(Failed::from),
        // A daemon may run behind a socket that cannot be tried: writing the file here would
        // answer success while that daemon never acts.
        Err(why) => {
            return Err(format!(
                "cannot reach the daemon's socket {}: {why}",
                socket.display()
            )
            .into());
        }
    };
    let info = match tokio::time::timeout(VERSION_WAIT, version_on(&mut client)).await {
        Ok(Ok(reply)) => reply.info,
        Ok(Err(_)) | Err(_) => {
            return Err(format!(
                "the daemon on {} did not say what it is: svolder daemon status",
                socket.display()
            )
            .into());
        }
    };
    if !info.supports(FEATURE_SERVER_SWITCH) {
        return Err(Failed::TooOld(Failure::of(
            Kind::DaemonTooOld,
            crate::reach::too_old_words(Some(&info.version), "switch its servers"),
        )));
    }
    let request = Request::ServerSwitch { server, enabled };
    let (web, mcp) = match client.request(request).await {
        Ok(Response::ServerSwitch { web, mcp }) => (web, mcp),
        Ok(Response::Error { message, .. }) => return Err(message.into()),
        Ok(other) => return Err(format!("the daemon answered {other:?}").into()),
        Err(e) => return Err(e.to_string().into()),
    };
    let (line, stands) = match server {
        Server::Web => (
            web.line(),
            match web {
                WebStatus::Listening { .. } => Stands::On,
                WebStatus::Failed { .. } => Stands::Failed,
                WebStatus::Off | WebStatus::Unknown => Stands::Off,
            },
        ),
        Server::Mcp => (
            mcp.line(),
            match mcp {
                McpStatus::Listening { .. } => Stands::On,
                McpStatus::Failed { .. } => Stands::Failed,
                McpStatus::Off | McpStatus::Unknown => Stands::Off,
            },
        ),
    };
    let table = server.table();
    let written = format!("[{table}] enabled = {enabled} is written");
    // One rule: the verb succeeds only when the server stands as asked.
    match (stands, enabled) {
        (Stands::On, true) | (Stands::Off, false) => Ok(Done {
            line,
            notes: Vec::new(),
        }),
        (Stands::Failed, true) => {
            Err(format!("{line} ({written}; `svolder {table} off` takes it back)").into())
        }
        _ => Err(format!(
            "{line} ({written} and the daemon has not acted on it: svolder daemon status)"
        )
        .into()),
    }
}

/// The switch with no daemon on `socket`: the key written to `own_config` by this side.
fn alone(socket: &Path, own_config: &Path, server: Server, enabled: bool) -> Result<Done, String> {
    match server {
        Server::Web => Config::update_web(own_config, |web| web.enabled = enabled),
        Server::Mcp => Config::update_mcp(own_config, |mcp| mcp.enabled = enabled),
    }
    .map_err(|e| e.to_string())?;
    let nothing = format!("nothing answers on {}", socket.display());
    let note = match enabled {
        true => format!("{nothing}: {} starts with the next daemon", called(server)),
        false => nothing,
    };
    Ok(Done {
        line: format!(
            "{}: {} in {}",
            server.table(),
            word(enabled),
            own_config.display()
        ),
        notes: vec![note],
    })
}

#[cfg(test)]
mod tests;
