// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder web on|off`, `svolder mcp on|off` and `svolder web guest on|off`: turn the daemon's
//! web UI or its MCP server on or off, or the web UI's guest read view, for a script and for a
//! first run in a container.
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
//! The guest verb is the same switch for `[web] guest` (`Request::GuestSwitch`): a daemon writes
//! `guest = "read"` or `"none"` and reloads, and the verb prints the web UI's status line, which
//! names the guest mode. The web UI being off is not a failure: the key is written and a note
//! says the page is off.
//!
//! The line goes to stdout and every note to stderr. The verb succeeds only when the server
//! stands as asked: one that cannot listen, and one the daemon's answer still shows as it was
//! (its reload failed or outlasted the write's wait), fail it, so a script sees either.

use std::path::Path;
use std::process::ExitCode;

use svolder_ipc::socket::daemon_version::{VERSION_WAIT, version_on};
use svolder_ipc::{
    Client, Config, FEATURE_GUEST_SWITCH, FEATURE_SERVER_SWITCH, McpStatus, Request, Response,
    Server, WebAuth, WebGuest, WebStatus, socket,
};

use crate::backend::Globals;
use crate::recovery::{Failure, Kind};

/// What a switch came to: the line for stdout and the notes for stderr, in order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Done {
    pub(crate) line: String,
    pub(crate) notes: Vec<String>,
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
pub(crate) enum Failed {
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
    let (socket, own) = places(g)?;
    finish(switch(&socket, &own, server, enabled).await)
}

/// Carry out `web guest on|off`.
pub(crate) async fn run_guest(g: &Globals<'_>, read: bool) -> Result<ExitCode, String> {
    let (socket, own) = places(g)?;
    finish(guest_switch(&socket, &own, read).await)
}

/// The daemon's socket and this side's own configuration file.
pub(crate) fn places(g: &Globals<'_>) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    Ok((
        socket::path::resolve(g.socket, config.socket.as_deref()),
        Config::path(),
    ))
}

/// Print what a switch came to: the line on stdout, the notes on stderr, or the failure with the
/// exit code it has.
pub(crate) fn finish(done: Result<Done, Failed>) -> Result<ExitCode, String> {
    let done = match done {
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

/// Ask the daemon on `socket` the `request` that needs `feature`: `Ok(None)` when no daemon
/// answers there (the caller writes its own file), else where the two servers stand after it.
/// `what` finishes the sentence of a daemon too old to take it: "cannot" and `what`.
async fn through_daemon(
    socket: &Path,
    feature: &str,
    what: &str,
    request: Request,
) -> Result<Option<(WebStatus, McpStatus)>, Failed> {
    match ask_daemon(socket, feature, what, request).await? {
        None => Ok(None),
        Some(Response::ServerSwitch { web, mcp }) => Ok(Some((web, mcp))),
        Some(other) => Err(format!("the daemon answered {other:?}").into()),
    }
}

/// Ask the daemon on `socket` the `request` that needs `feature`: `Ok(None)` when no daemon
/// answers there (the caller writes its own file), else its answer, which is never an error
/// response (that is `Err` with the daemon's words).
pub(crate) async fn ask_daemon(
    socket: &Path,
    feature: &str,
    what: &str,
    request: Request,
) -> Result<Option<Response>, Failed> {
    let mut client = match Client::connect_if_any(socket).await {
        Ok(Some(client)) => client,
        Ok(None) => return Ok(None),
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
    if !info.supports(feature) {
        return Err(Failed::TooOld(Failure::of(
            Kind::DaemonTooOld,
            crate::reach::too_old_words(Some(&info.version), what),
        )));
    }
    match client.request(request).await {
        Ok(Response::Error { message, .. }) => Err(message.into()),
        Ok(answer) => Ok(Some(answer)),
        Err(e) => Err(e.to_string().into()),
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
    let request = Request::ServerSwitch { server, enabled };
    let Some((web, mcp)) =
        through_daemon(socket, FEATURE_SERVER_SWITCH, "switch its servers", request).await?
    else {
        return alone(socket, own_config, server, enabled).map_err(Failed::from);
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

/// The guest switch over a given socket and a given configuration file of this side's own:
/// through the daemon on `socket` when one answers, else written to `own_config`.
///
/// The daemon writes and reloads, or this side writes `guest` itself. The TUI sends the same
/// request through `ConnChange::WebGuest` (`tui/run/connect.rs`) and writes the file through
/// [`Config::update_web`] when no daemon runs.
pub(crate) async fn guest_switch(
    socket: &Path,
    own_config: &Path,
    read: bool,
) -> Result<Done, Failed> {
    let request = Request::GuestSwitch { read };
    let Some((web, _)) = through_daemon(
        socket,
        FEATURE_GUEST_SWITCH,
        "switch the guest view",
        request,
    )
    .await?
    else {
        return guest_alone(socket, own_config, read)
            .await
            .map_err(Failed::from);
    };
    let line = web.line();
    let asked = guest_word(read);
    let mut notes = Vec::new();
    if read
        && matches!(
            &web,
            WebStatus::Listening {
                auth: WebAuth::None,
                ..
            }
        )
    {
        notes.push(NO_GUEST_WITHOUT_SIGN_IN.to_string());
    }
    let written = format!("[web] guest = \"{asked}\" is written");
    match web {
        WebStatus::Listening { guest, .. } if guest == guest_of(read) => Ok(Done { line, notes }),
        WebStatus::Off => {
            notes.push(format!(
                "{written}; the web UI is off, so no one sees it until `svolder web on`"
            ));
            Ok(Done { line, notes })
        }
        WebStatus::Failed { .. } => Err(format!(
            "{line} ({written}; `svolder web guest {}` takes it back)",
            word(!read)
        )
        .into()),
        _ => Err(format!(
            "{line} ({written} and the daemon has not acted on it: svolder daemon status)"
        )
        .into()),
    }
}

/// What `guest on` says where there is no sign-in to be a guest of.
const NO_GUEST_WITHOUT_SIGN_IN: &str =
    "web.auth = \"none\": every visitor is signed in, so the guest view changes nothing";

/// The guest mode `read` asks for.
fn guest_of(read: bool) -> WebGuest {
    match read {
        true => WebGuest::Read,
        false => WebGuest::None,
    }
}

/// The guest mode's word in the file.
fn guest_word(read: bool) -> &'static str {
    match read {
        true => "read",
        false => "none",
    }
}

/// The guest switch with no daemon on `socket`: the key written to `own_config` by this side.
///
/// A daemon may have started since the socket was tried and read the file before this write, so a
/// daemon that reads this file is told to reload: `guest off` must reach it.
async fn guest_alone(socket: &Path, own_config: &Path, read: bool) -> Result<Done, String> {
    Config::update_web(own_config, |web| web.guest = guest_of(read)).map_err(|e| e.to_string())?;
    crate::backend::reload_if_reads(socket.to_path_buf(), own_config).await;
    let mut notes = Vec::new();
    if read && Config::load_from(own_config).is_ok_and(|c| c.web.auth == WebAuth::None) {
        notes.push(NO_GUEST_WITHOUT_SIGN_IN.to_string());
    }
    notes.push(format!(
        "nothing answers on {}: the guest mode applies with the next daemon",
        socket.display()
    ));
    Ok(Done {
        line: format!("web guest: {} in {}", word(read), own_config.display()),
        notes,
    })
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
